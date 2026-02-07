# 변경 이력

## v0.9.0 - 리포지토리 자동 동기화 및 스케줄러 (2026-02-07)

### 새로운 기능
- ✅ **하드웨어 가속 지원**
  - Apple Accelerate framework 지원 (macOS 최적화 - **권장**)
  - CUDA GPU 지원 (NVIDIA GPU)
  - Optional features: `accelerate`, `cuda`
  - 임베딩 생성 속도 대폭 향상 (특히 Apple Silicon + Accelerate)
  - GPU/가속 실패 시 자동으로 CPU로 fallback

- ✅ **sqlite-vec 정적 번들링**
  - sqlite-vec가 이제 빌드 시점에 정적 링크 (runtime 로딩 불필요)
  - `sqlite-vec` Rust 크레이트 사용 (0.1.6)
  - embedding feature에 포함
  - 런타임 확장 로딩 코드 제거
  - sqlite3_auto_extension으로 자동 등록

- ✅ **리포지토리 자동 동기화**
  - 원격 RPM 저장소의 메타데이터를 자동으로 체크하고 업데이트
  - TOML 기반 설정 파일로 여러 저장소 관리
  - 각 저장소마다 독립적인 동기화 주기 설정
  - repomd.xml의 checksum 비교로 변경 감지
  - 변경 시 primary.xml 다운로드 및 증분 업데이트
  - 동기화 상태 추적 및 이력 관리
  - 항상 포함됨

- ✅ **스케줄러**
  - 데몬 모드로 백그라운드 지속 실행
  - 각 저장소별 주기적 동기화 (tokio interval 기반)
  - 일회성 동기화 모드 지원

### 기술적 변경사항

#### 주요 의존성 업그레이드
- `quick-xml` 0.31 → 0.39 (XML 파싱 라이브러리)
  - API 변경: `trim_text()` 제거, `unescape()` → `decoder().decode()`
  - `src/repomd/parser.rs`, `src/sync/syncer.rs` 수정
- `rusqlite` 0.31 → 0.38 (SQLite 바인딩)
  - Breaking change: `usize`가 `FromSql` trait 미구현
  - COUNT(*) 쿼리 결과를 `i64`로 받아 `usize`로 캐스팅
  - `src/storage/sqlite.rs` 수정
- `tokenizers` 0.20 → 0.22 (토큰화 라이브러리)
- `hf-hub` 0.3 → 0.4 (HuggingFace Hub 클라이언트)
- `thiserror` 1.0 → 2.0 (에러 derive 매크로)
- `tokio` 1.40 → 1.49 (비동기 런타임)
- `reqwest` 0.12 → 0.13 (HTTP 클라이언트)
- `toml` 0.8 → 0.9 (TOML 파서)
- `flate2` 1.0 → 1.1 (압축 라이브러리)

#### 새 모듈
- `src/sync/config.rs` - 동기화 설정 구조 (SyncConfig, RepoSyncConfig, SyncStatus)
- `src/sync/state.rs` - 동기화 상태 추적 스토어 (SyncStateStore)
- `src/sync/syncer.rs` - 저장소 동기화 로직 (RepoSyncer)
- `src/sync/scheduler.rs` - 주기적 스케줄링 (SyncScheduler)
- `src/sync/mod.rs` - 모듈 통합

#### CLI 업데이트
- `sync-init` - 예제 설정 파일 생성
- `sync-once` - 일회성 전체 동기화
- `sync-daemon` - 데몬 모드 실행
- `sync-status` - 동기화 상태 조회

#### 데이터베이스 변경
- `repo_sync_state` 테이블 추가
  - repo_name, status, last_sync, checksum, error_message 필드

#### 새 의존성 추가
- `sqlite-vec` (0.1.6) - 벡터 검색 확장 (정적 링크, optional - embedding feature)
- `cudarc` (0.12) - CUDA GPU 가속 (optional - cuda feature)
- `reqwest` (0.13) - HTTP 클라이언트 (blocking mode, optional)
- `toml` (0.9) - TOML 파싱 (optional)
- `chrono` (0.4) - 시간 처리 (serde feature, optional)

