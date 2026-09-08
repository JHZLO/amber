import { describe, expect, it } from "vitest";
import { waitLine } from "./aiWait";
import { t } from "./i18n";

const base = { startedAt: 1_000, activity: null, activityAt: 0, hasRefDirs: false };

describe("waitLine", () => {
  it("rotates within the connecting pool every 4 seconds", () => {
    expect(waitLine(base, 1_000)).toBe(t("common.ai.wait.connect1"));
    expect(waitLine(base, 5_100)).toBe(t("common.ai.wait.connect2"));
    expect(waitLine(base, 9_200)).toBe(t("common.ai.wait.connect1"));
  });

  it("follows the stream phases and only claims 'almost done' on the thought signal", () => {
    const thinking = { tool: "thinking", target: null };
    expect(waitLine({ ...base, activity: thinking, activityAt: 2_000 }, 2_500)).toBe(t("common.ai.wait.think1"));
    expect(waitLine({ ...base, activity: thinking, activityAt: 2_000 }, 6_500)).toBe(t("common.ai.wait.think2"));
    expect(waitLine({ ...base, activity: { tool: "thought", target: null }, activityAt: 9_000 }, 9_100)).toBe(
      t("common.ai.wait.thought"),
    );
    expect(waitLine({ ...base, activity: { tool: "writing", target: null }, activityAt: 9_500 }, 9_600)).toBe(
      t("common.ai.wait.write"),
    );
  });

  it("shows tool activity verbatim and appends a patience note after a minute", () => {
    const read = waitLine({ ...base, activity: { tool: "Read", target: "/r/src/db.ts" }, activityAt: 3_000 }, 4_000);
    expect(read).toContain("…/src/db.ts");
    expect(waitLine(base, 1_000 + 61_000).endsWith(t("common.ai.wait.long"))).toBe(true);
    expect(waitLine({ ...base, hasRefDirs: true }, 1_000 + 181_000).endsWith(t("common.ai.wait.longerRef"))).toBe(true);
  });
});
