# RPM Repository Vector Search - 사용 가이드

## 빌드

### 기본 빌드
```bash
cargo build --release
```

### 하드웨어 가속 빌드
```bash
# macOS (Apple Accelerate)
cargo build --release --features accelerate

# NVIDIA GPU (CUDA)
cargo build --release --features cuda
```

바이너리는 `target/release/rpm_repo_search`에 생성됩니다.

## 사용법

### 1. 저장소 인덱싱

RPM 저장소의 `primary.xml`, `primary.xml.gz` 또는 `primary.xml.zst` 파일을 다운로드하여 인덱싱합니다:

```bash
# Tizen Unified 저장소 예제
wget https://download.tizen.org/snapshots/TIZEN/Tizen/Tizen-Unified/reference/repos/standard/packages/repodata/primary.xml.gz

# 인덱싱 (gz, zst 압축 자동 감지)
./target/release/rpm_repo_search index \
  --file primary.xml.gz \
  --repo tizen-unified
```

### 2. 통계 확인

```bash
./target/release/rpm_repo_search stats
```

출력:
```
Database Statistics:
  Total packages: 1234
```

### 3. 패키지 검색 (이름 기반)

이름 기반 검색:

```bash
# 정확한 이름 검색
./target/release/rpm_repo_search search "openssl"

# 부분 이름 검색
./target/release/rpm_repo_search search "ssl"
```

### 4. Embedding 기반 검색

Embedding 기능은 기본적으로 포함되어 있습니다:

#### 4.1. 모델 다운로드

```bash
# HuggingFace에서 all-MiniLM-L6-v2 모델 다운로드
mkdir -p models/all-MiniLM-L6-v2
cd models/all-MiniLM-L6-v2

# wget 또는 curl로 다운로드
wget https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/config.json
wget https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/model.safetensors
wget https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json

cd ../..

# 또는 직접 방문하여 다운로드:
# https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/tree/main
```

#### 4.2. Embedding 생성

```bash
# 인크리멘털 (기본) - 새 패키지만 처리하여 빠름
./target/release/rpm_repo_search build-embeddings

# 전체 재빌드 - 모든 임베딩을 삭제하고 다시 생성
./target/release/rpm_repo_search build-embeddings --rebuild

# 상세 배치 정보 표시
./target/release/rpm_repo_search build-embeddings --verbose
```

**기본 출력 (진행률 표시):**
```
Building embeddings...
Processing: 32/9588 packages (0.3%)...
Processing: 64/9588 packages (0.7%)...
...
Processing: 9588/9588 packages (100.0%)...
✓ Built embeddings for 9588 packages
```

**Verbose 출력 (상세 정보):**
```
Building embeddings...
Total packages to process: 9588
Batch size: 32

Batch 1/300: Processed 32 packages → Total: 32/9588 (0.3%)
Batch 2/300: Processed 32 packages → Total: 64/9588 (0.7%)
Batch 3/300: Processed 32 packages → Total: 96/9588 (1.0%)
...
Batch 300/300: Processed 20 packages → Total: 9588/9588 (100.0%)
✓ Built embeddings for 9588 packages
```

#### 4.3. 의미 기반 검색

```bash
# 자연어 검색
./target/release/rpm_repo_search search "cryptography library for SSL"

# 필터와 함께 검색
./target/release/rpm_repo_search search "network tools" \
  --arch x86_64 \
  --not-requiring glibc \
  --top-k 20
```

## 명령어 상세

### index
저장소 메타데이터 인덱싱

```bash
rpm_repo_search index --file <PATH> --repo <NAME>
```

**옵션:**
- `-f, --file <PATH>`: primary.xml, primary.xml.gz, 또는 primary.xml.zst 파일 경로
- `-r, --repo <NAME>`: 저장소 이름

### build-embeddings
패키지에 대한 벡터 임베딩 생성

