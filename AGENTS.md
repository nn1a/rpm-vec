# Agent Guidelines - RPM Repository Vector Search

이 문서는 이 프로젝트에서 작업하는 AI 에이전트를 위한 가이드라인입니다.

## 프로젝트 개요

**RPM Repository Vector Search**는 Rust로 작성된 RPM 저장소 메타데이터 검색 시스템입니다.
- **목적**: rpm-md 형식의 메타데이터를 파싱하고 검색 기능 제공
- **핵심 기술**: Rust, SQLite, Candle, Vector Embeddings
- **배포 방식**: 단일 바이너리, 완전 오프라인 동작

## 프로젝트 구조

```
rpm-vec/
├── Cargo.toml              # 프로젝트 설정 및 의존성
├── README.md               # 프로젝트 메인 문서
├── AGENTS.md              # 이 파일 - 에이전트 가이드라인
│
├── src/                    # 소스 코드
│   ├── main.rs            # CLI 진입점
│   ├── config.rs          # 설정 관리
│   ├── error.rs           # 에러 타입 정의
│   ├── repomd/            # RPM 메타데이터 파싱
│   ├── normalize/         # 데이터 정규화
│   ├── storage/           # SQLite 스토리지
│   ├── embedding/         # 벡터 임베딩
│   ├── search/            # 검색 엔진
│   └── api/               # 공개 API
│
├── tests/                  # 통합 테스트
│   └── test_compression.rs
│
└── docs/                   # 문서
    ├── USAGE.md           # 사용 가이드
    ├── DEVELOPMENT.md     # 개발 문서
    ├── COMPRESSION.md     # 압축 형식 가이드
    ├── CHANGELOG.md       # 변경 이력
    └── design/            # 설계 문서
        ├── rpm_repo_vector_search_design.md
        └── rpm_repo_vector_search_detailed_design.md
```

## 코딩 가이드라인

### 1. Rust 스타일
- **공식 스타일 가이드 준수**: `rustfmt`와 `clippy` 권장사항 따르기
- **에러 처리**: `Result<T, RpmSearchError>` 사용, `unwrap()` 지양
- **문서화**: 공개 API는 반드시 문서 주석 작성

```rust
/// Package metadata from RPM repository
/// 
/// # Examples
/// 
/// ```
/// let package = Package::from_rpm_package(rpm_pkg, "repo".to_string());
/// ```
pub struct Package { ... }
```

### 2. 명명 규칙
- **모듈**: snake_case (`repomd`, `normalize`)
- **구조체/열거형**: PascalCase (`Package`, `RpmSearchError`)
- **함수/변수**: snake_case (`build_embedding_text`, `pkg_id`)
- **상수**: SCREAMING_SNAKE_CASE (`SCHEMA_VERSION`)

### 3. 모듈 구조
각 기능은 독립적인 모듈로 분리:
- `mod.rs`: 모듈 공개 인터페이스
- 구현 파일: 구체적 로직
- 모듈 간 의존성 최소화

### 4. Feature Flags
Embedding, MCP, Sync 기능은 기본 빌드에 포함됩니다.
하드웨어 가속만 optional feature로 제공:
```toml
[features]
default = []

