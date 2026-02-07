# Repository Synchronization Guide

RPM Repository Vector Search v0.9.0 이상에서는 RPM 저장소의 자동 동기화 기능을 제공합니다. 이 기능을 사용하면 원격 RPM 저장소의 메타데이터를 주기적으로 확인하고, 변경 사항이 있을 때 자동으로 로컬 데이터베이스를 업데이트할 수 있습니다.

## 기능 개요

자동 동기화는 다음과 같은 방식으로 작동합니다:

1. **메타데이터 확인**: 저장소의 `repomd.xml` 파일을 다운로드하여 최신 상태 확인
2. **변경 감지**: checksum을 비교하여 메타데이터 변경 여부 판단
3. **증분 업데이트**: 변경이 감지되면 `primary.xml`을 다운로드하고 기존 `--update` 기능으로 증분 업데이트
4. **인크리멘털 임베딩 빌드**: 업데이트 후 새 패키지에 대해서만 임베딩을 자동 생성 (전체 재빌드 없이 효율적 처리)
5. **주기적 실행**: 각 저장소마다 독립적인 주기로 자동 재실행
6. **상태 추적**: 동기화 성공/실패 이력을 데이터베이스에 저장

## 빌드

```bash
# 기본 빌드
cargo build --release

# MCP 기능 추가
cargo build --release --features mcp
```

## 기본 사용법

### 1. 설정 파일 생성

예제 설정 파일을 생성합니다:

```bash
rpm_repo_search sync-init
```

기본적으로 `sync-config.toml` 파일이 생성됩니다. 다른 경로를 사용하려면:

```bash
rpm_repo_search sync-init --output /path/to/config.toml
```

### 2. 설정 파일 편집

생성된 `sync-config.toml` 파일을 편집하여 동기화할 저장소를 설정합니다:

```toml
# 작업 디렉토리 (메타데이터 파일 저장 위치)
work_dir = ".rpm-sync"

# 저장소 목록
[[repositories]]
name = "rocky9-baseos"
base_url = "https://dl.rockylinux.org/pub/rocky/9/BaseOS/x86_64/os"
interval_seconds = 3600    # 1시간마다 체크
enabled = true
arch = "x86_64"

[[repositories]]
name = "rocky9-appstream"
base_url = "https://dl.rockylinux.org/pub/rocky/9/AppStream/x86_64/os"
interval_seconds = 7200    # 2시간마다 체크
enabled = true
arch = "x86_64"

[[repositories]]
name = "fedora-updates"
base_url = "https://mirror.example.com/fedora/updates/39/x86_64"
interval_seconds = 1800    # 30분마다 체크
enabled = false            # 비활성화된 저장소
arch = "x86_64"
```

### 3. 일회성 동기화

설정된 모든 저장소를 한 번 동기화합니다:

```bash
rpm_repo_search sync-once
```

다른 설정 파일 사용:

```bash
rpm_repo_search sync-once --config /path/to/config.toml
```

### 4. 데몬 모드 실행

백그라운드에서 지속적으로 동기화를 수행합니다:

```bash
# 포그라운드 실행
rpm_repo_search sync-daemon

# 백그라운드 실행
rpm_repo_search sync-daemon &

# nohup으로 세션 종료 후에도 유지
nohup rpm_repo_search sync-daemon > sync.log 2>&1 &
```

### 5. 동기화 상태 확인

각 저장소의 동기화 상태를 확인합니다:

```bash
rpm_repo_search sync-status
```

출력 예시:

```
Sync Status:
Repository                Status          Last Sync                 Checksum       
rocky9-baseos            Success         2026-02-07 16:25:41       abc123def456   
rocky9-appstream         Success         2026-02-07 16:26:15       789ghi012jkl   
```

## 상세 설정

### 설정 파일 구조

```toml
# 전역 설정
work_dir = ".rpm-sync"    # 메타데이터 임시 저장 디렉토리

# 각 저장소는 [[repositories]] 섹션으로 정의
[[repositories]]
name = "unique-repo-name"           # 저장소 고유 이름 (필수)
base_url = "https://..."            # 저장소 기본 URL (필수)
interval_seconds = 3600             # 동기화 주기 (초) (필수)
enabled = true                       # 활성화 여부 (선택, 기본값: true)
arch = "x86_64"                     # 아키텍처 (선택, 기본값: x86_64)
```

### 주요 파라미터

