# RPM Repository (rpm-md) 저장·검색 설계 문서

## 1. 목적
본 문서는 **Rust 기반** 환경에서 **rpm-md 형식의 RPM Repository 메타데이터**를 수집·저장·검색하기 위한 설계를 정의한다.
검색은 **정확 검색(구조적)** 과 **자연어 검색(semantic)** 을 병행하며, 전체 시스템은 **로컬/오프라인 배포**를 전제로 한다.

### 기술 스택
- Language: Rust
- Embedding: candle + all-MiniLM-L6-v2 (384 dim)
- Vector Store: SQLite + sqlite-vec
- Metadata Store: SQLite

---

## 2. 범위

### In-Scope
- rpm-md (repomd.xml, primary.xml, filelists.xml, other.xml) 파싱
- RPM 패키지 메타데이터 저장
- 자연어 기반 패키지 검색
- Requires / Provides 조건 기반 필터링
- 단일 노드 / 로컬 실행

### Out-of-Scope
- RPM 바이너리 다운로드 / 설치
- Distributed / Cluster 환경
- 실시간 repo mirroring

---

## 3. 전체 아키텍처

```
[rpm-md XML]
      │
      ▼
[rpm-md Parser]
      │
      ▼
[Metadata Normalizer]
      │
      ├───────────────┐
      ▼               ▼
[SQLite Metadata]   [sqlite-vec]
  (정형 검색)        (Vector 검색)
      ▲               ▲
      └──── Query Planner ────┘
              │
              ▼
        Search API / MCP Tool
```

---

## 4. 데이터 모델

### 4.1 packages 테이블

```sql
CREATE TABLE packages (
  pkg_id      INTEGER PRIMARY KEY,
  name        TEXT NOT NULL,
  epoch       INTEGER,
  version     TEXT,
  release     TEXT,
  arch        TEXT,
  summary     TEXT,
  description TEXT,
  repo        TEXT
);
```

---

### 4.2 requires / provides 테이블

```sql
CREATE TABLE requires (
  pkg_id  INTEGER,
  name    TEXT,
  flags   TEXT,
  version TEXT
);

CREATE TABLE provides (
  pkg_id  INTEGER,
  name    TEXT,
  flags   TEXT,
  version TEXT
);
```

---

### 4.3 files 테이블 (선택)

```sql
CREATE TABLE files (
  pkg_id INTEGER,
  path   TEXT
);
```

> 대규모 repo에서는 비활성화 가능

---

## 5. Vector 인덱싱 설계

### 5.1 Embedding 대상 선정

Vector 인덱싱은 **의미 요약 레벨**로 제한한다.

```text
<name>
<summary>
<description>
Provides: ...
Requires: ...
```

- 1 package = 1 vector
- header / 소스 파일 단위 embedding은 제외

---

### 5.2 sqlite-vec 스키마

```sql
CREATE VIRTUAL TABLE pkg_embedding USING vec0(
  embedding FLOAT[384]
);
```

- `rowid == packages.pkg_id`

---

### 5.3 Embedding 파이프라인

```
for package in packages:
    text = build_embedding_text(package)
    vec  = MiniLM.embed(text)
    sqlite_vec.insert(pkg_id, vec)
```

- embedding 계산은 초기 인덱싱 시 1회

---

## 6. 검색 설계

### 6.1 질의 유형

| 질의 유형 | 처리 방식 |
|----------|-----------|
| name / arch | SQLite WHERE |
| version 조건 | SQLite WHERE |
| requires / provides | SQLite JOIN |
| 자연어 질의 | Vector Search |
| 혼합 질의 | Vector → SQL Filter |

---

### 6.2 자연어 검색 흐름

```
User Query
  ↓
MiniLM Embedding
  ↓
sqlite-vec KNN
  ↓
Top-N pkg_id
  ↓
SQLite metadata filter
```

예시:
> "glibc 2.34 이상 필요 없는 패키지"

1. Vector search → 후보 패키지
2. requires 테이블에서 glibc 조건 제외

---

## 7. 성능 고려사항

- MiniLM (384 dim)
- 패키지 수 10k ~ 100k: sqlite-vec 충분
- 검색 latency: 수 ms ~ 수십 ms
- 메모리 사용량 낮음 (디스크 기반)

---

## 8. 배포 전략

- Rust 단일 바이너리
- SQLite DB 파일 1개
- embedding 모델 로컬 파일로 번들링
- 완전 오프라인 실행 가능

---

## 9. 확장 포인트

- repo / arch 별 namespace
- chroot / sysroot 연계
- ctags / tree-sitter 결과를 pkg_id와 연결

---

## 10. 요약

본 설계는 다음 요구사항을 만족한다:
- 로컬 / 오프라인 동작
- rpm-md 메타데이터 정확 검색
- 자연어 기반 패키지 탐색
- 경량 배포 (SQLite + MiniLM)

Vector 인덱스는 **탐색 가이드**, SQLite는 **정확성 보장** 역할을 수행한다.
