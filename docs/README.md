# Documentation Index

이 디렉토리는 RPM Repository Vector Search 프로젝트의 모든 문서를 포함합니다.

## 📁 디렉토리 구조

```
docs/
├── README.md                    # 이 파일
├── USAGE.md                     # 사용자 가이드
├── DEVELOPMENT.md              # 개발자 문서
├── COMPRESSION.md              # 압축 형식 가이드
├── CHANGELOG.md                # 버전 변경 이력
└── design/                     # 설계 문서
    ├── rpm_repo_vector_search_design.md
    └── rpm_repo_vector_search_detailed_design.md
```

## 📖 문서 가이드

### 사용자 대상

#### [USAGE.md](USAGE.md)
실제 사용 방법과 예제를 다룹니다.
- **대상**: 최종 사용자, CLI 사용자
- **내용**: 
  - 설치 및 빌드 방법
  - 명령어 사용법 (index, search, build-embeddings, stats)
  - 실전 예제 및 워크플로우
  - 문제 해결 가이드

#### [COMPRESSION.md](COMPRESSION.md)
지원되는 압축 형식에 대한 상세 정보입니다.
- **대상**: RPM 저장소 메타데이터를 다루는 사용자
- **내용**:
  - 지원 압축 형식 (Gzip, Zstandard)
  - 성능 비교
  - 실전 예제
  - 새 형식 추가 방법

### 개발자 대상

#### [DEVELOPMENT.md](DEVELOPMENT.md)
개발 과정과 아키텍처 결정을 문서화합니다.
- **대상**: 프로젝트 기여자, 개발자
- **내용**:
  - 구현 완료 사항
  - 기술적 결정 및 이유
  - 모듈 구조
  - 알려진 제한사항
  - 향후 개선 계획

#### [design/](design/)
프로젝트의 원래 설계 문서입니다.
- **대상**: 아키텍트, 시니어 개발자
- **내용**:
  - 전체 시스템 설계 ([rpm_repo_vector_search_design.md](design/rpm_repo_vector_search_design.md))
  - 구현 레벨 상세 설계 ([rpm_repo_vector_search_detailed_design.md](design/rpm_repo_vector_search_detailed_design.md))
  - 데이터 모델 및 아키텍처
  - 성능 고려사항

### 프로젝트 관리

#### [CHANGELOG.md](CHANGELOG.md)
버전별 변경사항을 추적합니다.
- **대상**: 모든 사용자
- **내용**:
  - 버전별 변경사항
  - 새 기능 및 개선사항
  - 버그 수정
  - Breaking changes

## 📝 문서 작성 가이드라인

### 새 문서 추가 시

1. **적절한 위치 선택**
   - 사용자 가이드 → `docs/` 루트
   - 설계 문서 → `docs/design/`
   - 한시적 문서 → 적절한 서브디렉토리

2. **Markdown 형식 사용**
   - 명확한 제목 계층 구조
   - 코드 블록에 언어 명시
   - 링크는 상대 경로 사용

3. **이 인덱스 업데이트**
   - 새 문서를 목록에 추가
   - 간단한 설명 포함

### 문서 업데이트 시

1. **일관성 유지**
   - 기존 톤과 스타일 따르기
   - 용어 통일

2. **버전 관리**
   - 중요한 변경은 CHANGELOG에 기록
   - 날짜 명시

3. **크로스 레퍼런스**
   - 관련 문서 링크
   - 중복 최소화

## 🔍 문서 검색 팁

### 특정 주제 찾기

| 주제 | 문서 |
|------|------|
| 설치 방법 | USAGE.md |
| 명령어 사용법 | USAGE.md |
| 압축 형식 | COMPRESSION.md |
| 아키텍처 | design/*.md, DEVELOPMENT.md |
| 에러 해결 | USAGE.md, DEVELOPMENT.md |
| API 설계 | design/rpm_repo_vector_search_detailed_design.md |
| 변경 이력 | CHANGELOG.md |
| 개발 환경 설정 | DEVELOPMENT.md |

### 기여하기

문서 개선 제안이나 오류 발견 시:
1. 이슈 생성 또는
2. Pull Request 제출

## 📚 외부 참고 자료

- [Rust Book](https://doc.rust-lang.org/book/) - Rust 학습
- [RPM Documentation](https://rpm.org/documentation.html) - RPM 포맷
- [SQLite Docs](https://www.sqlite.org/docs.html) - SQLite 사용법
- [Candle Examples](https://github.com/huggingface/candle/tree/main/candle-examples) - 임베딩 예제

## 🛠️ 문서 도구

### 로컬 빌드
```bash
# Rust API 문서 생성
cargo doc --no-deps --open
```

### 링크 검증
```bash
# Markdown 링크 체크 (선택적)
# npm install -g markdown-link-check
find docs/ -name "*.md" -exec markdown-link-check {} \;
```

## 📞 연락처

문서 관련 질문이나 제안:
- GitHub Issues
- Pull Requests

---

**마지막 업데이트**: 2026-02-07  
**유지관리자**: RPM Repository Vector Search Team
