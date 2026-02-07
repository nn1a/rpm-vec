# 압축 포맷 지원

## 지원되는 압축 형식

RPM 저장소 메타데이터는 다양한 압축 형식을 사용할 수 있습니다. 이 도구는 다음 형식들을 자동으로 감지하고 처리합니다:

### 1. **Gzip (.gz)**
- 가장 일반적인 형식
- 파일명: `primary.xml.gz`
- 라이브러리: `flate2`

```bash
# 예제
./rpm_repo_search index -f primary.xml.gz -r myrepo
```

### 2. **Zstandard (.zst, .zstd)**
- 최신 압축 형식
- 더 빠른 압축/해제 속도
- 더 나은 압축률
- 파일명: `primary.xml.zst` 또는 `primary.xml.zstd`
- 라이브러리: `zstd`

```bash
# 예제
./rpm_repo_search index -f primary.xml.zst -r myrepo
./rpm_repo_search index -f primary.xml.zstd -r myrepo
```

### 3. **압축 없음 (.xml)**
- 압축되지 않은 원본 XML
- 파일명: `primary.xml`

```bash
# 예제
./rpm_repo_search index -f primary.xml -r myrepo
```

## 자동 감지

파일 확장자를 기반으로 압축 형식을 자동으로 감지합니다:

```rust
// src/repomd/fetch.rs
pub fn auto_decompress<P: AsRef<Path>>(path: P, data: &[u8]) -> Result<Vec<u8>> {
    let extension = path.as_ref()
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    match extension {
        "gz" => Self::decompress_gz(data),
        "zst" | "zstd" => Self::decompress_zstd(data),
        _ => Ok(data.to_vec()),
    }
}
```

## 성능 비교

### Gzip vs Zstandard

| 특성 | Gzip | Zstandard |
|------|------|-----------|
| 압축률 | 중간 | 높음 |
| 압축 속도 | 느림 | 빠름 |
| 해제 속도 | 보통 | 매우 빠름 |
| CPU 사용량 | 높음 | 중간 |
| 호환성 | 매우 높음 | 높음 |

### 벤치마크 예시

100MB primary.xml 기준:

| 압축 형식 | 압축 크기 | 해제 시간 |
|----------|-----------|----------|
| 압축 없음 | 100 MB | - |
| Gzip | ~15 MB | ~2.5s |
| Zstd | ~12 MB | ~0.8s |

*실제 성능은 하드웨어와 데이터에 따라 다를 수 있습니다.*

## RPM 저장소에서 사용 현황

### Fedora / RHEL 9+
- 최근 버전은 Zstandard를 기본으로 사용
- Gzip도 여전히 지원

```bash
# Fedora 최신 repodata 확인
ls /var/cache/dnf/fedora-*/repodata/
# primary.xml.zst
# filelists.xml.zst
# other.xml.zst
```

### Rocky Linux / AlmaLinux
- Gzip이 주로 사용됨
- 일부 저장소는 Zstandard 지원

### OpenSUSE
- Gzip 사용

## 실전 예제

### Fedora 38 저장소 인덱싱

```bash
# repodata 다운로드
wget https://mirrors.fedoraproject.org/mirrorlist?repo=fedora-38&arch=x86_64

# primary.xml.zst 다운로드
cd repodata/
wget https://download.fedoraproject.org/.../primary.xml.zst

# 인덱싱 (자동으로 zstd 감지)
rpm_repo_search index -f primary.xml.zst -r fedora38
```

### Rocky Linux 9 저장소 인덱싱

```bash
# primary.xml.gz 다운로드
wget https://download.rockylinux.org/.../primary.xml.gz

# 인덱싱 (자동으로 gzip 감지)
rpm_repo_search index -f primary.xml.gz -r rocky9
```

## 트러블슈팅

### "Unsupported compression format" 오류

파일 확장자가 표준이 아닌 경우:

```bash
# 확장자 확인
file primary.xml.unknown

# 수동 압축 해제 후 인덱싱
gunzip primary.xml.unknown  # gzip인 경우
zstd -d primary.xml.unknown # zstd인 경우
rpm_repo_search index -f primary.xml -r myrepo
```

### 손상된 압축 파일

```bash
# Gzip 파일 검증
gunzip -t primary.xml.gz

# Zstd 파일 검증
zstd -t primary.xml.zst

# 재다운로드
wget --continue <URL>
```

## 개발 노트

### 의존성

```toml
[dependencies]
flate2 = "1.0"   # Gzip 지원
zstd = "0.13"     # Zstandard 지원
```

### 새 압축 형식 추가

다른 압축 형식(예: bzip2, xz)을 추가하려면:

1. `Cargo.toml`에 해당 crate 추가
2. `src/repomd/fetch.rs`에 압축 해제 함수 추가
3. `auto_decompress` 매치 패턴에 확장자 추가

```rust
// 예: bzip2 지원 추가
pub fn decompress_bz2(data: &[u8]) -> Result<Vec<u8>> {
    use bzip2::read::BzDecoder;
    let mut decoder = BzDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;
    Ok(decompressed)
}

// auto_decompress에 추가
match extension {
    "gz" => Self::decompress_gz(data),
    "zst" | "zstd" => Self::decompress_zstd(data),
    "bz2" => Self::decompress_bz2(data),  // 새로 추가
    _ => Ok(data.to_vec()),
}
```

## 참고 자료

- [Zstandard 공식 문서](https://facebook.github.io/zstd/)
- [DNF Documentation - Repository Configuration](https://dnf.readthedocs.io/)
- [createrepo_c - RPM metadata compression](https://github.com/rpm-software-management/createrepo_c)
