import { describe, expect, it } from "vitest";
import { composeInstruction } from "./aiInstruction";

describe("composeInstruction", () => {
  it("returns the typed text alone when nothing is chosen", () => {
    expect(composeInstruction("  write about mut  ", [])).toBe("write about mut");
  });

  it("appends chosen instructions as blank-line paragraphs, typed text first", () => {
    expect(composeInstruction("focus on shadowing", ["Use Kotlin examples", "Add a table"])).toBe(
      "focus on shadowing\n\nUse Kotlin examples\n\nAdd a table",
    );
  });

  it("works with no typed text and drops empty extras", () => {
    expect(composeInstruction("", ["", "  Go deeper  "])).toBe("Go deeper");
    expect(composeInstruction("", [])).toBe("");
  });

  it("keeps multi-line saved prompts intact", () => {
    const saved = "# 들어가기 전\n\n## 1-1";
    expect(composeInstruction("", [saved])).toBe(saved);
  });
});

