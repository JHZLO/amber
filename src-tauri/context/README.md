# context — AI 시스템 프롬프트

Amber의 AI 기능이 쓰는 시스템 프롬프트 모음이다. 각 파일은 한 기능에 대해 AI CLI
(Claude / Codex / Gemini)로 **그대로(verbatim)** 전달되는 프롬프트 본문이다.
`src-tauri/src/ai.rs`에서 `include_str!`로 바이너리에 컴파일해 넣으므로,
**프롬프트를 고치면 재빌드가 필요하다**(`pnpm tauri dev` / `pnpm tauri build`).

| 파일 | Tauri 커맨드 | 기능 | 출력 계약 |
|------|--------------|------|-----------|
| `concept-generate.md` | `ai_generate` | Q&A 원문 → 개념 카드, 그리고 "선택 영역 → 개념 승격" | raw JSON (Contract) |
| `concept-augment.md`  | `ai_augment`  | 기존 개념 노트 보강 | raw JSON (Contract) |
| `note-compose.md`     | `ai_note_compose` / `_stream` | 필기노트 작성·보강 | raw 마크다운 |
| `note-ask.md`         | `ai_note_ask` | 노트 인라인 질문에 짧은 답변 | 평문 텍스트 |
| `diagram-erd.md`      | `ai_erd_generate_stream` | 다이어그램 탭: 스키마 DDL → ERD mermaid | raw mermaid 소스 |

> 데일리 리포트 프롬프트만 예외로 `src-tauri/src/report.rs` 안에 인라인 상수
> (`REPORT_SYSTEM_PROMPT`)로 있다 — 수집한 활동 데이터 포맷과 강하게 묶여 있어서다.

## 규칙 (수정 시 반드시 지킬 것)

- **프롬프트는 영어로 쓰되, 결과물은 노트의 주 언어(대개 한국어)로 나와야 한다.**
  각 프롬프트에 "입력의 주 언어를 따른다(대개 한국어)" 규칙을 유지해 사용자 출력이
  한국어로 유지되게 한다.
- **출력·프로토콜에 쓰이는 한국어 토큰은 번역하지 않는다.** `ai.rs`가 입력에 끼워 넣는
  섹션 마커 — `[사용자 추가 지시]` · `[대화 원문]` · `[보강 요청]` · `[현재 노트]` ·
  `[작성 요청]` · `[질문]` · `[선택한 부분]` · `[이전 문답]` · `[노트 전체]` ·
  `[스키마 DDL]` · `[추가 지시]` · `[현재 다이어그램 (Mermaid)]` — 와
  `concept-generate.md` 안의 권장 섹션 헤더(`## 핵심` · `## 예시` …)는 리터럴이다.
  번역하면 입력 매칭이 깨지거나 사용자 노트 구조가 바뀐다.
- **JSON 키**(`title` · `summary` · `detail_markdown` · `tags` · `confidence_suggestion` ·
  `source_excerpt`)는 `claude.rs`의 `Contract` 구조체와 1:1이다 — 그대로 둔다.
- 파일 내용은 그대로 전송되므로 front-matter나 장식을 붙이지 말고 순수 프롬프트 텍스트만 둔다.
