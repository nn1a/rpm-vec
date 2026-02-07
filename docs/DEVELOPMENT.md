# 개발 노트

## 구현 완료 사항

### ✅ 기본 구조
- [x] Rust 프로젝트 초기화
- [x] 의존성 설정 (Cargo.toml)
- [x] 모듈 구조 정의

### ✅ 코어 모듈
- [x] 에러 처리 시스템 (error.rs)
- [x] 설정 관리 (config.rs)

### ✅ rpm-md 파싱
- [x] XML 파서 구현 (quick-xml 기반)
- [x] 데이터 모델 정의
- [x] 파일 fetch 및 압축 해제
- [x] Gzip 압축 지원
- [x] Zstandard 압축 지원

### ✅ 데이터 정규화
- [x] Package 모델
- [x] Dependency 모델
- [x] RPM → 내부 모델 변환
- [x] RPM 버전 비교 알고리즘 (rpmvercmp)

### ✅ 스토리지
- [x] SQLite 스키마 정의
- [x] 패키지 CRUD 작업
- [x] Vector 저장소 (간소화 버전)
- [x] 인덱스 생성

### ✅ Embedding
- [x] Candle 통합
- [x] MiniLM 모델 로더
- [x] 배치 임베딩
- [x] Feature flag로 분리 (기본 포함)

### ✅ 검색 엔진
- [x] Query Planner
- [x] 의미 기반 검색 (Semantic)
- [x] 구조적 검색 (Structured)
- [x] 필터링 시스템

### ✅ API & CLI
- [x] 통합 API (RpmSearchApi)
- [x] CLI 인터페이스 (clap)
- [x] 명령어: index, build-embeddings, search, stats
- [x] 진행률 표시 (기본 활성화)
- [x] Verbose 옵션 (상세 배치 정보)
- [x] 모델 다운로드 스크립트
- [x] 개선된 에러 메시지

### ✅ MCP 서버 (v0.8.0, 2026-02-07)
- [x] JSON-RPC 2.0 프로토콜 구현
- [x] stdio 기반 통신
- [x] 5개 도구 제공 (search, info, list, compare, stats)
- [x] Claude Desktop 통합 가이드

### ✅ 리포지토리 자동 동기화 (v0.9.0, 2026-02-07)
- [x] TOML 설정 파일 기반 저장소 관리
- [x] repomd.xml 다운로드 및 파싱
- [x] checksum 기반 변경 감지
- [x] 증분 업데이트 연동
- [x] 동기화 상태 추적 (SQLite)
- [x] 주기적 스케줄러 (tokio interval)
- [x] CLI 커맨드 4개 (init, once, daemon, status)

## 아키텍처 특징

### 모듈 분리
```
repomd/     - RPM 메타데이터 처리
normalize/  - 데이터 정규화
storage/    - 영구 저장
embedding/  - 벡터 임베딩
search/     - 검색 엔진
api/        - 공개 API
```

### 설계 원칙
1. **Embedding by Default**: 임베딩 기능 기본 포함 (선택적 제외 가능)
2. **Single Binary**: 단일 실행 파일 배포
3. **Offline First**: 외부 의존성 없음
4. **Accuracy > Speed**: 정확성 우선

## 기술적 결정

### quick-xml 사용
- 스트리밍 파서로 메모리 효율적
- SAX 스타일 이벤트 기반 처리

### SQLite
- 서버 불필요
- 신뢰성 높음
- 파일 기반 배포 용이

### Candle (Optional)
- Rust 네이티브
- CPU 추론 지원
- HuggingFace 모델 호환

### sqlite-vec (Bundled)
- **정적 링크**: 빌드 시점에 C 소스 컴파일
- **런타임 로딩 불필요**: sqlite3_auto_extension으로 자동 등록
- **단일 바이너리**: 별도 .so/.dylib 파일 불필요
- embedding feature에 포함

### Feature Flags
```toml
[features]
default = ["embedding"]
embedding = ["candle-core", "candle-nn", ..., "sqlite-vec"]
mcp = ["tokio"]
sync = ["tokio", "reqwest", "toml", "chrono"]
```

- **embedding**: 벡터 임베딩 및 의미 기반 검색 (sqlite-vec 정적 링크 포함)
- **mcp**: MCP 서버 (JSON-RPC 2.0, stdio 통신)
- **sync**: 리포지토리 자동 동기화 및 스케줄러

