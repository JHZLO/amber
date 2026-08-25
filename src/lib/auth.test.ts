// CLI 로그인 스트림 파싱 — 모달이 URL 을 못 뽑으면 사용자는 브라우저로 갈 방법이 없다.
// 표본은 `claude auth login --claudeai` 의 실제 stdout 이다.

import { describe, expect, it } from "vitest";
import { asksForCode, extractAuthUrl } from "./auth";

const URL =
  "https://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e" +
  "&response_type=code&redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback" +
  "&scope=org%3Acreate_api_key+user%3Aprofile&code_challenge=7yrPyvKDb8Tl0AsapOGymXjbwWfuWoh8" +
  "&code_challenge_method=S256&state=q-ZiEPvcCD5C6wccD-Ih7iTcNKAfoQ_9Mq3ADWHKT0s";

const STREAM = `Opening browser to sign in…\nIf the browser didn't open, visit: ${URL}\nPaste code here if prompted > `;

describe("extractAuthUrl", () => {
  it("안내 문장에 섞여 온 인증 URL 을 통째로 뽑는다 (쿼리까지)", () => {
    expect(extractAuthUrl(STREAM)).toBe(URL);
  });

  it("문장 끝 구두점은 URL 에 넣지 않는다", () => {
    expect(extractAuthUrl("visit: https://example.com/a.")).toBe("https://example.com/a");
  });

  it("아직 URL 이 안 왔으면 null", () => {
    expect(extractAuthUrl("Opening browser to sign in…")).toBeNull();
  });
});

describe("asksForCode", () => {
  it("CLI 가 코드를 물으면 true", () => {
    expect(asksForCode(STREAM)).toBe(true);
  });

  // URL 안의 code=true·code_challenge 에 걸려 입력칸이 먼저 뜨면 안 된다 —
  // 그래서 'code' 가 아니라 프롬프트 문구를, 출력 전체가 아니라 꼬리만 본다
  it("URL 만 온 단계에서는 false", () => {
    expect(asksForCode(`If the browser didn't open, visit: ${URL}`)).toBe(false);
  });

  // 콜백으로 스스로 끝나는 CLI(codex)는 코드를 묻지 않는다
  it("코드를 묻지 않는 흐름에서는 계속 false", () => {
    expect(asksForCode("Starting local login server on http://localhost:1455")).toBe(false);
  });
});
