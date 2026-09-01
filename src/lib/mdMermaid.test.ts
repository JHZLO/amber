import { describe, expect, it } from "vitest";
import { looksLikeMermaid } from "./mdMermaid";

describe("looksLikeMermaid", () => {
  it("선언 키워드로 시작하면 다이어그램이다", () => {
    expect(looksLikeMermaid("sequenceDiagram\n  A->>B: hi")).toBe(true);
    expect(looksLikeMermaid("stateDiagram-v2\n  [*] --> A")).toBe(true);
    expect(looksLikeMermaid("flowchart LR\n  A --> B")).toBe(true);
    expect(looksLikeMermaid("graph TD\n  A --> B")).toBe(true);
    expect(looksLikeMermaid("erDiagram\n  A ||--o{ B : has")).toBe(true);
    expect(looksLikeMermaid("pie title 점유율\n  \"A\" : 40")).toBe(true);
  });

  it("앞쪽 빈 줄과 %%{init}%% 지시문을 건너뛴다", () => {
    expect(looksLikeMermaid("\n\n%%{init: {'theme':'dark'}}%%\nflowchart TD\n A-->B")).toBe(
      true,
    );
  });

  it("선언처럼 시작하는 낱말은 걸리지 않는다 — 경계를 요구한다", () => {
    expect(looksLikeMermaid("graphql query { user { id } }")).toBe(false);
    expect(looksLikeMermaid("piechart = 1")).toBe(false);
    expect(looksLikeMermaid("ganttchart_init()")).toBe(false);
  });

  it("평범한 코드·글은 아니다", () => {
    expect(looksLikeMermaid("SELECT * FROM users;")).toBe(false);
    expect(looksLikeMermaid("const a = 1;")).toBe(false);
    expect(looksLikeMermaid("")).toBe(false);
    expect(looksLikeMermaid("   \n\n")).toBe(false);
  });
});