기본적으로 인크리멘털 모드로 동작하여 임베딩이 없는 패키지만 처리합니다.
`--rebuild` 옵션을 사용하면 전체 재빌드합니다.

```bash
rpm_repo_search build-embeddings [OPTIONS]
```

**옵션:**
- `-m, --model <PATH>`: 모델 디렉토리 (기본값: models/all-MiniLM-L6-v2)
- `-t, --tokenizer <PATH>`: 토크나이저 파일 (기본값: models/all-MiniLM-L6-v2/tokenizer.json)
- `-v, --verbose`: 상세한 배치 정보 표시 (기본적으로 진행률은 항상 표시됨)
- `--rebuild`: 전체 재빌드 (기존 임베딩을 모두 삭제하고 다시 생성)

**예제:**
```bash
# 인크리멘털 (기본) - 새 패키지만 처리
rpm_repo_search build-embeddings

# 전체 재빌드 - 모델 변경 후 등
rpm_repo_search build-embeddings --rebuild

# 상세 정보 표시
rpm_repo_search build-embeddings --verbose
```

### search
패키지 검색

```bash
rpm_repo_search search [OPTIONS] <QUERY>
```

**인자:**
- `<QUERY>`: 검색어

**옵션:**
- `-a, --arch <ARCH>`: 아키텍처 필터
- `-r, --repo <REPO>`: 저장소 필터
- `--not-requiring <DEP>`: 특정 의존성이 필요 없는 패키지만
- `--providing <CAP>`: 특정 기능을 제공하는 패키지만
- `-n, --top-k <N>`: 결과 개수 (기본값: 10)

### stats
데이터베이스 통계 표시

```bash
rpm_repo_search stats
```

## 예제 워크플로우

### 기본 사용

```bash
# 1. 빌드 (embedding 기본 포함)
cargo build --release

# 2. 저장소 인덱싱
./target/release/rpm_repo_search index -f primary.xml.gz -r myrepo

# 3. 통계 확인
./target/release/rpm_repo_search stats

# 4. 이름으로 검색
./target/release/rpm_repo_search search "openssl"
```

### 고급 사용 (Embedding 활용)

```bash
# 1. 빌드
cargo build --release

# 2. 모델 다운로드 (one-time)
# (모델 파일을 models/all-MiniLM-L6-v2/에 배치)

# 3. 저장소 인덱싱
./target/release/rpm_repo_search index -f primary.xml.gz -r myrepo

# 4. Embedding 생성 (기본적으로 진행률 표시됨)
./target/release/rpm_repo_search build-embeddings

# 5. 의미 기반 검색
./target/release/rpm_repo_search search "SSL cryptography library"

# 6. 필터링된 검색
./target/release/rpm_repo_search search "web server" \
  --arch x86_64 \
  --repo myrepo \
  --top-k 5
```

## 데이터베이스

기본적으로 `rpm_search.db` 파일이 현재 디렉토리에 생성됩니다.

다른 위치를 사용하려면 `--db` 옵션을 사용하세요:

```bash
./target/release/rpm_repo_search --db /path/to/my.db index -f primary.xml.gz -r myrepo
```

## 문제 해결

### 모델 파일 찾을 수 없음

모델 파일이 올바른 위치에 있는지 확인:

```
models/all-MiniLM-L6-v2/
├── config.json
├── model.safetensors
└── tokenizer.json
```

또는 `--model` 및 `--tokenizer` 옵션으로 경로를 지정하세요.

## 성능 팁

- 대용량 저장소 (100k+ 패키지)의 경우 인덱싱에 시간이 걸릴 수 있습니다
- Embedding 생성은 CPU 집약적이며 수 분이 걸릴 수 있습니다
- SSD를 사용하면 데이터베이스 성능이 향상됩니다

## 추가 정보

자세한 설계 문서는 다음을 참조하세요:
- `rpm_repo_vector_search_design.md`
- `rpm_repo_vector_search_detailed_design.md`