# 하드웨어 가속 (optional)
accelerate = ["candle-core/accelerate", ...]  # Apple Accelerate (macOS 권장)
cuda = ["candle-core/cuda", "cudarc", ...]    # NVIDIA GPU
```

**하드웨어 가속:**
- macOS: `accelerate` feature 권장 (Apple Accelerate framework)
- Linux + NVIDIA GPU: `cuda` feature

## 개발 워크플로우

### 새 기능 추가
1. **설계 검토**: `docs/design/` 문서 확인
2. **모듈 선택**: 적절한 모듈에 구현 (또는 새 모듈 생성)
3. **타입 정의**: 필요한 struct/enum 정의
4. **구현**: 핵심 로직 작성
5. **에러 처리**: `RpmSearchError`에 새 에러 타입 추가
6. **테스트**: 단위 테스트 작성
7. **문서화**: API 문서 및 사용 예제 추가

### 버그 수정
1. **재현**: 실패 케이스 테스트 작성
2. **디버깅**: `RUST_LOG=debug` 사용
3. **수정**: 최소 변경으로 해결
4. **검증**: 테스트 통과 확인
5. **회귀 방지**: 테스트 유지

### 성능 최적화
1. **측정**: 먼저 벤치마크로 병목 확인
2. **최적화**: 측정된 병목만 개선
3. **검증**: 벤치마크로 개선 확인
4. **트레이드오프**: 가독성 vs 성능 균형

## 주요 개념

### 1. RPM Metadata 처리
```
rpm-md XML → Parser → RpmPackage → Normalizer → Package → Storage
```

- **Parser**: `quick-xml`로 스트리밍 파싱
- **Normalizer**: RPM 특화 → 내부 모델 변환
- **Storage**: SQLite에 구조화 저장

### 2. 압축 지원
현재 지원: `.gz` (Gzip), `.zst` (Zstandard), 압축 없음

새 형식 추가 시:
1. `Cargo.toml`에 압축 라이브러리 추가
2. `src/repomd/fetch.rs`에 압축 해제 함수 구현
3. `auto_decompress` 매치에 확장자 추가
4. `tests/test_compression.rs`에 테스트 추가

### 3. 검색 전략
- **Structured Search**: SQL 기반 정확 검색
- **Semantic Search**: Vector 유사도 기반 탐색
- **Query Planner**: 두 방식을 조합하여 최적 결과 제공

Vector search는 **후보 축소**용, SQL이 **최종 정확성** 보장

### 4. Embedding
기본 빌드에 포함:
- Candle 0.9 (ML 프레임워크)
- all-MiniLM-L6-v2 모델 (384차원)
- sqlite-vec 정적 링크 (벡터 검색)

## 일반적인 작업

### 의존성 추가
```bash
# 기본 의존성
cargo add <crate-name>

# Optional 의존성
cargo add --optional <crate-name>
```

`Cargo.toml` 정리 유지

### 테스트 실행
```bash
# 전체 테스트
cargo test

# 특정 테스트
cargo test --test test_compression

# 문서 테스트
cargo test --doc
```

### 빌드
```bash
# 개발 빌드
cargo build

# 릴리스 빌드
cargo build --release

# macOS 최적화 빌드 (Accelerate framework - 권장)
cargo build --release --features accelerate

# NVIDIA GPU 가속 빌드
cargo build --release --features cuda
```

### 코드 품질
```bash
# 포맷팅
cargo fmt

# Lint
cargo clippy -- -D warnings

# 문서 생성
cargo doc --no-deps --open
```

## 문서 작성 규칙

### README.md
- 프로젝트 개요 및 quick start
- 간결하고 명확하게
- 링크로 상세 문서 연결

### docs/USAGE.md
- 사용자 대상 가이드
- 실용적인 예제 중심
- 일반적인 사용 사례 커버

### docs/DEVELOPMENT.md
- 개발자 대상 노트
- 아키텍처 결정 기록
- 알려진 제한사항

### docs/design/
- 설계 문서는 변경하지 않음 (historical record)
- 새 설계는 별도 문서 작성
- 날짜와 버전 명시

### docs/CHANGELOG.md
- 버전별 변경사항 기록
- [Keep a Changelog](https://keepachangelog.com/) 형식 준수
- Added, Changed, Deprecated, Removed, Fixed, Security 섹션

## 에러 처리 패턴

### Result 사용
```rust
// ✅ Good
pub fn parse_xml(path: &Path) -> Result<Vec<Package>> {
    let data = std::fs::read_to_string(path)?;
    // ...
    Ok(packages)
}

// ❌ Bad - unwrap 사용
pub fn parse_xml(path: &Path) -> Vec<Package> {
    let data = std::fs::read_to_string(path).unwrap();
    // ...
}
```

### 에러 전파
```rust
// ✅ Good - context 추가
let config = std::fs::read_to_string(path)
    .map_err(|e| RpmSearchError::Config(
        format!("Failed to read config from {}: {}", path.display(), e)
    ))?;

// ❌ Bad - context 없음
let config = std::fs::read_to_string(path)?;
```

### 새 에러 타입
`src/error.rs`에 추가:
```rust
#[derive(Error, Debug)]
pub enum RpmSearchError {
    // 기존...
    