#### 코드 개선
- `src/embedding/model.rs` - 디바이스 선택 로직 추가
  - `select_device()` 함수로 GPU → CPU fallback  
  - CUDA 우선, CPU는 Accelerate framework 사용 (macOS)
  - Accelerate framework 감지 및 로깅

#### 에러 타입 추가
- `RpmSearchError::Fetch` - HTTP 요청 실패
- `RpmSearchError::Parse` - XML 파싱 실패

### 사용 방법

```bash
# 설정 파일 생성
rpm_repo_search sync-init

# 설정 편집
vim sync-config.toml

# 일회성 동기화
rpm_repo_search sync-once

# 데몬 모드
rpm_repo_search sync-daemon &

# 상태 확인
rpm_repo_search sync-status
```

### 빌드 방법

```bash
# 기본 빌드
cargo build --release

# MCP 기능 포함
cargo build --release --features mcp
```

### 문서
- [SYNC_GUIDE.md](SYNC_GUIDE.md) - 상세 사용 가이드

---

## v0.8.0 - MCP (Model Context Protocol) 서버 지원 (2026-02-07)

### 새로운 기능
- ✅ **MCP 서버 지원**
  - AI 에이전트(Claude Desktop 등)가 RPM 검색 시스템에 접근 가능
  - stdio 기반 JSON-RPC 2.0 통신
  - 5개의 도구 제공: search_packages, get_package_info, list_repositories, compare_versions, get_repository_stats
  - MCP 기능은 `--features mcp`로 활성화

### 기술적 변경사항

#### 새 모듈
- `src/mcp/protocol.rs` - MCP 프로토콜 메시지 타입
- `src/mcp/tools.rs` - 도구 정의 및 메타데이터
- `src/mcp/server.rs` - MCP 서버 구현
- `src/mcp/mod.rs` - 모듈 통합

#### CLI 업데이트
- `mcp-server` 서브커맨드 추가

#### 의존성 추가
- `tokio` (1.40) - 비동기 런타임 (optional)

### 제공 도구

1. **search_packages** - 패키지 검색
   - 자연어 쿼리 지원
   - arch, repo 필터링
   - top_k 결과 제한

2. **get_package_info** - 패키지 상세 정보
   - 의존성 목록 (requires/provides)
   - 버전 정보
   - 메타데이터

3. **list_repositories** - 저장소 목록
   - 패키지 개수 포함

4. **compare_versions** - 버전 비교
   - rpmvercmp 알고리즘 활용
   - epoch:version-release 지원

5. **get_repository_stats** - 저장소 통계
   - 패키지 개수

### 빌드 방법

```bash
# MCP 기능 포함
cargo build --release --features mcp

# MCP 제외 (기본)
cargo build --release
```

**참고**: Embedding과 Sync 기능은 항상 포함됩니다.

### Claude Desktop 통합

```json
{
  "mcpServers": {
    "rpm-search": {
      "command": "/path/to/rpm_repo_search",
      "args": ["mcp-server"]
    }
  }
}
```

### 문서

- 새 가이드: [docs/MCP_GUIDE.md](docs/MCP_GUIDE.md)
- 구현 계획: [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)

---

## v0.7.0 - 벡터 검색 성능 최적화 (2026-02-07)

### 새로운 기능
- ✅ **사전 필터링 최적화 (Pre-filtering Optimization)**
  - SQL 필터(arch, repo)로 후보 축소 후 벡터 검색
  - sqlite-vec O(N) 제약을 우회하는 실용적 개선
  - 예: 1M 패키지 → arch 필터 200K → repo 필터 50K → 벡터 검색
  - 필터링된 검색의 성능 대폭 향상

### 기술적 변경사항

#### PackageStore 신규 메서드
- `get_filtered_pkg_ids(arch, repo)` - 아키텍처와 저장소로 사전 필터링

#### VectorStore 신규 메서드
- `search_similar_filtered(query_embedding, candidate_ids, top_k)` - 필터된 후보에 대해서만 벡터 검색

#### SemanticSearch 신규 메서드
- `search_filtered(query, candidate_ids, top_k)` - 사전 필터링 벡터 검색

