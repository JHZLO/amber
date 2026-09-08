import { describe, expect, it } from "vitest";
import { mergeModelOptions } from "./modelOptions";

const curated = [
  { id: "a", label: "A" },
  { id: "", label: "CLI default" },
];

describe("mergeModelOptions", () => {
  it("falls back to the curated list when there is no catalog", () => {
    expect(mergeModelOptions(curated, null, "CLI default")).toBe(curated);
    expect(mergeModelOptions(curated, [], "CLI default")).toBe(curated);
  });

  it("uses the catalog, shortens descriptions and appends the CLI default", () => {
    const out = mergeModelOptions(
      curated,
      [
        { id: "gpt-x", label: "GPT-X", description: "x".repeat(80) },
        { id: "gpt-y", label: "GPT-Y" },
      ],
      "CLI default",
    );
    expect(out.map((o) => o.id)).toEqual(["gpt-x", "gpt-y", ""]);
    expect(out[0].label.startsWith("GPT-X — ")).toBe(true);
    expect(out[0].label.endsWith("…")).toBe(true);
    expect(out[1].label).toBe("GPT-Y");
    expect(out[2].label).toBe("CLI default");
  });
});
