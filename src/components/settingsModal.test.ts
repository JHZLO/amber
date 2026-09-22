// 카드에 이름이 이미 있는데 버전 꼬리가 같은 이름을 되풀이하면 "Claude Code 2.1.263 (Claude Code)".
// CLI 가 말한 것을 함부로 자르지 않는다는 쪽과, 같은 말을 두 번 하지 않는다는 쪽의 경계.

import { describe, expect, it } from "vitest";
import { trimVersion } from "./SettingsModal";

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