#### StructuredSearch 신규 메서드
- `get_filtered_candidates(arch, repo)` - 필터된 후보 ID 조회

#### QueryPlanner 업데이트
- arch/repo 필터 존재 시 자동으로 사전 필터링 전략 사용
- 디버그 로그로 필터링된 검색 공간 크기 표시

### 성능 개선 효과

sqlite-vec의 O(N) 전체 스캔 제약을 다음과 같이 완화:

```
일반 검색 (필터 없음):
- 1,000,000 패키지 대상 벡터 검색
- O(N) = O(1,000,000)

사전 필터링 검색 (arch=x86_64, repo=rocky9):
- SQL 필터링: 1M → 200K (arch) → 50K (repo)
- 벡터 검색: O(50,000)
- 성능 개선: 약 20배
```

### 트레이드오프

- **장점**: 필터링된 검색의 성능 대폭 향상
- **장점**: 기존 인프라로 동작 (외부 의존성 없음)
- **중립**: 필터 없는 검색은 여전히 O(N)
- **중립**: 진정한 벡터 인덱스는 아님 (HNSW/IVF 등)

### Structured Logging

```
RUST_LOG=debug rpm_repo_search search --query "kernel" --arch x86_64 --repo rocky9
```

출력 예시:
```
Pre-filtered search space: 50,234 candidates (from 1,000,000 total)
Performing pre-filtered vector search
```

---

## v0.6.0 - 증분 업데이트 지원 (2026-02-07)

### 새로운 기능
- ✅ **증분 업데이트 (Incremental Update)**
  - `index --update` - 기존 저장소를 전체 재인덱싱 없이 증분 업데이트
  - 새 패키지 자동 추가
  - 버전 변경된 패키지 자동 갱신
  - 삭제된 패키지 자동 제거
  - Structured logging으로 추가/업데이트/삭제 통계 표시

### 기술적 변경사항

#### PackageStore 신규 메서드 (storage/sqlite.rs)
- `find_package(name, arch, repo)` - 패키지 검색
- `update_package(old_pkg_id, new_package)` - 패키지 업데이트
- `get_packages_in_repo(repo)` - 저장소의 모든 패키지 목록
- `delete_package(name, arch, repo)` - 패키지 삭제

#### RpmSearchApi 업데이트
- `index_repository()` - `update: bool` 파라미터 추가
- `update_repository_packages()` - 증분 업데이트 로직 구현
  - 기존 패키지 목록 로드
  - 새 패키지 목록과 비교
  - 버전 비교로 업데이트 여부 결정
  - 삭제된 패키지 제거

#### CLI 업데이트
- `index` 명령에 `--update` (-u) 플래그 추가

### 증분 업데이트 알고리즘

```
1. 기존 저장소의 모든 패키지 목록 조회 (name, arch, epoch, version, release)
2. 새 XML 파일 파싱
3. 각 패키지에 대해:
   a. 기존 패키지가 있는가?
      - YES: 버전 비교
        - 다르면 UPDATE
        - 같으면 SKIP
      - NO: INSERT (새 패키지)
4. 기존 패키지 중 새 목록에 없는 것: DELETE
5. 통계 반환: added, updated, removed
```

### 버전 비교 로직

패키지 업데이트 여부는 다음 비교로 결정:
- **Epoch**: 숫자 비교
 - **Version**: 문자열 비교
- **Release**: 문자열 비교

하나라도 다르면 업데이트 수행.

### 안전 처리

- **Embeddings 테이블 미존재**: 삭제 시 에러 무시 (build-embeddings 실행 전 상태) 
- **NULL Epoch**: 0으로 처리
- **트랜잭션**: 업데이트/삭제는 모두 트랜잭션으로 원자성 보장

### 사용 예제

```bash
# 초기 인덱싱
./rpm_repo_search index -f rocky9-baseos.xml.gz -r rocky9-baseos

# 저장소 업데이트 (새 파일로)
./rpm_repo_search index -f rocky9-baseos-updated.xml.gz -r rocky9-baseos --update

# 로그 출력 예:
# 2026-02-07 INFO: Starting incremental update
# 2026-02-07 INFO: Incremental update completed added=15 updated=42 removed=3 total=57
```