임베딩이 기본으로 포함되며, `--no-default-features`로 제외 가능  
sqlite-vec는 빌드 시점에 정적 링크되어 런타임 로딩 불필요

## 빌드 구성

### 기본 빌드 (embedding 포함, sqlite-vec 런타임 지원)
```bash
cargo build --release
```
- 크기: ~50MB
- 기능: 인덱싱, 이름 검색, 필터링, 의미 기반 검색, 벡터 임베딩
- 벡터 검색: sqlite-vec 확장 런타임 로딩 지원
  - 확장 파일 경로 제공 시: 자동으로 sqlite-vec 사용 (빠름)
  - 확장 없음: 수동 cosine similarity로 자동 폴백 (소규모에 충분)

### MCP 서버 포함 빌드
```bash
cargo build --release --features mcp
```
- 기능: 기본 빌드 + MCP 서버 (mcp-server 커맨드)

### 자동 동기화 포함 빌드
```bash
cargo build --release --features sync
```
- 기능: 기본 빌드 + 자동 동기화 (sync-* 커맨드)

### 모든 기능 포함 빌드
```bash
cargo build --release --features "embedding,mcp,sync"
```
- 기능: embedding + MCP 서버 + 자동 동기화

### 최소 빌드 (embedding 제외)
```bash
cargo build --release --no-default-features
```
- 크기: ~10MB
- 기능: 인덱싱, 이름 검색, 필터링

## 테스트 전략

### 단위 테스트
```rust
#[cfg(test)]
mod tests {
    // 각 모듈별 테스트
}
```

### 통합 테스트 (TODO)
- 실제 rpm-md 파일로 테스트
- end-to-end 워크플로우

## 향후 개선 사항

### 단기
- [x] 더 나은 RPM version 비교 로직 (rpmvercmp 알고리즘 구현)
- [x] 진행률 표시 (기본 활성화, verbose로 상세 정보)
- [x] 구조화된 로깅 (structured logging with tracing spans/events)

### 중기
- [x] sqlite-vec 확장 런타임 지원 (자동 폴백 포함)
- [x] 다중 저장소 관리 (list-repos, repo-stats, delete-repo 명령)
- [x] 증분 업데이트 지원 (index --update 플래그)
- [x] 벡터 검색 성능 최적화 (SQL 사전 필터링)
- [ ] sqlite-vec 번들링 (바이너리 임베딩, [가이드](SQLITE_VEC_BUNDLING.md) 참고)

### 중기
- [x] **MCP (Model Context Protocol) 서버** (v0.8.0, 2026-02-07)
  - AI 도구(Claude Desktop 등)에 RPM 검색 기능 노출
  - 제공 도구:
    - `search_packages`: 패키지 검색 (이름, 설명, 의미 기반)
    - `get_package_info`: 패키지 상세 정보 조회
    - `list_repositories`: 인덱스된 저장소 목록
    - `compare_versions`: RPM 버전 비교
    - `get_repository_stats`: 저장소 통계
  - 기술 스택:
    - JSON-RPC 2.0 프로토콜
    - stdio 기반 통신
    - 기존 RpmSearchApi 활용
  - 배포 방식:
    - 단일 바이너리에 `mcp-server` 서브커맨드
    - Claude Desktop 등록: `~/.config/claude/config.json`
  - Optional feature: `--features mcp`

- [x] **리포지토리 자동 동기화 및 스케줄러** (v0.9.0, 2026-02-07)
  - 원격 RPM 저장소의 자동 동기화
  - 제공 기능:
    - `sync-init`: 예제 설정 파일 생성
    - `sync-once`: 일회성 전체 동기화
    - `sync-daemon`: 데몬 모드 (주기적 실행)
    - `sync-status`: 동기화 상태 조회
  - 동작 방식:
    - repomd.xml의 checksum 기반 변경 감지
    - primary.xml 다운로드 및 증분 업데이트
    - 각 저장소별 독립적 주기 설정
    - 동기화 이력 추적 (repo_sync_state 테이블)
  - 기술 스택:
    - reqwest (HTTP 클라이언트, blocking mode)
    - toml (설정 파일)
    - chrono (시간 추적, serde support)
    - tokio (스케줄러)
  - Optional feature: `--features sync`

### 장기
- [ ] 웹 API 서버 (REST/gRPC)
- [ ] 패키지 의존성 그래프 분석 및 시각화

## 알려진 제한사항

