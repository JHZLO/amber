import { describe, expect, it } from "vitest";
import { describeActivity, shortTarget } from "./aiActivity";

describe("shortTarget", () => {
  it("keeps the last two path segments", () => {
    expect(shortTarget("/Users/me/repo/src/lib/db.ts")).toBe("…/lib/db.ts");
    expect(shortTarget("src/db.ts")).toBe("src/db.ts");
    expect(shortTarget("/repo/")).toBe("repo");
  });

  it("leaves short patterns alone and trims very long ones", () => {
    expect(shortTarget("**/*.rs")).toBe("**/*.rs");
    expect(shortTarget("x".repeat(80))).toHaveLength(60);
  });
});

describe("describeActivity", () => {
  it("mentions the shortened target for file tools", () => {
    expect(describeActivity({ tool: "Read", target: "/a/b/c/d.ts" })).toContain("…/c/d.ts");
    expect(describeActivity({ tool: "Grep", target: "getDb" })).toContain("getDb");
  });

  it("falls back to the tool name for unknown tools", () => {
    expect(describeActivity({ tool: "mcp__x", target: null })).toContain("mcp__x");
    expect(describeActivity({ tool: "Weird", target: "thing" })).toBe("Weird · thing");
  });
});
