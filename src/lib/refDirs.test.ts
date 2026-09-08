import { describe, expect, it } from "vitest";
import { pushRecent, refDirName } from "./refDirs";

describe("pushRecent", () => {
  it("puts the new folder first and drops duplicates", () => {
    expect(pushRecent(["/a", "/b"], "/b")).toEqual(["/b", "/a"]);
    expect(pushRecent(["/a"], "/c")).toEqual(["/c", "/a"]);
  });

  it("caps the list and ignores blank input", () => {
    expect(pushRecent(["/1", "/2", "/3"], "/0", 3)).toEqual(["/0", "/1", "/2"]);
    expect(pushRecent(["/a"], "   ")).toEqual(["/a"]);
  });
});

describe("refDirName", () => {
  it("returns the last path segment, ignoring a trailing slash", () => {
    expect(refDirName("/Users/me/code/amber")).toBe("amber");
    expect(refDirName("/Users/me/code/amber/")).toBe("amber");
    expect(refDirName("/")).toBe("/");
  });
});