### Structured Logging

증분 업데이트 과정의 모든 단계가 로깅됨:
```
- Starting incremental update
- Loaded existing packages (count)
- Adding new package (name, arch)
- Updating package (name, arch, old_version, new_version)
- Removing deleted package (name, arch)
- Incremental update completed (added, updated, removed, total)
```

### 성능

- **메모리**: 기존 패키지 목록을 HashMap으로 로드 (O(N) 메모리)
- **시간 복잡도**: O(N + M) - N=기존 패키지 수, M=새 패키지 수
- **I/O**: 변경된 패키지만 업데이트 (전체 재인덱싱 대비 대폭 개선)

### 이점

- **빠른 업데이트**: 전체 재인덱싱 불필요
- **변경 추적**: 무엇이 추가/업데이트/삭제되었는지 명확히 표시
- **안전성**: 트랜잭션으로 일관성 보장
- **유연성**: 일반 인덱싱과 증분 업데이트를 선택 가능

## v0.5.0 - 다중 저장소 관리 기능 (2026-02-07)

### 새로운 기능
- ✅ **다중 저장소 관리**
  - `list-repos` - 인덱싱된 모든 저장소 목록 조회
  - `repo-stats <repo>` - 특정 저장소 통계 표시
  - `delete-repo <repo> --yes` - 저장소 및 모든 패키지 삭제
  - 저장소별 패키지 카운트 표시

### 기술적 변경사항