    #[error("New error type: {0}")]
    NewError(String),
}
```

## 성능 고려사항

### 메모리
- 스트리밍 파싱 사용 (전체 메모리 로드 지양)
- 필요시에만 데이터 로드
- 큰 컬렉션은 Iterator 활용

### 디스크 I/O
- SQLite prepared statements 재사용
- 트랜잭션으로 여러 작업 묶기
- 인덱스 적절히 활용

### CPU
- 병렬 처리는 신중히 (대부분 I/O bound)
- Embedding은 배치로 처리
- 프로파일링 먼저, 최적화는 나중

## 보안 고려사항

- **입력 검증**: 외부 데이터(XML) 항상 검증
- **SQL Injection**: Prepared statements 사용
- **파일 시스템**: Path traversal 방지
- **의존성**: `cargo audit` 정기 실행

## 테스트 전략

### 단위 테스트
각 모듈 내 `#[cfg(test)]` 모듈:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_functionality() {
        // ...
    }
}
```

### 통합 테스트
`tests/` 디렉토리:
- End-to-end 시나리오
- 모듈 간 상호작용
- 실제 데이터 사용

### 문서 테스트
API 문서 예제가 실행 가능하도록:
```rust
/// # Examples
/// 
/// ```
/// use rpm_repo_search::Package;
/// let pkg = Package::new("test");
/// assert_eq!(pkg.name, "test");
/// ```
```

## 버전 관리

### Semantic Versioning
- MAJOR: 호환성 깨지는 변경
- MINOR: 기능 추가 (호환성 유지)
- PATCH: 버그 수정

### Git Workflow
- `main` 브랜치: 안정 버전
- Feature 브랜치: 새 기능 개발
- Descriptive commit messages

## 배포 체크리스트

릴리스 전 확인사항:
- [ ] 모든 테스트 통과 (`cargo test`)
- [ ] Clippy 경고 없음 (`cargo clippy`)
- [ ] 문서 업데이트 (README, CHANGELOG)
- [ ] 버전 번호 업데이트 (Cargo.toml)
- [ ] 빌드 확인 (기본 + embedding)
- [ ] 예제 동작 확인

## 문제 해결

### 일반적인 이슈

1. **Candle 버전 충돌**
   ```bash
   cargo clean
   cargo update
   ```

2. **SQLite 관련 에러**
   - `rusqlite` feature `bundled` 사용
   - 데이터베이스 파일 권한 확인

3. **Embedding 제외 빌드**
   - 기본적으로 포함되어 있음
   - 제외하려면: `--no-default-features` 사용

## AI Agent 특별 지침

### 코드 생성 시
1. **기존 패턴 일관성 유지**: 프로젝트 코드 스타일 따르기
2. **타입 안정성**: Rust의 타입 시스템 최대 활용
3. **에러 처리 필수**: 모든 실패 케이스 처리
4. **문서화**: 공개 API는 반드시 문서 작성

### 리팩토링 시
1. **테스트 먼저**: 현재 동작을 테스트로 보장
2. **점진적 변경**: 작은 단위로 변경
3. **이전 동작 유지**: 호환성 중요

### 디버깅 시
1. **로그 활용**: `tracing` 사용
2. **타입 에러 주의**: 컴파일러 메시지 정확히 읽기
3. **테스트로 재현**: 버그를 테스트로 먼저 재현

## 참고 자료

### 내부 문서
- [설계 개요](docs/design/rpm_repo_vector_search_design.md)
- [상세 설계](docs/design/rpm_repo_vector_search_detailed_design.md)
- [사용 가이드](docs/USAGE.md)
- [개발 문서](docs/DEVELOPMENT.md)

### 외부 리소스
- [Rust Book](https://doc.rust-lang.org/book/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [rpm.org](https://rpm.org/)
- [createrepo_c](https://github.com/rpm-software-management/createrepo_c)

## 연락처

프로젝트 관련 질문이나 제안은 이슈 트래커를 사용하세요.

---

**마지막 업데이트**: 2026-02-07  
**문서 버전**: 1.0
