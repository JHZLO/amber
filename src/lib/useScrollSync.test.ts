import { describe, expect, it } from "vitest";
import { cleanAnchors, mapWithAnchors, mappedScrollTop } from "./useScrollSync";

describe("mappedScrollTop", () => {
  it("carries the position across as a ratio and refuses unscrollable panes", () => {
    expect(mappedScrollTop(50, 100, 400)).toBe(200);
    expect(mappedScrollTop(500, 100, 400)).toBe(400); // 범위를 넘어도 끝에 붙는다
    expect(mappedScrollTop(50, 0, 400)).toBeNull();
    expect(mappedScrollTop(50, 100, 0)).toBeNull();
  });
});

describe("cleanAnchors", () => {
  it("drops points that go backwards or sit outside either pane", () => {
    const pts = [
      { a: 100, b: 100 },
      { a: 90, b: 200 }, // a 가 뒤로 감
      { a: 200, b: 80 }, // b 가 뒤로 감
      { a: 300, b: 300 },
      { a: 900, b: 400 }, // a 가 끝 밖
    ];
    expect(cleanAnchors(pts, 800, 800)).toEqual([
      { a: 100, b: 100 },
      { a: 300, b: 300 },
    ]);
  });
});

describe("mapWithAnchors", () => {
  // 원문 1000px 중 400~600 구간이 렌더에서 300px 그림 한 장(400~700)으로 부푼 경우
  const anchors = [
    { a: 400, b: 400 },
    { a: 600, b: 700 },
  ];

  it("pins the document start and end to each other", () => {
    expect(mapWithAnchors(0, anchors, 1000, 1200)).toBe(0);
    expect(mapWithAnchors(1000, anchors, 1000, 1200)).toBe(1200);
  });

  it("lands exactly on a block that starts in both panes", () => {
    expect(mapWithAnchors(400, anchors, 1000, 1200)).toBe(400);
    expect(mapWithAnchors(600, anchors, 1000, 1200)).toBe(700);
  });

  it("interpolates inside a block that renders taller than its source", () => {
    // 원문으로 절반 지난 지점이면 렌더에서도 그 그림의 절반
    expect(mapWithAnchors(500, anchors, 1000, 1200)).toBe(550);
  });

  it("beats the plain ratio where the densities differ", () => {
    const byRatio = mappedScrollTop(600, 1000, 1200)!;
    const byAnchor = mapWithAnchors(600, anchors, 1000, 1200);
    expect(byRatio).toBe(720); // 비율은 그림을 지나쳐 20px 밀린다
    expect(byAnchor).toBe(700); // 대응점은 그림 끝에 정확히 선다
  });

  it("falls back to a plain ratio when there is nothing to anchor on", () => {
    expect(mapWithAnchors(500, [], 1000, 2000)).toBe(1000);
  });

  it("refuses panes that cannot scroll", () => {
    expect(mapWithAnchors(500, anchors, 0, 1200)).toBe(0);
  });
});