- **name**: 저장소를 식별하는 고유한 이름. 데이터베이스의 `repository` 필드로 사용됩니다.
- **base_url**: RPM 저장소의 기본 URL. 이 URL 아래에 `repodata/repomd.xml`이 있어야 합니다.
- **interval_seconds**: 동기화를 체크하는 주기 (초 단위)
  - 3600 = 1시간
  - 7200 = 2시간
  - 86400 = 24시간
- **enabled**: `false`로 설정하면 동기화를 건너뜁니다.
- **arch**: 저장소의 아키텍처. 검색 시 필터링에 사용됩니다.

## 실행 흐름

### 동기화 프로세스

각 저장소에 대해 다음 단계를 수행합니다:

1. **repomd.xml 다운로드**
   ```
   GET {base_url}/repodata/repomd.xml
   ```

2. **primary.xml 정보 추출**
   - `<data type="primary">` 요소 찾기
   - location href 및 checksum 추출

3. **변경 감지**
   - 이전 동기화 시 저장한 checksum과 비교
   - 동일하면 → 건너뛰기
   - 다르면 → 다운로드 진행

4. **primary.xml 다운로드**
   ```
   GET {base_url}/{primary_location}
   ```
   - 자동 압축 해제 (gzip, zstd 지원)

5. **증분 업데이트**
   - 기존의 `index --update` 기능 호출
   - 새 패키지 추가, 기존 패키지 업데이트, 삭제된 패키지 제거

6. **인크리멘털 임베딩 빌드**
   - 변경된 패키지가 있으면 새 패키지에 대해서만 임베딩 자동 생성
   - 전체 재빌드 없이 효율적으로 처리 (기존 임베딩 유지)
   - 삭제/변경된 패키지의 임베딩은 증분 업데이트 단계에서 자동 정리

7. **상태 저장**
   - 성공/실패 상태
   - checksum
   - 최종 동기화 시간

### 데몬 모드 동작

`sync-daemon` 명령은 다음과 같이 동작합니다:

1. 설정 파일 로드
2. 각 저장소마다 독립적인 스케줄러 생성
3. 지정된 주기마다 동기화 실행
4. Ctrl+C 종료 시까지 계속 실행

```
Time: 0s     1h      2h      3h      4h
Repo A: [sync]  [sync]  [sync]  [sync]    (interval: 1h)
Repo B: [sync]          [sync]            (interval: 2h)
```

## 운영 시나리오

### 시나리오 1: 최초 셋업

```bash
# 1. 데이터베이스 초기화
rpm_repo_search index ~/repodata/primary.xml

# 2. 동기화 설정
rpm_repo_search sync-init
vim sync-config.toml

# 3. 일회성 동기화로 테스트
rpm_repo_search sync-once
rpm_repo_search sync-status

# 4. 데몬 모드로 전환
nohup rpm_repo_search sync-daemon > sync.log 2>&1 &
```

### 시나리오 2: 서버 재시작 후

```bash
# 1. 상태 확인
rpm_repo_search sync-status

# 2. 강제 재동기화 (선택)
rpm_repo_search sync-once

# 3. 데몬 재시작
nohup rpm_repo_search sync-daemon > sync.log 2>&1 &
```

### 시나리오 3: 새 저장소 추가

```bash
# 1. 설정 파일에 새 저장소 추가
vim sync-config.toml

# 2. 일회성 동기화로 새 저장소 다운로드
rpm_repo_search sync-once

# 3. 데몬 재시작 (이미 실행 중이라면)
pkill -f "rpm_repo_search sync-daemon"
nohup rpm_repo_search sync-daemon > sync.log 2>&1 &
```

### 시나리오 4: 문제 진단

```bash
# 1. 상태 확인
rpm_repo_search sync-status

# 2. 로그 확인
tail -f sync.log

# 3. 디버그 로그로 재실행
RUST_LOG=debug rpm_repo_search sync-once

# 4. 특정 저장소만 활성화하여 테스트
# sync-config.toml에서 다른 저장소는 enabled = false로 설정
rpm_repo_search sync-once
```

## systemd 서비스 예제

### /etc/systemd/system/rpm-sync.service

```ini
[Unit]
Description=RPM Repository Sync Daemon
After=network.target

[Service]
Type=simple
User=rpm-user
WorkingDirectory=/path/to/workspace
ExecStart=/usr/local/bin/rpm_repo_search sync-daemon --config /etc/rpm-sync/config.toml
Restart=always
RestartSec=60
Environment="RUST_LOG=info"

[Install]
WantedBy=multi-user.target
```

### 서비스 관리

