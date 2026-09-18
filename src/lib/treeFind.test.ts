import { describe, expect, it } from "vitest";
import { filterTree, hits, splitHit, type FindableNode, type TreeMatch } from "./treeFind";

const paths = (m: TreeMatch[]) => m.map((x) => x.path);

const dir = (name: string, children: FindableNode[]): FindableNode => ({
  path: name,
  name,
  isDir: true,
  children,
});
const file = (parent: string, name: string): FindableNode => ({
  path: `${parent}/${name}`,
  name,
  isDir: false,
});

const TREE: FindableNode[] = [
  dir("네트워크", [file("네트워크", "TCP 혼잡 제어.md"), file("네트워크", "HTTP2.md")]),
  dir("CS", [dir("CS/운영체제", [file("CS/운영체제", "스케줄러.md")])]),
  file("", "메모.md"),
];

describe("filterTree", () => {
  it("검색어가 비면 원본 그대로 두고 아무것도 펼치지 않는다", () => {
    const r = filterTree(TREE, "  ");
    expect(r.nodes).toBe(TREE);
    expect(r.matches).toEqual([]);
    expect(r.expand.size).toBe(0);
  });

  it("맞은 파일과 그 조상만 남기고, 조상을 펼친다", () => {
    const r = filterTree(TREE, "스케줄러");
    expect(paths(r.matches)).toEqual(["CS/운영체제/스케줄러.md"]);
    expect([...r.expand].sort()).toEqual(["CS", "CS/운영체제"]);
    expect(r.nodes).toHaveLength(1);
    expect(r.nodes[0].name).toBe("CS");
    expect(r.nodes[0].children?.[0].children).toHaveLength(1);
  });

  it("대소문자를 가리지 않는다", () => {
    expect(paths(filterTree(TREE, "http").matches)).toEqual(["네트워크/HTTP2.md"]);
  });

  it("차례는 트리에 보이는 순서(전위)다", () => {
    expect(paths(filterTree(TREE, "메").matches)).toEqual(["/메모.md"]);
    const many = paths(filterTree(TREE, ".md").matches);
    expect(many).toEqual([
      "네트워크/TCP 혼잡 제어.md",
      "네트워크/HTTP2.md",
      "CS/운영체제/스케줄러.md",
      "/메모.md",
    ]);
  });

  it("폴더가 맞으면 그 아래는 통째로 남기고, 그 폴더는 펼치지 않는다", () => {
    const r = filterTree(TREE, "운영체제");
    expect(r.matches).toEqual([{ path: "CS/운영체제", isDir: true }]);
    // 안의 파일은 차례에 넣지 않는다 — 접힌 행으로 이동하게 되므로
    expect(r.expand.has("CS/운영체제")).toBe(false);
    expect(r.expand.has("CS")).toBe(true);
    expect(r.nodes[0].children?.[0].children).toHaveLength(1);
  });

  it("아무것도 안 맞으면 빈 트리다", () => {
    const r = filterTree(TREE, "없는말");
    expect(r.nodes).toEqual([]);
    expect(r.matches).toEqual([]);
  });

  it("원본을 건드리지 않는다", () => {
    filterTree(TREE, "스케줄러");
    expect(TREE[0].children).toHaveLength(2);
  });
});

describe("hits", () => {
  it("빈 검색어는 아무것도 맞지 않는다", () => {
    expect(hits("아무거나", "")).toBe(false);
    expect(hits("아무거나", "   ")).toBe(false);
  });
});

describe("splitHit", () => {
  it("맞은 조각만 hit 으로 쪼갠다", () => {
    expect(splitHit("TCP 혼잡 제어", "혼잡")).toEqual([
      { text: "TCP ", hit: false },
      { text: "혼잡", hit: true },
      { text: " 제어", hit: false },
    ]);
  });

  it("여러 번 나오면 전부 쪼갠다", () => {
    expect(splitHit("aXaXa", "x")).toEqual([
      { text: "a", hit: false },
      { text: "X", hit: true },
      { text: "a", hit: false },
      { text: "X", hit: true },
      { text: "a", hit: false },
    ]);
  });

  it("검색어가 비면 통째로 한 조각", () => {
    expect(splitHit("이름", "")).toEqual([{ text: "이름", hit: false }]);
  });
});
