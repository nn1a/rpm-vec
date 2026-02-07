# MCP 서버 사용 가이드

## 개요

RPM Repository Search는 MCP (Model Context Protocol) 서버로 실행되어 AI 에이전트가 RPM 패키지 검색 시스템에 접근할 수 있도록 합니다.

## Claude Desktop 통합

### 1. 설정 파일 수정

Claude Desktop 설정 파일(`~/.config/claude/config.json`)에 MCP 서버 추가:

```json
{
  "mcpServers": {
    "rpm-search": {
      "command": "/path/to/rpm_repo_search",
      "args": ["mcp-server", "--db", "/path/to/rpm_search.db"]
    }
  }
}
```

### 2. Claude Desktop 재시작

설정 변경 후 Claude Desktop을 재시작합니다.

### 3. 사용

채팅에서 자연어로 요청:

```
"Tizen Unified에서 사용 가능한 커널 패키지를 찾아줘"
"nginx 패키지의 상세 정보를 알려줘"
"인덱스된 저장소 목록을 보여줘"
```

## 제공 도구 (Tools)

### 1. rpm_search

RPM 패키지 검색 (이름, 설명, 의미 기반)

**파라미터:**
- `query` (필수): 검색 쿼리
- `arch` (선택): 아키텍처 필터 (예: x86_64, aarch64)
- `repo` (선택): 저장소 필터
- `top_k` (선택, 기본값: 10): 최대 결과 수

**예시:**
```json
{
  "name": "rpm_search",
  "arguments": {
    "query": "kernel",
    "arch": "x86_64",
    "repo": "tizen-unified",
    "top_k": 5
  }
}
```

### 2. rpm_package_info

특정 RPM 패키지의 상세 정보 조회

**파라미터:**
- `name` (필수): 패키지 이름
- `arch` (선택): 아키텍처
- `repo` (선택): 저장소 이름

**예시:**
```json
{
  "name": "rpm_package_info",
  "arguments": {
    "name": "nginx",
    "arch": "x86_64"
  }
}
```

### 3. rpm_repositories

인덱스된 모든 RPM 저장소 목록 및 패키지 수 조회

**파라미터:** 없음

**반환 정보:**
- 저장소 이름
- 각 저장소의 패키지 수

**예시:**
```json
{
  "name": "rpm_repositories",
  "arguments": {}
}
```

## 직접 테스트

MCP 서버를 stdio 모드로 직접 테스트:

> **참고**: MCP 모드에서 로그는 자동으로 stderr로 출력되어 stdout의 JSON-RPC 통신을 방해하지 않습니다.

```bash
# 전체 프로토콜 핸드셰이크 테스트
echo '{
  "jsonrpc":"2.0","id":1,"method":"initialize",
  "params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1"}}
}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | \
  ./target/release/rpm_repo_search mcp-server 2>/dev/null

# 패키지 검색
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"search_packages","arguments":{"query":"kernel","top_k":3}}}' | \
  ./target/release/rpm_repo_search mcp-server 2>/dev/null
```

### 지원 메서드

| 메서드 | 타입 | 설명 |
|--------|------|------|
| `initialize` | Request | 서버 초기화 및 capability 교환 |
| `ping` | Request | 서버 연결 확인 |
| `tools/list` | Request | 사용 가능한 도구 목록 조회 |
| `tools/call` | Request | 도구 실행 |
| `resources/list` | Request | 리소스 목록 (빈 목록 반환) |
| `prompts/list` | Request | 프롬프트 목록 (빈 목록 반환) |
| `notifications/initialized` | Notification | 클라이언트 초기화 완료 알림 |
| `notifications/cancelled` | Notification | 요청 취소 알림 |

## 로깅

MCP 서버의 로그는 **stderr**로 출력되어 stdout의 JSON-RPC 통신을 방해하지 않습니다.
MCP 모드에서 기본 로그 레벨은 `warn`이며, `RUST_LOG` 환경 변수로 조절 가능합니다:

```bash
# Claude Desktop config.json에서:
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

로그 레벨:
- `error`: 에러만
- `info`: 기본 정보 (권장)
- `debug`: 상세 디버깅
- `trace`: 모든 세부사항

## 문제 해결

### MCP 서버가 나타나지 않음

1. Claude Desktop을 완전히 종료하고 재시작
2. 설정 파일 경로 확인: `~/.config/claude/config.json` (macOS/Linux)
3. 바이너리 경로가 절대 경로인지 확인
4. 데이터베이스 파일이 존재하는지 확인

### 검색 결과가 없음

1. 데이터베이스에 저장소가 인덱스되어 있는지 확인:
   ```bash
   ./rpm_repo_search list-repos
   ```

2. 임베딩이 생성되어 있는지 확인:
   ```bash
   ./rpm_repo_search stats
   ```

### 로그 확인

Claude Desktop의 개발자 도구에서 MCP 관련 로그 확인:
- Help → Developer Tools → Console

## 참고

- [MCP 프로토콜 사양](https://modelcontextprotocol.io/)
- [Claude Desktop MCP 가이드](https://docs.anthropic.com/claude/docs/mcp)
