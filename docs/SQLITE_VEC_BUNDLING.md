# SQLite-vec 번들링 가이드

## 개요

sqlite-vec 확장을 바이너리와 함께 배포하는 방법들을 설명합니다.

## 현재 상태 (v0.1.0)

- **런타임 로딩**: `Config.sqlite_vec_path`에 확장 경로 지정
- **자동 폴백**: 확장 없으면 수동 cosine similarity 사용
- **배포 방식**: 사용자가 직접 sqlite-vec 설치 필요

## 번들링 옵션

### Option 1: 바이너리 임베딩 (권장)

확장 파일을 바이너리에 포함하고 런타임에 추출하는 방법.

#### 구현 단계

1. **플랫폼별 확장 파일 준비**

```bash
# Linux
vendor/sqlite-vec/linux/vec0.so

# macOS
vendor/sqlite-vec/macos/vec0.dylib

# Windows
vendor/sqlite-vec/windows/vec0.dll
```

2. **build.rs 추가**

```rust
// build.rs
use std::env;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    
    match target_os.as_str() {
        "linux" => {
            println!("cargo:rustc-env=SQLITE_VEC_PATH=vendor/sqlite-vec/linux/vec0.so");
        }
        "macos" => {
            println!("cargo:rustc-env=SQLITE_VEC_PATH=vendor/sqlite-vec/macos/vec0.dylib");
        }
        "windows" => {
            println!("cargo:rustc-env=SQLITE_VEC_PATH=vendor/sqlite-vec/windows/vec0.dll");
        }
        _ => {}
    }
}
```

3. **런타임 추출 로직**

```rust
// src/storage/vector.rs
pub struct VectorStore {
    conn: Connection,
    use_sqlite_vec: bool,
    #[cfg(feature = "bundled-vec")]
    temp_ext_path: Option<PathBuf>,
}

#[cfg(feature = "bundled-vec")]
impl VectorStore {
    pub fn load_bundled_extension(&mut self) -> Result<()> {
        // 플랫폼별 바이너리 데이터
        #[cfg(target_os = "linux")]
        const EXT_BYTES: &[u8] = include_bytes!(env!("SQLITE_VEC_PATH"));
        
        #[cfg(target_os = "macos")]
        const EXT_BYTES: &[u8] = include_bytes!(env!("SQLITE_VEC_PATH"));
        
        #[cfg(target_os = "windows")]
        const EXT_BYTES: &[u8] = include_bytes!(env!("SQLITE_VEC_PATH"));
        
        // 임시 디렉토리에 추출
        let temp_dir = std::env::temp_dir().join("rpm-search");
        std::fs::create_dir_all(&temp_dir)?;
        
        let ext_name = format!("vec0-{}.{}", 
            std::process::id(),
            std::env::consts::DLL_EXTENSION
        );
        let ext_path = temp_dir.join(ext_name);
        
        std::fs::write(&ext_path, EXT_BYTES)?;
        
        self.try_load_extension(&ext_path)?;
        self.temp_ext_path = Some(ext_path);
        
        Ok(())
    }
}

impl Drop for VectorStore {
    fn drop(&mut self) {
        // 임시 파일 정리
        #[cfg(feature = "bundled-vec")]
        if let Some(ref path) = self.temp_ext_path {
            let _ = std::fs::remove_file(path);
        }
    }
}
```

4. **Cargo.toml 설정**

```toml
[features]
default = ["embedding"]
embedding = ["candle-core", "candle-nn", "candle-transformers", "tokenizers", "hf-hub"]
bundled-vec = []  # sqlite-vec 번들링 활성화

[build-dependencies]
# build.rs 실행에 필요한 의존성 없음
```

5. **API 수정**

```rust
// src/api/search.rs
pub fn build_embeddings(&self, embedder: &Embedder, verbose: bool) -> Result<usize> {
    let conn = Connection::open(&self.config.db_path)?;
    let mut vector_store = VectorStore::new(conn)?;
    
    // 번들링된 확장 먼저 시도
    #[cfg(feature = "bundled-vec")]
    {
        match vector_store.load_bundled_extension() {
            Ok(()) => {
                if verbose {
                    println!("✓ Using bundled sqlite-vec extension");
                }
            }
            Err(e) => {
                if verbose {
                    println!("⚠ Failed to load bundled extension: {}", e);
                }
            }
        }
    }
    
    // Config 경로가 있으면 시도
    if !vector_store.is_sqlite_vec_loaded() {
        if let Some(ref ext_path) = self.config.sqlite_vec_path {
            vector_store.try_load_extension(ext_path).ok();
        }
    }
    
    vector_store.initialize(self.config.embedding_dim)?;
    // ...
}
```

#### 빌드 및 사용

```bash
# 번들링 없이 (기본)
cargo build --release

# sqlite-vec 번들링 포함
cargo build --release --features bundled-vec

# 배포
# 단일 바이너리만 배포 - 확장 포함됨
```

#### 장점
- ✅ 단일 바이너리 배포
- ✅ 사용자가 확장 설치 불필요
- ✅ 크로스 플랫폼 지원
- ✅ 기존 런타임 로딩과 병행 가능

