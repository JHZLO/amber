<div align="center">
  <img src="docs/logo.svg" width="108" alt="Amber" />

  <h1>Amber</h1>

  <p><b>배움을 잊히지 않게 — 호박(amber) 속에 보존하듯.</b></p>

  <p>
    개념 학습 카드 · 필기노트 · mermaid 다이어그램을 한곳에서.<br/>
    내가 쓰는 AI를 그대로 연결하고, 모든 데이터는 내 컴퓨터의 순수 텍스트로.
  </p>

  <p>
    <img src="https://img.shields.io/badge/platform-macOS-18181b" alt="platform" />
    <img src="https://img.shields.io/badge/Tauri-v2-FFC131?logo=tauri&logoColor=white" alt="tauri" />
    <img src="https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black" alt="react" />
    <img src="https://img.shields.io/badge/AI-BYO%20(Claude·Codex·Gemini)-8b5cf6" alt="byo-ai" />
    <img src="https://img.shields.io/badge/license-MIT-3da639" alt="license" />
  </p>
</div>

---

**Amber**는 학습을 위한 로컬 우선(local-first) 지식 보관함입니다.
AI와의 Q&A로 배운 개념을 카드로 정리해 바탕화면 위젯으로 반복 노출하고,
마크다운 필기노트와 mermaid 다이어그램까지 하나의 데스크톱 앱에서 관리합니다.

- **Local-first** — 콘텐츠는 순수 Markdown/텍스트 파일. 앱 없이도 읽히고, 폴더 복사로 백업이 끝납니다.
- **Bring your own AI** — 이미 쓰고 있는 AI CLI(Claude Code · OpenAI Codex · Gemini CLI)를
  온보딩에서 자동 감지해 연결합니다. API 키를 앱에 저장하지 않습니다.
- **잊지 않는 구조** — 새로 배운 것일수록 자주 보이는 순환 위젯과 졸업(learned) 모델.

## ✨ 주요 기능

### 🧠 개념 카드 — 잊기 전에 다시 만나기

- AI Q&A 원문을 붙여넣으면 연결된 AI가 **제목·요약·상세 노트**를 자동 생성 (저장 전 검토·수정 가능)
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

### 🤖 AI 연결 — 내 구독을 그대로

- 최초 실행 시 로그인 셸 PATH에서 설치된 AI CLI를 **자동 감지**해 카드로 제시, 클릭 한 번으로 연결
- **Claude Code**(스트리밍 지원) · **OpenAI Codex CLI** · **Gemini CLI** 지원 — 각 CLI의 로그인 세션 재사용
- 설정에서 언제든 다시 감지·전환. AI 없이도 노트/다이어그램 기능은 전부 동작

### 그 외

- 라이트/다크 테마 (시스템 추종 + 원클릭 토글, 창 간 동기화)
- 삭제는 macOS **휴지통으로 이동** — 실수해도 복구 가능

## 🏗 아키텍처

| 영역 | 선택 | 이유 |
|---|---|---|
| 셸 | Tauri v2 (Rust) | 가벼운 네이티브 창 + 멀티 윈도우(메인/위젯) |
| UI | React 19 + TypeScript + Vite | |
| 메타 저장 | SQLite (`tauri-plugin-sql`) | 정렬·검색·설정 등 구조화 데이터 |
| 콘텐츠 저장 | 순수 Markdown/`.mmd` 파일 | 앱 독립성, git 버전 관리 |
| AI | 로컬 AI CLI (headless) | API 키 미저장 — CLI 로그인 세션 재사용 |

데이터 위치 (`~/Library/Application Support/dev.jhzlo.til/`):

```
til.db                          # 메타 (개념 카드·설정)
vault/
├── concepts/<ulid>/index.md    # 개념 상세 노트
├── notes/**/*.md               # 필기노트 (+ *.comments.json 질문 사이드카)
└── diagrams/**/*.mmd           # mermaid 다이어그램
```

## 🚀 시작하기

### 요구 사항

- macOS
- Node.js 20+ · [pnpm](https://pnpm.io)
- [Rust](https://rustup.rs) (stable)
- AI 기능 사용 시: [Claude Code](https://claude.com/claude-code) ·
  [Codex CLI](https://developers.openai.com/codex) ·
  [Gemini CLI](https://github.com/google-gemini/gemini-cli) 중 하나 이상 설치·로그인

### 개발 실행

```bash
pnpm install
pnpm tauri dev
```

### 프로덕션 빌드

```bash
pnpm tauri build
```

> 첫 실행 시 온보딩이 설치된 AI CLI를 자동 감지해 연결을 안내합니다.
> 위젯의 투명 창은 `macOSPrivateApi`를 사용하므로 App Store 배포 대상이 아닙니다.

## 라이선스

[MIT](./LICENSE)
