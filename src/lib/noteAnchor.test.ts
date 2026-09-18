import { describe, expect, it } from "vitest";
import { mergeLineRects, type Rect } from "./noteAnchor";

const r = (top: number, left: number, width: number, height = 20): Rect => ({ top, left, width, height });

describe("mergeLineRects", () => {
  it("한 줄 안에서 굵은 글씨로 쪼개진 조각을 하나로 합친다", () => {
    // "앞 **굵게** 뒤" 한 줄 → getClientRects 는 조각 3개를 준다
    expect(mergeLineRects([r(100, 40, 60), r(100, 100, 80), r(100, 180, 50)])).toEqual([
      r(100, 40, 190),
    ]);
  });

  it("줄이 다르면 합치지 않는다", () => {
    const out = mergeLineRects([r(100, 40, 300), r(128, 40, 200)]);
    expect(out).toHaveLength(2);
    expect(out[1].top).toBe(128);
  });

  it("조각 순서가 뒤섞여 와도 줄 순서로 돌려준다", () => {
    const out = mergeLineRects([r(128, 90, 60), r(100, 140, 40), r(100, 40, 100), r(128, 40, 50)]);
    expect(out).toEqual([r(100, 40, 140), r(128, 40, 110)]);
  });

  it("글자 크기가 섞여도 절반 넘게 겹치면 같은 줄이다", () => {
    // 위첨자처럼 낮고 살짝 올라간 조각 — 줄을 새로 만들면 형광펜이 두 겹으로 겹친다
    const out = mergeLineRects([r(100, 40, 60, 20), r(104, 100, 20, 12)]);
    expect(out).toHaveLength(1);
    expect(out[0]).toEqual({ top: 100, left: 40, width: 80, height: 20 });
  });

  it("넓이나 높이가 0 인 조각은 버린다", () => {
    expect(mergeLineRects([r(100, 40, 0), r(100, 40, 60), { top: 100, left: 40, width: 30, height: 0 }])).toEqual([
      r(100, 40, 60),
    ]);
  });

  it("빈 입력은 빈 결과", () => {
    expect(mergeLineRects([])).toEqual([]);
  });
});
