import { describe, expect, it } from "vitest";
import { decideDiskSync, isRelevantEvent } from "./noteWatch";

const base = { disk: "new", body: "old", editing: false, draft: "", dismissed: null };

describe("decideDiskSync", () => {
  it("does nothing when the disk matches what is on screen (our own save, a touch)", () => {
    expect(decideDiskSync({ ...base, disk: "old" })).toBe("none");
    expect(decideDiskSync({ ...base, disk: "old", editing: true, draft: "typed" })).toBe("none");
  });

  it("reloads the body in read mode", () => {
    expect(decideDiskSync(base)).toBe("reload");
  });

  it("swaps the draft too when editing without changes", () => {
    expect(decideDiskSync({ ...base, editing: true, draft: "old" })).toBe("reload-draft");
  });

  it("swaps the draft when the edit already equals the disk", () => {
    expect(decideDiskSync({ ...base, editing: true, draft: "new" })).toBe("reload-draft");
  });

  it("asks instead of overwriting unsaved edits", () => {
    expect(decideDiskSync({ ...base, editing: true, draft: "mine" })).toBe("prompt");
  });

  it("does not ask twice about the same disk version", () => {
    expect(decideDiskSync({ ...base, editing: true, draft: "mine", dismissed: "new" })).toBe("none");
    expect(decideDiskSync({ ...base, editing: true, draft: "mine", dismissed: "older" })).toBe("prompt");
  });
});

describe("isRelevantEvent", () => {
  it("ignores access events so our own reads don't loop", () => {
    expect(
      isRelevantEvent({ type: { access: { kind: "open", mode: "read" } }, paths: ["/v/a.md"] }),
    ).toBe(false);
  });

  it("ignores events that only touch our atomic-save temp file", () => {
    expect(
      isRelevantEvent({ type: { create: { kind: "file" } }, paths: ["/v/a.md.amber-tmp"] }),
    ).toBe(false);
  });

  it("keeps note, folder and rename events", () => {
    expect(
      isRelevantEvent({ type: { modify: { kind: "data", mode: "content" } }, paths: ["/v/a.md"] }),
    ).toBe(true);
    expect(
      isRelevantEvent({
        type: { modify: { kind: "rename", mode: "both" } },
        paths: ["/v/a.md.amber-tmp", "/v/a.md"],
      }),
    ).toBe(true);
    expect(isRelevantEvent({ type: "any", paths: [] })).toBe(true);
  });
});