#### API 추가
- **PackageStore (storage/sqlite.rs)**
  - `list_repositories()` - 저장소 목록 및 패키지 수 반환
  - `count_packages_by_repo(repo)` - 특정 저장소 패키지 수
  - `delete_repository(repo)` - 저장소 삭제 (트랜잭션

- **RpmSearchApi (api/search.rs)**
  - `list_repositories()` - API 레벨 저장소 목록
  - `repo_package_count(repo)` - 저장소 통계
  - `delete_repository(repo)` - 저장소 삭제

#### CLI 명령
새로운 서브커맨드 3개 추가:
```bash
# 저장소 목록
./rpm_repo_search list-repos

# 저장소 통계
./rpm_repo_search repo-stats rocky9

# 저장소 삭제
./rpm_repo_search delete-repo tizen --yes
```

### 데이터베이스 동작

저장소 삭제 시 자동으로 처리되는 항목:
- `packages` 테이블에서 해당 repo의 모든 패키지 삭제
- 관련된 `requires`, `provides` 데이터 삭제
- 관련된 `embeddings` 데이터 삭제
- 트랜잭션으로 원자성 보장

### 사용 예제

```bash
# 다중 저장소 인덱싱
./rpm_repo_search index -f rocky9-baseos.xml.gz -r rocky9-baseos
./rpm_repo_search index -f rocky9-appstream.xml.gz -r rocky9-appstream
./rpm_repo_search index -f fedora-39.xml.zst -r fedora-39

# 저장소 목록 확인
./rpm_repo_search list-repos
# 출력:
# Repository                       Packages
# ──────────────────────────────────────────
# fedora-39                            15234
# rocky9-appstream                     8765
# rocky9-baseos                        2341

# 특정 저장소 통계
./rpm_repo_search repo-stats rocky9-baseos

# 저장소 검색 (필터링)
./rpm_repo_search search "kernel" --repo rocky9-baseos

# 저장소 삭제
./rpm_repo_search delete-repo fedora-39 --yes
```

### Structured Logging

모든 저장소 관리 명령에 structured logging 적용:
```
2026-02-07T06:29:44.877718Z  INFO list_repos: Retrieved repository list repo_count=3
2026-02-07T06:29:50.931492Z  INFO repo_stats{repo=rocky9}: Retrieved repository statistics count=2341
2026-02-07T06:30:12.456789Z  INFO delete_repo{repo=fedora-39}: Deleted repository deleted=15234
```

### 이점

- **유연성**: 여러 버전/배포판 동시 관리 가능
- **분리**: 저장소별 독립적 관리
- **청소**: 불필요한 저장소 쉽게 삭제
- **통계**: 저장소별 현황 파악 용이

## v0.4.0 - 구조화된 로깅 구현 (2026-02-07)

### 새로운 기능
- ✅ **Structured Logging with Tracing**
  - `tracing` 및 `tracing-subscriber` 통합
  - 환경 변수 `RUST_LOG`로 로그 레벨 제어
  - Spans와 events를 활용한 구조화된 로깅
  - Key-value 페어로 구조화된 필드 (예: `count=123`, `repo=rocky9`)
  - 타임스탬프 자동 기록 (ISO 8601 형식)

### 기술적 변경사항

#### 의존성
- `tracing-subscriber` feature 추가: `env-filter`
  ```toml
  tracing-subscriber = { version = "0.3", features = ["env-filter"] }
  ```

#### 코드 변경
- **src/main.rs**
  - `println!`을 `tracing::info!`로 변경
  - 명령별 spans 추가 (`index`, `search`, `build_embeddings`, `stats`)
  - `RUST_LOG` 환경 변수 지원 (기본값: `info`)

- **src/api/search.rs**
  - 모든 API 함수에 `#[instrument]` 속성 추가
  - Progress 출력은 `println!` 유지 (사용자 인터페이스)
  - 내부 동작은 `debug!`, `info!`, `warn!` 사용
  - Extension 로딩, 배치 처리 등 상세 로깅

- **src/normalize/version.rs**
  - 디버그용 `eprintln!` 제거 (테스트 코드 정리)

### 사용 방법

```bash
# 기본 (info 레벨)
./rpm_repo_search stats

# Debug 레벨
RUST_LOG=debug ./rpm_repo_search search "kernel"

# Trace 레벨 (모든 로그)
RUST_LOG=trace ./rpm_repo_search build-embeddings

# 특정 모듈만 로깅
RUST_LOG=rpm_repo_search::api=debug ./rpm_repo_search index -f primary.xml.gz -r test
```

### 로그 출력 예제

```
2026-02-07T06:24:23.805545Z  INFO stats: Retrieved statistics count=19176
2026-02-07T06:24:23.805545Z  INFO index: Indexing repository repo=rocky9 file=primary.xml.gz
2026-02-07T06:24:25.123456Z  INFO index: Successfully indexed packages count=1234
```

### 이점

- **디버깅**: 상세한 로그 레벨 제어로 문제 추적 용이
- **프로덕션**: 기본 info 레벨로 최소한의 로그만 출력
- **구조화**: JSON 형식 등으로 파싱 가능한 구조화된 로그
- **성능**: 조건부 로깅으로 성능 영향 최소화
- **관찰성**: Spans로 요청별 추적 가능

## v0.3.0 - RPM 버전 비교 완전 구현 (2026-02-07)

### 새로운 기능
- ✅ **완전한 rpmvercmp 알고리즘 구현**
  - epoch:version-release 형식 완벽 지원
  - 숫자/문자 세그먼트 교차 비교
  - **틸드(~) pre-release 버전 특수 처리**
    - `1.0~rc1` < `1.0` (pre-release가 정식 버전보다 작음)
    - `1.0~alpha` < `1.0~beta` < `1.0`
    - RPM 표준 pre-release 시맨틱 완전 구현

### 기술적 변경사항

#### 새로운 모듈 및 타입
- `src/normalize/version.rs` - `RpmVersion` 구조체 및 비교 로직
  - `compare_segments()` - 세그먼트별 비교 알고리즘
  - 틸드 특수 처리 로직
  - `Ord` trait 구현

#### 개선된 기능
- `Package` 구조체에 `Ord` trait 구현
  - 이름, 아키텍처, 버전 순으로 정렬
  - `to_rpm_version()` 메서드로 버전 객체 생성

#### XML 파서 개선
- `src/repomd/parser.rs`
  - Self-closing 태그 (`<version ... />`) 지원
  - `Event::Empty`와 `Event::Start` 모두 처리

### 테스트
- 14개 버전 비교 테스트 (모두 통과)
  - Epoch 비교
  - Numeric 세그먼트 (1.10 > 1.2)
  - Alpha 세그먼트 (1.0a < 1.0b)
  - Release 비교
  - 실제 패키지 버전 패턴
  - **Tilde pre-release 버전**
    - 기본 케이스: 1.0~rc1 < 1.0
    - 다중 pre-release: alpha < beta < rc1 < rc2
    - Numeric suffix: 2.0~1 < 2.0~2
    - Release에서 tilde: 1~rc1 < 1

### 문서
- [DEVELOPMENT.md](DEVELOPMENT.md) - 버전 비교 구현 완료 표시
- 소스 코드 주석 업데이트 (틸드 시맨틱 설명 추가)

### 마이그레이션 노트

기존 코드에서 `Package` 정렬을 사용하는 경우, 이제 올바른 RPM 버전 순서로 정렬됩니다:

```rust
let mut packages = vec![pkg1, pkg2, pkg3];
packages.sort(); // Now uses proper RPM version comparison
```

## v0.2.0 - Zstandard 압축 지원 추가 (2026-02-07)

### 새로운 기능
- ✅ **Zstandard (.zst, .zstd) 압축 지원**
  - Fedora, RHEL 9+ 등 최신 RPM 저장소에서 사용되는 zstd 압축 형식 지원
  - Gzip 대비 더 빠른 압축 해제 속도와 더 나은 압축률
  - 자동 압축 형식 감지 (`auto_decompress`)

### 기술적 변경사항

#### 의존성 추가
```toml
[dependencies]
zstd = "0.13"  # Zstandard 압축 지원
```

#### 새로운 API
- `RepoFetcher::decompress_zstd()` - Zstd 압축 해제
- `RepoFetcher::auto_decompress()` - 확장자 기반 자동 압축 해제

#### 파일 변경
- `src/repomd/fetch.rs` - Zstd 압축 해제 로직 추가
- `src/api/search.rs` - 자동 압축 감지 사용
- `src/main.rs` - CLI 도움말 업데이트

### 지원 파일 형식
| 확장자 | 압축 형식 | 지원 여부 |
|--------|----------|----------|
| .xml | 압축 없음 | ✅ |
| .gz | Gzip | ✅ |
| .zst | Zstandard | ✅ (신규) |
| .zstd | Zstandard | ✅ (신규) |

### 테스트
- 압축 형식별 단위 테스트 추가 (`tests/test_compression.rs`)
- Gzip, Zstd 압축/해제 정상 동작 확인
- 파일 확장자 자동 감지 테스트

### 문서
- [COMPRESSION.md](COMPRESSION.md) - 압축 형식 상세 가이드 추가
- [README.md](README.md) - Zstd 지원 명시
- [USAGE.md](USAGE.md) - 사용 예제 업데이트
- [DEVELOPMENT.md](DEVELOPMENT.md) - 개발 노트 업데이트

### 사용 예제

#### Fedora 저장소 (Zstd)
```bash
wget https://download.fedoraproject.org/.../primary.xml.zst
./rpm_repo_search index -f primary.xml.zst -r fedora38
```

#### Rocky Linux 저장소 (Gzip)
```bash
wget https://download.rockylinux.org/.../primary.xml.gz
./rpm_repo_search index -f primary.xml.gz -r rocky9
```

### 성능 향상
- Zstd 압축 해제가 Gzip 대비 약 3배 빠름
- 100MB XML 기준: Gzip ~2.5s → Zstd ~0.8s

### 호환성
- 기존 Gzip 파일 처리 로직 유지
- 압축 없는 XML 파일도 계속 지원
- 기존 사용자 코드에 영향 없음

---

## v0.1.0 - 초기 릴리스 (2026-02-07)

### 핵심 기능
- RPM 저장소 메타데이터 파싱 (rpm-md)
- SQLite 기반 패키지 저장
- 이름 기반 검색 및 필터링
- 선택적 벡터 임베딩 (MiniLM-L6-v2)
- 의미 기반 검색 (optional)
- CLI 인터페이스
- Gzip 압축 지원

### 지원 명령어
- `index` - 저장소 인덱싱
- `search` - 패키지 검색
- `build-embeddings` - 벡터 임베딩 생성
- `stats` - 통계 조회
