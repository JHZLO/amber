# Amber

**호박(amber) 속에 보존하듯 — 배움을 잊히지 않게 붙잡아두는 로컬 지식 보관함.**

AI와의 Q&A로 학습한 개념을 카드로 정리해 바탕화면 위젯으로 반복 노출하고,
마크다운 필기노트와 mermaid 다이어그램까지 한곳에서 관리하는 macOS 데스크톱 앱입니다.
모든 데이터는 로컬에 순수 텍스트로 저장됩니다(local-first).

`Tauri v2` · `React 19` · `TypeScript` · `SQLite` · `mermaid`

---

## 주요 기능

### 🧠 개념 카드 — 잊기 전에 다시 만나기

- AI Q&A 원문을 붙여넣으면 로컬 `claude` CLI가 **제목·요약·상세 노트**를 자동 생성 (저장 전 검토·수정 가능)
- **바탕화면 스티커 위젯**(always-on-top)이 학습 중인 카드를 자신감 낮은 순으로 순환 노출
- 자신감 3단계(`● ○ ○`) + **졸업(learned)** 모델 — 완전히 익힌 개념은 아카이브로
- 기존 노트를 지시 한 줄로 **AI 보강** (예: "예시 코드 추가해줘")

### 📝 필기노트 — 디렉토리로 정리하는 마크다운

- **실제 폴더/파일이 곧 분류 구조** — DB 없이 파일시스템이 정본이라 git·외부 에디터와 그대로 호환
- 편집 모드: 좌 소스 / 우 **라이브 프리뷰**, `⌘S` 저장
- 읽기 모드: 우측 플로팅 **목차**(스크롤 스파이), mermaid 코드펜스 렌더 + 클릭 확대(팬/줌)
- **AI 작성**: 생성 과정을 실시간 스트리밍으로 보여주고, 기존 노트 편집 시 **git 스타일 diff**로 변경점을 검토한 뒤 적용
- **인라인 질문**: 문장을 드래그해 질문하면 AI가 간결하게 답변 — 본문을 불리지 않도록
  질문/답변은 사이드카(`*.comments.json`)에 저장되고 본문엔 하이라이트로 표시 (Notion 댓글 스타일)
- 저장 프롬프트: 자주 쓰는 AI 지시문을 설정에 저장해 칩으로 재사용

### 📊 다이어그램 — mermaid 스튜디오

- ERD·플로우차트·시퀀스를 `.mmd` 파일로 폴더 관리
- **svg-pan-zoom 캔버스**: 휠 줌 · 드래그 팬 · 더블클릭 줌 · 화면 맞춤(단축키 `+` `-` `0` `1`)
- 편집 시 라이브 렌더, 흔한 문법 실수(`\"` 이스케이프) 자동 복구, 오류 원인 표시

### 그 외

- 라이트/다크 테마 (시스템 추종 + 원클릭 토글, 창 간 동기화)
- 삭제는 macOS **휴지통으로 이동** — 실수해도 복구 가능
- 콘텐츠는 순수 Markdown/텍스트 — 앱 없이도 읽히고, vault 폴더 복사만으로 백업 완결

## 아키텍처

| 영역 | 선택 | 이유 |
|---|---|---|
| 셸 | Tauri v2 (Rust) | 가벼운 네이티브 창 + 멀티 윈도우(메인/위젯) |
| UI | React 19 + TypeScript + Vite | |
| 메타 저장 | SQLite (`tauri-plugin-sql`) | 정렬·검색·설정 등 구조화 데이터 |
| 콘텐츠 저장 | 순수 Markdown/`.mmd` 파일 | 앱 독립성, git 버전 관리 |
| AI | 로컬 `claude` CLI (headless) | API 키를 앱에 저장하지 않음 — CLI 인증 재사용 |

데이터 위치 (`~/Library/Application Support/dev.jhzlo.til/`):

```
til.db                          # 메타 (개념 카드·설정)
vault/
├── concepts/<ulid>/index.md    # 개념 상세 노트
├── notes/**/*.md               # 필기노트 (+ *.comments.json 질문 사이드카)
└── diagrams/**/*.mmd           # mermaid 다이어그램
```

## 시작하기

### 요구 사항

- macOS
- Node.js 20+ · [pnpm](https://pnpm.io)
- [Rust](https://rustup.rs) (stable)
- AI 기능 사용 시: [claude CLI](https://claude.com/claude-code) 설치 및 로그인

### 개발 실행

```bash
pnpm install
pnpm tauri dev
```

### 프로덕션 빌드

```bash
pnpm tauri build
```

> AI 기능은 설정(⚙)에서 `claude` 실행 경로와 모델을 지정할 수 있습니다.
> 위젯의 투명 창은 `macOSPrivateApi`를 사용하므로 App Store 배포 대상이 아닙니다.

## 라이선스

[MIT](./LICENSE)
