// Rust 커맨드 에러 → 화면 문구. **문구는 여기서만 만든다.**
//
// Rust 쪽(ai.rs AiError)은 `{ code, message, detail }` 만 돌려주고 사람이 읽을 문장은 만들지
// 않는다 — Rust 에 문구를 두면 UI 언어를 따라갈 수 없기 때문이다. `message` 는 번역이 없는
// 코드용 폴백(한국어)이라 여기에 code 가 빠지면 영어 UI 에 한국어가 새어 나온다.
// Rust 에 새 에러 코드를 만들면 CODE_KEY 에 같이 추가한다.

import { t, type MsgKey } from "./i18n";

/** Rust AiError 와 동일 구조 (앱 공용 에러 봉투 — AI·백업·휴지통 공용) */
export interface CodedError {
  code: string;
  message: string;
  /** 문구에 끼울 가변부(경로·초 수·OS 에러 텍스트). `{detail}` 로 들어간다 */
  detail?: string | null;
}

export function isCodedError(e: unknown): e is CodedError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}

export const CODE_KEY: Record<string, MsgKey> = {
  // AI CLI 실행
  AI_NOT_FOUND: "common.err.ai.notFound",
  AI_AUTH: "common.err.ai.auth",
  AI_RATE_LIMIT: "common.err.ai.rateLimit",
  AI_TIMEOUT: "common.err.ai.timeout",
  AI_BAD_ENVELOPE: "common.err.ai.badEnvelope",
  AI_BAD_CONTRACT: "common.err.ai.badResult",
  AI_ERROR: "common.err.ai.generic",
  SPAWN_ERROR: "common.err.ai.spawn",
  STDIN_ERROR: "common.err.ai.stdin",
  WAIT_ERROR: "common.err.ai.wait",
  // 입력 검증 (코드가 곧 어느 입력이 비었는지)
  EMPTY_TRANSCRIPT: "common.err.empty.transcript",
  EMPTY_INSTRUCTION: "common.err.empty.instruction",
  EMPTY_NOTE_BODY: "common.err.empty.noteBody",
  EMPTY_QUESTION: "common.err.empty.question",
  EMPTY_SELECTION: "common.err.empty.selection",
  EMPTY_DDL: "common.err.empty.ddl",
  // 데일리 리포트 수집(gh)
  GH_NOT_FOUND: "common.err.gh.notFound",
  GH_AUTH: "common.err.gh.auth",
  GH_ERROR: "common.err.gh.generic",
  GH_WINDOW_TRUNCATED: "common.err.gh.windowTruncated",
  REPORT_TIMEOUT: "common.err.report.timeout",
  REPORT_NO_ACTIVITY: "common.err.report.noActivity",
  // 백업 · 휴지통
  BACKUP_NO_DEST: "common.err.backup.noDest",
  BACKUP_INSIDE_APPDATA: "common.err.backup.insideAppData",
  BACKUP_MKDIR: "common.err.backup.mkdir",
  BACKUP_WRITE: "common.err.backup.write",
  BACKUP_PATH: "common.err.backup.path",
  TRASH_FORBIDDEN: "common.err.trash.forbidden",
  TRASH_FAILED: "common.err.trash.failed",
};

/** 어떤 예외든 화면에 쓸 한 줄로. 코드가 있으면 번역, 없으면 원문. */
export function errText(e: unknown): string {
  if (isCodedError(e)) {
    const key = CODE_KEY[e.code];
    // 미등록 코드는 Rust 의 폴백 문구를 그대로 — 비어 있으면 코드라도 보여 준다
    if (!key) return e.message || e.code;
    const text = t(key, { detail: e.detail ?? "" });
    // detail 이 없는데 문구가 "… — " 로 끝나면 매달린 대시를 떼어 낸다.
    // (CLI 가 stderr 를 안 남기고 죽는 경우가 있다)
    return text.replace(/\s*[—-]\s*$/, "");
  }
  if (e instanceof Error) return e.message;
  return String(e);
}
