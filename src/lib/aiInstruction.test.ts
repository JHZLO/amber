import { describe, expect, it } from "vitest";
import { composeInstruction, tailSpan } from "./aiInstruction";

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


describe("tailSpan", () => {
  it("returns the whole text when it is short", () => {
    expect(tailSpan("short", 700)).toBe("short");
  });

  it("starts the tail at a line boundary before the size limit", () => {
    const body = "line one\nline two\nline three\nline four";
    const tail = tailSpan(body, 12);
    expect(tail).toBe("line three\nline four");
    expect(body.endsWith(tail)).toBe(true);
  });

  it("falls back to a hard cut when there is no earlier line break", () => {
    const one = "x".repeat(50);
    expect(tailSpan(one, 10)).toBe("x".repeat(10));
  });
});