#### 단점
- ❌ 바이너리 크기 증가 (~1-2MB per platform)
- ❌ 플랫폼별 빌드 필요
- ❌ 임시 파일 생성/삭제 오버헤드

---

### Option 2: build.rs에서 소스 컴파일

sqlite-vec을 빌드 시점에 컴파일하여 정적 링크.

#### 구현 단계

1. **sqlite-vec 소스 추가**

```bash
git submodule add https://github.com/asg017/sqlite-vec.git vendor/sqlite-vec
# 또는 소스 복사
```

2. **build.rs 구현**

```rust
// build.rs
use std::env;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    
    // sqlite-vec 컴파일
    cc::Build::new()
        .file("vendor/sqlite-vec/sqlite-vec.c")
        .include("vendor/sqlite-vec")
        .flag_if_supported("-fPIC")
        .flag_if_supported("-DSQLITE_CORE")
        .compile("sqlite_vec");
    
    println!("cargo:rerun-if-changed=vendor/sqlite-vec/sqlite-vec.c");
    
    // 공유 라이브러리 생성
    println!("cargo:rustc-link-lib=dylib=sqlite_vec");
    println!("cargo:rustc-link-search=native={}", out_dir.display());
}
```

3. **Cargo.toml**

```toml
[build-dependencies]
cc = "1.0"

[features]
compiled-vec = []  # 소스 컴파일 활성화
```

#### 장점
- ✅ 완전한 통합
- ✅ 버전 관리 명확
- ✅ 소스 레벨 최적화 가능

#### 단점
- ❌ C 컴파일러 필요
- ❌ 빌드 시간 증가
- ❌ 크로스 컴파일 복잡

---

### Option 3: 자동 다운로드 (첫 실행 시)

첫 실행 시 GitHub Releases에서 확장 다운로드.

#### 구현

```rust
// src/storage/extension_manager.rs
use std::path::PathBuf;

pub struct ExtensionManager;

impl ExtensionManager {
    pub fn ensure_extension() -> Result<PathBuf> {
        let data_dir = dirs::data_local_dir()
            .ok_or_else(|| RpmSearchError::Storage("Cannot find data directory".into()))?
            .join("rpm-search");
        
        std::fs::create_dir_all(&data_dir)?;
        
        let ext_name = format!("vec0.{}", std::env::consts::DLL_EXTENSION);
        let ext_path = data_dir.join(&ext_name);
        
        if !ext_path.exists() {
            println!("Downloading sqlite-vec extension...");
            Self::download_extension(&ext_path)?;
            println!("✓ Extension downloaded to {:?}", ext_path);
        }
        
        Ok(ext_path)
    }
    
    fn download_extension(dest: &Path) -> Result<()> {
        let target = env::consts::OS;
        let arch = env::consts::ARCH;
        
        let url = format!(
            "https://github.com/asg017/sqlite-vec/releases/download/v0.1.0/sqlite-vec-{}-{}.{}",
            target, arch, std::env::consts::DLL_EXTENSION
        );
        
        // reqwest나 ureq로 다운로드
        // 여기서는 간단히 표시만
        todo!("Implement download logic");
    }
}
```

#### 장점
- ✅ 바이너리 크기 작음
- ✅ 항상 최신 버전 사용 가능

#### 단점
- ❌ 네트워크 필요 (오프라인 불가)
- ❌ 첫 실행 느림
- ❌ 보안 고려 필요

---

## 권장 사항

### 현재 (v0.1.0)
**Option 4: 현 상태 유지 + 문서화**
- 설치 가이드 제공
- 선택적 기능으로 유지

### 단기 (v0.2.0)
**Option 1: 바이너리 임베딩**
- `--features bundled-vec` 빌드 옵션 제공
- GitHub Actions로 플랫폼별 릴리스 생성

### 장기 (v1.0.0)
**Option 2: 소스 컴파일**
- 완전 통합
- 의존성 명확화

## 구현 계획

### Phase 1: 준비
- [ ] sqlite-vec 소스 vendor에 추가
- [ ] 플랫폼별 사전 컴파일 확장 수집
- [ ] 라이선스 검토

### Phase 2: 구현
- [ ] build.rs 작성
- [ ] VectorStore::load_bundled_extension() 구현
- [ ] Drop trait 구현 (임시 파일 정리)
- [ ] API 수정

### Phase 3: 테스트
- [ ] 각 플랫폼에서 번들링 테스트
- [ ] 폴백 동작 테스트
- [ ] 성능 벤치마크

### Phase 4: 배포
- [ ] GitHub Actions 워크플로우 설정
- [ ] 플랫폼별 릴리스 빌드
- [ ] 문서 업데이트

## 참고 자료

- [sqlite-vec GitHub](https://github.com/asg017/sqlite-vec)
- [rusqlite loadable extensions](https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.load_extension)
- [cc crate](https://docs.rs/cc/latest/cc/)
- [include_bytes! macro](https://doc.rust-lang.org/std/macro.include_bytes.html)