1. **Vector Search 구현**
   - 기본: 단순 cosine similarity scan (소규모 데이터셋에 충분)
   - Runtime: sqlite-vec 확장 로딩 지원 (단일 바이너리)
     - Config에 경로 지정 → 런타임에 확장 로딩 시도
     - 로딩 성공: virtual table + KNN 검색 사용
     - 로딩 실패: 자동으로 수동 cosine similarity로 폴백
   - 참고: sqlite-vec는 virtual table만 사용하며, 벡터 인덱스는 미구현
     - 1M+ 패키지에서는 여전히 O(N) full scan
     - ✅ **사전 필터링 최적화 적용** (v0.7.0, 2026-02-07)
       - arch/repo 필터로 SQL 사전 필터링 후 벡터 검색
       - 예: 1M → 200K (arch) → 50K (repo) → 벡터 검색
       - 필터링된 검색의 성능 대폭 향상 (약 20배)
     - 향후 HNSW/IVF 인덱스 추가 시 진정한 성능 향상 가능

2. **RPM Version 비교**
   - ✅ rpmvercmp 알고리즘 완전 구현 (2026-02-07)
   - epoch:version-release 형식 지원
   - 숫자/문자 세그먼트 교차 비교
   - 틸드(~) pre-release 버전 특수 처리 구현
   - Package 구조체에 Ord trait 구현
   - 테스트: 14개 테스트 모두 통과
     - Epoch 비교
     - Numeric/Alpha 세그먼트
     - Release 버전
     - 실제 패키지 패턴
     - Tilde pre-release (1.0~rc1 < 1.0)

3. **XML 파서**
   - quick-xml 사용, SAX 스타일 이벤트 처리
   - Event::Start와 Event::Empty 모두 지원 (self-closing 태그)
   - 스트리밍 파싱으로 메모리 효율적

4. **메모리 사용**
   - 전체 XML을 스트리밍하지만
   - 패키지 목록은 메모리 상주

## 디버깅 팁

### 로깅 활성화
구조화된 로깅을 사용하며 `RUST_LOG` 환경 변수로 로그 레벨 조절 가능:

```bash
# 기본 (info 레벨)
./target/release/rpm_repo_search search "test"

# Debug 레벨 (API 호출 및 내부 동작 로깅)
RUST_LOG=debug ./target/release/rpm_repo_search search "test"

# Trace 레벨 (모든 세부 로깅)
RUST_LOG=trace ./target/release/rpm_repo_search build-embeddings

# 특정 모듈만 로깅
RUST_LOG=rpm_repo_search::api=debug ./target/release/rpm_repo_search index -f primary.xml.gz -r test
```

**로깅 기능:**
- Structured fields: `count=123`, `repo=rocky9` 등 key-value 페어
- Spans: 명령별 컨텍스트 추적 (`index`, `search`, `build_embeddings`)
- Instrumentation: 함수 레벨 트레이싱
- Timestamps: ISO 8601 형식

### 데이터베이스 검사
```bash
sqlite3 rpm_search.db
> .schema
> SELECT COUNT(*) FROM packages;
> SELECT * FROM packages LIMIT 5;
```

### 빌드 문제
Candle 버전 충돌 시:
```bash
cargo clean
cargo update
cargo build --no-default-features
```

## MCP 서버 구현 계획

### 개요
Model Context Protocol (MCP) 서버를 구현하여 AI 에이전트가 RPM 패키지 검색 시스템에 접근할 수 있도록 합니다.

### 유스케이스
- **대화형 패키지 검색**: "Rocky Linux 9에서 사용 가능한 최신 커널 패키지는?"
- **의존성 분석**: "이 패키지를 설치하면 어떤 의존성이 필요한가?"
- **버전 비교**: "패키지 A의 버전 1.2.3과 1.2.4 중 어느 것이 최신인가?"
- **저장소 탐색**: "어떤 저장소가 인덱스되어 있고 각각 몇 개의 패키지가 있는가?"

### 제공 도구 (Tools)

#### 1. search_packages
```json
{
  "name": "search_packages",
  "description": "Search RPM packages by name, description, or semantic similarity",
  "inputSchema": {
    "type": "object",
    "properties": {
      "query": {"type": "string", "description": "Search query"},
      "arch": {"type": "string", "description": "Filter by architecture (x86_64, aarch64, etc.)"},
      "repo": {"type": "string", "description": "Filter by repository name"},
      "top_k": {"type": "integer", "default": 10}
    },
    "required": ["query"]
  }
}
```

