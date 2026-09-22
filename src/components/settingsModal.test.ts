// 카드에 이름이 이미 있는데 버전 꼬리가 같은 이름을 되풀이하면 "Claude Code 2.1.263 (Claude Code)".
// CLI 가 말한 것을 함부로 자르지 않는다는 쪽과, 같은 말을 두 번 하지 않는다는 쪽의 경계.

import { describe, expect, it } from "vitest";
import { aiSnapshot, trimVersion } from "./SettingsModal";

describe("trimVersion", () => {
  it("제 이름을 되풀이하는 괄호만 뗀다", () => {
    expect(trimVersion("2.1.263 (Claude Code)", "Claude Code")).toBe("2.1.263");
  });

  it("대소문자는 따지지 않는다", () => {
    expect(trimVersion("2.1.263 (claude code)", "Claude Code")).toBe("2.1.263");
  });

  it("다른 말이 든 괄호는 남긴다 — CLI 가 알려 준 정보다", () => {
    expect(trimVersion("0.153.4 (rust build)", "OpenAI Codex CLI")).toBe("0.153.4 (rust build)");
  });

  it("괄호가 없으면 그대로", () => {
    expect(trimVersion("codex-cli 0.153.4", "OpenAI Codex CLI")).toBe("codex-cli 0.153.4");
  });

  it("공백만 붙은 것도 정리한다", () => {
    expect(trimVersion("  2.1.263  ", "Claude Code")).toBe("2.1.263");
  });
});

describe("aiSnapshot", () => {
  it("네 값이 그대로면 같은 지문 — 없는 변경으로 확인 창을 띄우지 않는다", () => {
    expect(aiSnapshot("claude", "/bin/claude", "opus", "ko")).toBe(
      aiSnapshot("claude", "/bin/claude", "opus", "ko"),
    );
  });

  it("경로의 앞뒤 공백은 변경이 아니다 — 저장이 trim 해서 넣는다", () => {
    expect(aiSnapshot("claude", "  /bin/claude  ", "opus", "ko")).toBe(
      aiSnapshot("claude", "/bin/claude", "opus", "ko"),
    );
  });

  it("프로바이더, 모델, 언어가 바뀌면 각각 다른 지문", () => {
    const base = aiSnapshot("claude", "/bin/claude", "opus", "ko");
    expect(aiSnapshot("codex", "/bin/claude", "opus", "ko")).not.toBe(base);
    expect(aiSnapshot("claude", "/bin/claude", "sonnet", "ko")).not.toBe(base);
    expect(aiSnapshot("claude", "/bin/claude", "opus", "en")).not.toBe(base);
  });

  it("연결 해제(null)도 변경이다", () => {
    expect(aiSnapshot(null, "", "", "auto")).not.toBe(
      aiSnapshot("claude", "", "", "auto"),
    );
  });
});