```bash
# 서비스 시작
sudo systemctl start rpm-sync

# 부팅 시 자동 시작
sudo systemctl enable rpm-sync

# 상태 확인
sudo systemctl status rpm-sync

# 로그 확인
sudo journalctl -u rpm-sync -f
```

## 성능 고려사항

### 네트워크 트래픽

- **repomd.xml**: 보통 10-20KB, 매 주기마다 다운로드
- **primary.xml**: 수 MB ~ 수십 MB, 변경 시에만 다운로드

주기를 너무 짧게 설정하면 불필요한 네트워크 트래픽 발생. 권장 최소 주기:
- 빠른 업데이트 저장소 (Fedora updates): 30분
- 일반 저장소: 1-2시간
- 안정 저장소 (RHEL, Rocky Linux): 6-24시간

### 디스크 사용량

- `work_dir`에 임시 메타데이터 파일 저장
- primary.xml 파일은 다운로드 후 파싱 완료되면 삭제되지 않음 (향후 개선 예정)
- 주기적으로 `work_dir` 정리 권장

### 메모리 및 CPU

- 파싱은 스트리밍 방식으로 메모리 효율적
- CPU 사용량은 증분 업데이트 시에만 일시적으로 증가

## 제한사항

현재 버전 (v0.9.0)의 제한사항:

1. **HTTP만 지원**: FTP, NFS 등 다른 프로토콜 미지원
2. **인증 미지원**: Basic Auth, 클라이언트 인증서 등 미지원
3. **프록시 미지원**: 환경 변수 기반 프록시도 현재 미설정
4. **압축 형식**: gzip, zstd만 지원. bzip2, xz는 미지원 (향후 추가 가능)
5. **에러 복구**: 동기화 실패 시 재시도 없음 (다음 주기까지 대기)

## 문제 해결

### "Failed to fetch repomd.xml"

**원인**: 네트워크 오류 또는 잘못된 URL

**해결**:
```bash
# URL 직접 확인
curl -I https://your-repo-url/repodata/repomd.xml

# 설정 파일의 base_url 수정
vim sync-config.toml
```

### "Failed to parse repomd.xml"

**원인**: XML 형식이 올바르지 않거나 예상하지 못한 구조

**해결**:
```bash
# 수동으로 파일 다운로드하여 확인
curl https://your-repo-url/repodata/repomd.xml > /tmp/repomd.xml
xmllint --format /tmp/repomd.xml

# 이슈 리포트 with XML 파일
```

### "No primary data found"

**원인**: repomd.xml에 `<data type="primary">` 요소가 없음

**해결**:
- 저장소 URL이 올바른지 확인
- 아키텍처가 맞는지 확인 (x86_64, aarch64 등)

### 동기화가 계속 실패함

**해결**:
```bash
# 디버그 로그로 상세 정보 확인
RUST_LOG=debug rpm_repo_search sync-once

# 일시적으로 해당 저장소 비활성화
# sync-config.toml에서 enabled = false 설정
```

## 고급 사용법

### 여러 설정 파일 사용

프로덕션과 테스트 환경을 분리:

```bash
# 프로덕션
rpm_repo_search sync-daemon --config prod-sync.toml &

# 테스트
rpm_repo_search sync-daemon --config test-sync.toml &
```

### 조건부 동기화

cron으로 특정 시간에만 동기화:

```bash
# crontab -e
# 매일 오전 2시에만 동기화
0 2 * * * /usr/local/bin/rpm_repo_search sync-once --config /etc/rpm-sync/config.toml
```

### 모니터링 통합

상태를 주기적으로 확인하여 알림:

```bash
#!/bin/bash
# check-sync.sh

STATUS=$(rpm_repo_search sync-status)
if echo "$STATUS" | grep -q "Failed"; then
    echo "Sync failure detected!" | mail -s "RPM Sync Alert" admin@example.com
fi
```

## API 통합 (향후)

현재 버전은 CLI만 제공하지만, 향후 다음을 고려 중:

- HTTP API 엔드포인트 (`/api/sync/status`, `/api/sync/trigger`)
- MCP 서버에 동기화 도구 추가
- 웹 UI 대시보드

## 참고

- [USAGE.md](USAGE.md) - 기본 사용법
- [MCP_GUIDE.md](MCP_GUIDE.md) - MCP 서버 가이드
- [COMPRESSION.md](COMPRESSION.md) - 지원 압축 형식
- [DEVELOPMENT.md](DEVELOPMENT.md) - 개발 문서

---

**버전**: v0.9.0  
**최종 업데이트**: 2026-02-07