#### 2. get_package_info
```json
{
  "name": "get_package_info",
  "description": "Get detailed information about a specific package",
  "inputSchema": {
    "type": "object",
    "properties": {
      "name": {"type": "string"},
      "arch": {"type": "string"},
      "repo": {"type": "string"}
    },
    "required": ["name"]
  }
}
```

#### 3. list_dependencies
```json
{
  "name": "list_dependencies",
  "description": "List all dependencies (requires/provides) for a package",
  "inputSchema": {
    "type": "object",
    "properties": {
      "package_name": {"type": "string"},
      "depth": {"type": "integer", "default": 1, "description": "Dependency tree depth"}
    },
    "required": ["package_name"]
  }
}
```

#### 4. compare_versions
```json
{
  "name": "compare_versions",
  "description": "Compare two RPM versions using rpmvercmp algorithm",
  "inputSchema": {
    "type": "object",
    "properties": {
      "version1": {"type": "string", "description": "epoch:version-release"},
      "version2": {"type": "string", "description": "epoch:version-release"}
    },
    "required": ["version1", "version2"]
  }
}
```

#### 5. list_repositories
```json
{
  "name": "list_repositories",
  "description": "List all indexed repositories with package counts",
  "inputSchema": {
    "type": "object",
    "properties": {}
  }
}
```

### 구현 구조

```rust
// src/mcp/mod.rs
pub struct McpServer {
    api: RpmSearchApi,
}

impl McpServer {
    pub fn new(config: Config) -> Result<Self> {
        let api = RpmSearchApi::new(config)?;
        Ok(Self { api })
    }

    pub async fn run(&self) -> Result<()> {
        // stdio 기반 JSON-RPC 통신
        // MCP 프로토콜 메시지 처리
    }

    async fn handle_tool_call(&self, tool: &str, args: serde_json::Value) -> Result<String> {
        match tool {
            "search_packages" => self.search_packages(args).await,
            "get_package_info" => self.get_package_info(args).await,
            "list_dependencies" => self.list_dependencies(args).await,
            "compare_versions" => self.compare_versions(args).await,
            "list_repositories" => self.list_repositories(args).await,
            _ => Err(RpmSearchError::InvalidTool(tool.to_string())),
        }
    }
}
```

### CLI 통합

```bash
# MCP 서버 모드 실행 (stdio)
rpm_repo_search mcp-server

# Claude Desktop 설정 예시 (~/.config/claude/config.json)
{
  "mcpServers": {
    "rpm-search": {
      "command": "/path/to/rpm_repo_search",
      "args": ["mcp-server"],
      "env": {
        "RUST_LOG": "info"
      }
    }
  }
}
```

### 필요 의존성

```toml
# Cargo.toml에 추가
[dependencies]
tokio = { version = "1.40", features = ["full"], optional = true }
serde_json = "1.0"

[features]
mcp = ["tokio"]
```

### 개발 단계

1. **Phase 1**: MCP 프로토콜 기본 구현
   - stdio JSON-RPC 통신
   - 도구 등록 및 메타데이터 제공
   
2. **Phase 2**: 검색 도구 구현
   - search_packages
   - get_package_info
   - list_repositories

3. **Phase 3**: 고급 기능
   - list_dependencies (의존성 트리)
   - compare_versions (버전 비교)

4. **Phase 4**: 최적화
   - 비동기 처리
   - 캐싱
   - 에러 핸들링 개선

### 테스트 방법

```bash
# MCP 서버 직접 테스트 (stdio)
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | ./target/release/rpm_repo_search mcp-server

# Claude Desktop 통합 테스트
# 1. config.json 설정
# 2. Claude Desktop 재시작
# 3. 대화에서 "Rocky Linux 패키지 검색해줘" 등 요청
```

## 참고 자료

### 내부 문서
- [SQLite-vec 번들링 가이드](SQLITE_VEC_BUNDLING.md) - 확장 배포 방법

### 외부 리소스
- [RPM Package Manager](https://rpm.org/)
- [rpm-md format](https://github.com/rpm-software-management/createrepo_c)
- [Candle Framework](https://github.com/huggingface/candle)
- [all-MiniLM-L6-v2](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2)
- [sqlite-vec](https://github.com/asg017/sqlite-vec) - SQLite vector search extension
