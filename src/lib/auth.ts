// AI CLI 로그인 브리지의 프론트 wrapper (Rust: src-tauri/src/auth.rs).
//
// 인증 코드는 여기서도 저장하지 않는다 — 입력받은 문자열을 그대로 Rust 로 넘기고,
// Rust 는 그것을 CLI 자식 프로세스의 stdin 에 흘려보낸다. 토큰 보관은 CLI 몫이다.

import { invoke, Channel } from "@tauri-apps/api/core";

export interface AuthStatus {
  /** 이 CLI 를 앱 안에서 로그인시킬 수 있는가 (false = 터미널 안내로 내려간다) */
  supported: boolean;
  /** true/false = 확인됨, null = 확인 불가(옛 CLI·예상 밖 출력) */
  loggedIn: boolean | null;
  /** CLI 가 말한 그대로 — 진단용 */
  detail: string;
}

/** 로그인 진행 중 Rust 가 흘리는 이벤트 */
export type LoginEvent =
  | { kind: "output"; text: string }
  | { kind: "done"; status: AuthStatus };

export async function aiAuthStatus(
  provider: string | null,
  cliPath: string | null,
): Promise<AuthStatus> {
  return invoke<AuthStatus>("ai_auth_status", {
    provider: provider ?? null,
    cliPath: cliPath || null,
  });
}

/** 로그인 시작. 즉시 반환하고, 진행은 onEvent 로 흘러온다 */
export async function aiAuthLogin(
  provider: string | null,
  cliPath: string | null,
  onEvent: (e: LoginEvent) => void,
): Promise<void> {
  const channel = new Channel<LoginEvent>();
  channel.onmessage = onEvent;
  return invoke("ai_auth_login", {
    provider: provider ?? null,
    cliPath: cliPath || null,
    onEvent: channel,
  });
}

/** 브라우저에서 받은 코드를 진행 중인 CLI 에 전달 */
export async function aiAuthCode(code: string): Promise<void> {
  return invoke("ai_auth_code", { code });
}

/** 진행 중인 로그인 중단 (모달을 닫거나 다시 시작할 때) */
export async function aiAuthCancel(): Promise<void> {
  return invoke("ai_auth_cancel");
}

/** 스트림에서 인증 URL 한 개를 뽑는다. CLI 가 "…visit: <url>" 로 알려 준다 */
export function extractAuthUrl(output: string): string | null {
  return output.match(/https:\/\/\S+/)?.[0]?.replace(/[.,)"']+$/, "") ?? null;
}

/** CLI 가 코드를 붙여넣으라고 물었는가.
 *  출력 전체를 보면 URL 안의 `code=true`·`code_challenge` 에 걸리므로 꼬리만 본다. */
export function asksForCode(output: string): boolean {
  return /paste/i.test(output.slice(-200));
}
