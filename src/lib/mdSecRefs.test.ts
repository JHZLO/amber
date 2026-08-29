// 절 참조 회귀 테스트. 오탐(날짜·범위가 링크가 되는 것)과 누락(참조가 글자로 남는 것)은
// 둘 다 본문에서 바로 보이는 사고라 데이터로 고정한다.

import { describe, expect, it } from "vitest";
import { findSecRefs, headingSection, remarkSecRefs } from "./mdSecRefs";

type MdNode = {
  type: string;
  value?: string;
  url?: string;
  children?: MdNode[];
  data?: { hProperties?: Record<string, string> };
};

const text = (value: string): MdNode => ({ type: "text", value });
const para = (...children: MdNode[]): MdNode => ({ type: "paragraph", children });
const run = (tree: MdNode) => {
  remarkSecRefs()(tree);
  return tree;
};

describe("findSecRefs", () => {
  it("번호만 있으면 번호가 그대로 표시된다", () => {
    expect(findSecRefs("[[1-2]]에서 본 대로")[0]).toMatchObject({
      sec: "1-2",
      label: "1-2",
    });
  });

  it("별칭이 있으면 별칭만 보이고 이동은 번호로 한다", () => {
    expect(findSecRefs("[[1-2|앞 절]]에서 설명한")[0]).toMatchObject({
      sec: "1-2",
      label: "앞 절",
    });
  });

  it("깊이에 상관없이 잡는다", () => {
    expect(findSecRefs("[[1]] [[1-2]] [[1-2-3]]").map((r) => r.sec)).toEqual([
      "1",
      "1-2",
      "1-2-3",
    ]);
  });

  // 이게 전용 문법을 쓰는 이유다 — 본문의 모든 '1-2' 를 링크하면 아래가 전부 걸린다
  it("맨 텍스트의 날짜·범위·버전은 건드리지 않는다", () => {
    expect(findSecRefs("3-4월에 2-3배로 늘었고 v1-2 를 썼다")).toEqual([]);
  });

  it("대괄호가 하나뿐이면 참조가 아니다", () => {
    expect(findSecRefs("[1-2](#1-2)")).toEqual([]);
  });
});

describe("headingSection", () => {
  it("제목 앞의 번호를 뽑는다", () => {
    expect(headingSection("1-2. 커밋은 스칼라 하나다")).toBe("1-2");
    expect(headingSection("1. 리밸런스의 무대 장치")).toBe("1");
    expect(headingSection("1-1-1. 트리거 조건")).toBe("1-1-1");
  });

  it("번호가 없으면 null", () => {
    expect(headingSection("정리하면")).toBeNull();
    // 마침표가 없으면 번호로 보지 않는다(본문 제목에 숫자가 섞인 경우)
    expect(headingSection("2026 회고")).toBeNull();
  });
});

describe("remarkSecRefs", () => {
  it("참조를 링크로 갈아 끼우고 앞뒤 글자를 보존한다", () => {
    const tree = run(para(text("앞. [[1-2]]에서 본 대로 커밋은 스칼라다.")));
    const kids = tree.children!;
    expect(kids.map((k) => k.type)).toEqual(["text", "link", "text"]);
    expect(kids[0].value).toBe("앞. ");
    expect(kids[1].children![0].value).toBe("1-2");
    expect(kids[1].data?.hProperties?.["data-secref"]).toBe("1-2");
    expect(kids[2].value).toBe("에서 본 대로 커밋은 스칼라다.");
  });

  it("한 문단에 여러 개가 있어도 모두 바뀐다", () => {
    const tree = run(para(text("[[1]] 과 [[2-1|뒤 절]] 참고")));
    const links = tree.children!.filter((k) => k.type === "link");
    expect(links).toHaveLength(2);
    expect(links[1].children![0].value).toBe("뒤 절");
    expect(links[1].data?.hProperties?.["data-secref"]).toBe("2-1");
  });

  it("링크 안의 텍스트는 건드리지 않는다 — 링크 속 링크가 된다", () => {
    const tree = run({
      type: "root",
      children: [
        { type: "link", url: "https://x", children: [text("[[1-2]]")] },
      ],
    });
    const link = tree.children![0];
    expect(link.children!.map((k) => k.type)).toEqual(["text"]);
    expect(link.children![0].value).toBe("[[1-2]]");
  });

  it("참조가 없으면 문단을 그대로 둔다", () => {
    const tree = run(para(text("그냥 본문이다")));
    expect(tree.children!.map((k) => k.type)).toEqual(["text"]);
  });
});
