# RPM Repository Vector Search – 상세 설계 문서

본 문서는 `rpm_repo_vector_search_design.md`의 상위 설계를 기반으로 한 **구현 지향 상세 설계서**이다.
Rust + candle + MiniLM + SQLite + sqlite-vec 조합을 전제로 하며, 실제 코드 구조·데이터 흐름·쿼리 전략까지 정의한다.

---

## 1. 설계 목표 (Design Goals)

1. **완전 로컬 / 오프라인 동작**
2. **정확성 우선 (RPM 메타데이터 규칙 준수)**
3. **자연어 탐색은 길 찾기 역할로 제한**
4. **구현 복잡도 최소화 (단일 바이너리)**
5. **MCP / LSP / CLI 에서 재사용 가능**

---

## 2. 모듈 상세 구조

```
src/
 ├─ main.rs
 ├─ config.rs
 ├─ error.rs
 ├─ repomd/
 │   ├─ mod.rs
 │   ├─ fetch.rs        # file://, http:// (선택)
 │   ├─ parser.rs       # repomd.xml, primary.xml 파싱
 │   └─ model.rs        # Raw RPM metadata structs
 │
 ├─ normalize/
 │   ├─ mod.rs
 │   └─ package.rs     # rpm-md → 내부 Package 모델
 │
 ├─ embedding/
 │   ├─ mod.rs
 │   ├─ model.rs       # MiniLM 로딩
 │   └─ embed.rs       # 텍스트 → Vec<f32>
 │
 ├─ storage/
 │   ├─ mod.rs
 │   ├─ schema.rs      # SQLite DDL
 │   ├─ sqlite.rs      # CRUD
 │   └─ vector.rs      # sqlite-vec wrapper
 │
 ├─ search/
 │   ├─ mod.rs
 │   ├─ planner.rs     # Query Planner (중심)
 │   ├─ semantic.rs    # vector search
 │   └─ structured.rs  # SQL filter search
 │
 └─ api/
     ├─ mod.rs
     └─ search.rs      # MCP / CLI 공통 API
```

---

## 3. 내부 데이터 모델

### 3.1 정규화 Package 모델

```rust
struct Package {
    pkg_id: i64,
    name: String,
    epoch: Option<i64>,
    version: String,
    release: String,
    arch: String,
    summary: String,
    description: String,
    repo: String,
    requires: Vec<Dependency>,
    provides: Vec<Dependency>,
}

struct Dependency {
    name: String,
    flags: Option<String>, // >=, <=, =
    version: Option<String>,
}
```

---

## 4. rpm-md 파싱 상세

### 4.1 입력 데이터

- repomd.xml
- primary.xml(.gz)
- filelists.xml(.gz) (선택)
- other.xml(.gz) (changelog)

### 4.2 파싱 전략

- streaming XML 파서 사용 (quick-xml)
- primary.xml 기준으로 패키지 엔트리 생성
- requires / provides 는 name + flags + version 그대로 유지

> ⚠️ RPM version 비교 로직은 **저장 단계에서는 하지 않음**

---

## 5. Embedding 상세 설계

### 5.1 Embedding Text Builder

```text
Package: <name>
Summary: <summary>
Description:
<description>
Provides: <p1>, <p2>, ...
Requires: <r1>, <r2>, ...
```

- 순수 자연어 + 최소한의 구조 태그
- version 숫자는 제거하지 않음 (검색 힌트)

---

### 5.2 MiniLM 모델 운용

- 모델: all-MiniLM-L6-v2
- 차원: 384
- candle-transformers 사용
- CPU inference only

### 5.3 배치 전략

- 16~32 패키지 단위 batch embedding
- 인덱싱 시에만 수행

---

## 6. SQLite / sqlite-vec 상세

### 6.1 Schema 초기화

```sql
-- metadata
CREATE TABLE packages (...);
CREATE TABLE requires (...);
CREATE TABLE provides (...);

-- vector
CREATE VIRTUAL TABLE pkg_embedding USING vec0(
  embedding FLOAT[384]
);
```

### 6.2 Insert 규칙

1. packages insert → pkg_id 확보
2. requires / provides insert
3. pkg_embedding rowid = pkg_id

---

## 7. Query Planner (핵심 설계)

### 7.1 Query 분류

| 유형 | 예 | 처리 |
|----|----|----|
| Name | "openssl" | SQL |
| 조건 | "glibc >= 2.34" | SQL |
| 의미 | "암호화 라이브러리" | Vector |
| 혼합 | "glibc 필요없는 네트워크 패키지" | Vector → SQL |

---

### 7.2 Planner 처리 흐름

```
User Query
  ↓
Lightweight Rule Parser
  ├─ dependency 조건 추출
  └─ 나머지 텍스트 → semantic query
        ↓
Vector Search (Top-N)
        ↓
SQL Filtering (requires/provides/version)
        ↓
Final Result
```

---

### 7.3 Vector Search 상세

```sql
SELECT rowid, distance
FROM pkg_embedding
ORDER BY embedding <-> :query_vec
LIMIT 50;
```

---

### 7.4 Dependency 필터 예시

> "glibc 2.34 이상 필요 없는 패키지"

```sql
SELECT * FROM packages
WHERE pkg_id IN (:candidate_ids)
AND pkg_id NOT IN (
  SELECT pkg_id FROM requires
  WHERE name = 'glibc'
  AND flags = '>='
  AND version >= '2.34'
);
```

> ⚠️ version 비교는 문자열 비교가 아닌 rpmvercmp 필요

---

## 8. 성능 및 한계

- Vector 검색은 **후보 축소용**
- 정확성은 SQL이 보장
- sqlite-vec는 100k 패키지까지 충분
- 수백만 패키지는 scope 아님

---

## 9. 에러 처리 / 복구

- rpm-md 파싱 오류 → repo 단위 skip
- embedding 실패 → vector 없이 metadata만 저장
- DB corruption → full reindex 권장

---

## 10. 확장 설계 (Non-Blocking)

- chroot/sysroot 연계 시 `repo` 컬럼 활용
- header / source 요약 vector 추가 가능
- ctags / tree-sitter 결과를 pkg_id와 연결
- 향후 Qdrant 전환 가능 (schema 유지)

---

## 11. 설계 요약

- Vector DB는 **검색 가이드** 역할
- SQLite는 **진실의 근원(Source of Truth)**
- rpm 생태계 특성에 맞춘 최소·정확 설계
- MCP / LSP / CLI 어디에도 재사용 가능

이 설계는 **과도한 AI 의존 없이 실용성 중심**으로 작성되었다.
