// layoutColumn(겹침 배치) 회귀 테스트. 눈으로는 "좀 좁네" 정도로만 보여서 안 잡히는 규칙들 —
// 포함/교차 구분, 중첩 깊이, 좁은 컬럼에서의 강등이 핵심.
// 특히 두 불변식을 지킨다: (1) 어떤 블록도 사라지지 않는다, (2) 같은 시간에 두 블록이
// 완전히 같은 자리에 놓이지 않는다. 둘 중 하나라도 깨지면 사용자는 블록을 잡을 수 없다.

import { describe, expect, it } from "vitest";
import {
  GHOST_ID,
  GUTTER,
  MAX_DEPTH,
  boxLeft,
  boxWidth,
  layoutColumn,
  withGhost,
  type Box,
} from "./ttLayout";
import type { TimeBlock } from "../types";

let seq = 0;
const blk = (start_min: number, end_min: number, id = ++seq): TimeBlock => ({
  id,
  date: "2026-08-06",
  start_min,
  end_min,
  title: "",
  todo_id: null,
  created_at: 0,
  updated_at: 0,
});

const H = (h: number, m = 0) => h * 60 + m;

// 일 뷰: 컬럼 1개, 그리드 1170px
const DAY_W = 1170;
const dayRoot = (): Box => ({ pctL: 0, pxL: 0, pctW: 100, pxW: -GUTTER });
// 주 뷰: 컬럼 7개. gridW 를 좁게 줘서 lane 하한에 걸리는 상황을 만든다
const WEEK_W = 630;
const weekRoot = (col: number): Box => ({
  pctL: (col * 100) / 7,
  pxL: 0,
  pctW: 100 / 7,
  pxW: -GUTTER,
});

/** 배치 결과를 id → {left, width, depth} 로 (px 로 환산해 비교하기 쉽게) */
const px = (box: Box, gridW: number) => ({
  left: (gridW * box.pctL) / 100 + box.pxL,
  width: (gridW * box.pctW) / 100 + box.pxW,
});

describe("layoutColumn — 거터", () => {
  it("겹침이 없어도 컬럼 우측 12px 는 항상 비운다 (겹쳐 만들기 드래그가 닿을 표면)", () => {
    const [p] = layoutColumn([blk(H(9), H(18))], dayRoot(), DAY_W);
    expect(px(p.box, DAY_W)).toEqual({ left: 0, width: DAY_W - GUTTER });
    expect(p.depth).toBe(0);
  });
});

describe("layoutColumn — 포함은 캐스케이드", () => {
  it("09–18 안의 10–11: 큰 블록은 폭을 잃지 않고, 안쪽이 14px 들여써서 얹힌다", () => {
    const outer = blk(H(9), H(18));
    const inner = blk(H(10), H(11));
    const out = layoutColumn([outer, inner], dayRoot(), DAY_W);
    const byId = new Map(out.map((p) => [p.b.id, p]));

    // 이게 이 기능의 핵심: 회의 하나 때문에 하루짜리 블록이 반쪽이 되지 않는다
    expect(px(byId.get(outer.id)!.box, DAY_W)).toEqual({
      left: 0,
      width: DAY_W - GUTTER,
    });
    expect(byId.get(outer.id)!.depth).toBe(0);

    const i = px(byId.get(inner.id)!.box, DAY_W);
    expect(i.left).toBe(14);
    expect(byId.get(inner.id)!.depth).toBe(1);
    // 오른쪽 끝은 바깥 블록과 같은 선에서 맞춘다 (들여쓰기는 왼쪽만)
    expect(i.left + i.width).toBe(DAY_W - GUTTER);
  });

  it("중첩 포함은 깊이가 쌓인다 (09–18 ⊃ 10–14 ⊃ 11–12)", () => {
    const a = blk(H(9), H(18));
    const b = blk(H(10), H(14));
    const c = blk(H(11), H(12));
    const out = layoutColumn([a, b, c], dayRoot(), DAY_W);
    const d = new Map(out.map((p) => [p.b.id, p.depth]));
    expect([d.get(a.id), d.get(b.id), d.get(c.id)]).toEqual([0, 1, 2]);
  });

  it("부모는 '가장 작은' 감싸는 블록이다 — 할아버지가 아니라", () => {
    const a = blk(H(9), H(18));
    const b = blk(H(10), H(14));
    const c = blk(H(11), H(12));
    const out = layoutColumn([a, b, c], dayRoot(), DAY_W);
    const byId = new Map(out.map((p) => [p.b.id, p]));
    // c 가 a 의 직속 자식이었다면 깊이 1, 들여쓰기 14px 에 그친다
    expect(px(byId.get(c.id)!.box, DAY_W).left).toBe(28);
  });

  it("들여쓰기는 MAX_DEPTH 에서 멈춘다 (폭이 0 으로 수렴하지 않게)", () => {
    const nest = Array.from({ length: 7 }, (_, i) =>
      blk(H(9) + i * 10, H(18) - i * 10),
    );
    const out = layoutColumn(nest, dayRoot(), DAY_W);
    const lefts = out.map((p) => px(p.box, DAY_W).left);
    expect(Math.max(...lefts)).toBe(MAX_DEPTH * 14);
    // 깊이(z 순서)는 계속 올라간다
    expect(Math.max(...out.map((p) => p.depth))).toBe(6);
  });
});

describe("layoutColumn — 교차는 lane 분할", () => {
  it("09–12 와 11–14 는 서로를 품지 않으니 폭을 반씩 나눈다", () => {
    const a = blk(H(9), H(12));
    const b = blk(H(11), H(14));
    const out = layoutColumn([a, b], dayRoot(), DAY_W);
    const byId = new Map(out.map((p) => [p.b.id, p]));
    const interior = DAY_W - GUTTER;

    expect(byId.get(a.id)!.depth).toBe(0);
    expect(byId.get(b.id)!.depth).toBe(0);
    expect(px(byId.get(a.id)!.box, DAY_W)).toEqual({
      left: 0,
      width: interior / 2,
    });
    expect(px(byId.get(b.id)!.box, DAY_W)).toEqual({
      left: interior / 2,
      width: interior / 2,
    });
  });

  it("범위가 완전히 같으면 포함이 아니라 교차 — 나란히 놓는다", () => {
    const a = blk(H(9), H(12));
    const b = blk(H(9), H(12));
    const out = layoutColumn([a, b], dayRoot(), DAY_W);
    expect(out.every((p) => p.depth === 0)).toBe(true);
    expect(new Set(out.map((p) => p.box.pctL)).size).toBe(2);
  });

  it("겹치지 않는 블록은 각자 컬럼을 다 쓴다 (묶음이 따로 끊긴다)", () => {
    const out = layoutColumn(
      [blk(H(9), H(10)), blk(H(11), H(12))],
      dayRoot(),
      DAY_W,
    );
    for (const p of out) {
      expect(px(p.box, DAY_W).width).toBe(DAY_W - GUTTER);
    }
  });

  it("포함된 자식들끼리 교차하면 부모 안에서 lane 을 나눈다", () => {
    const parent = blk(H(9), H(18));
    const x = blk(H(10), H(12));
    const y = blk(H(11), H(13));
    const out = layoutColumn([parent, x, y], dayRoot(), DAY_W);
    const byId = new Map(out.map((p) => [p.b.id, p]));
    const inner = DAY_W - GUTTER - 14; // 부모 상자에서 한 번 들여쓴 폭

    expect(byId.get(x.id)!.depth).toBe(1);
    expect(byId.get(y.id)!.depth).toBe(1);
    expect(px(byId.get(x.id)!.box, DAY_W).width).toBe(inner / 2);
    expect(px(byId.get(y.id)!.box, DAY_W).left).toBe(14 + inner / 2);
  });
});

describe("layoutColumn — 좁은 컬럼 강등", () => {
  // 주 뷰 컬럼 90px, 거터 빼면 78px → lane 하한(56px)에 걸려 1 lane 만 허용된다
  it("lane 을 더 못 쪼개면 얹되, 얹힌 것끼리도 단계를 벌린다", () => {
    const a = blk(H(9), H(12));
    const b = blk(H(11), H(14));
    const c = blk(H(13), H(16));
    const out = layoutColumn([a, b, c], weekRoot(0), WEEK_W);
    const d = new Map(out.map((p) => [p.b.id, p.depth]));

    expect(d.get(a.id)).toBe(0);
    expect(d.get(b.id)).toBe(1);
    // c 는 b 와 겹치므로(13–14) b 와 같은 단계에 얹히면 통째로 가려진다
    expect(d.get(c.id)).toBe(2);
  });

  it("겹치지 않는 얹힘은 같은 단계를 재사용한다 (쓸데없이 깊어지지 않게)", () => {
    const a = blk(H(9), H(18)); // 컬럼을 붙잡고 있는 lane 0
    const b = blk(H(10), H(11));
    const c = blk(H(12), H(13));
    // b·c 는 a 안에 '포함'이라 캐스케이드 — 서로는 안 겹치니 둘 다 깊이 1
    const out = layoutColumn([a, b, c], weekRoot(0), WEEK_W);
    const d = new Map(out.map((p) => [p.b.id, p.depth]));
    expect([d.get(b.id), d.get(c.id)]).toEqual([1, 1]);
  });

  it("컬럼이 넓으면 강등하지 않고 그냥 lane 을 나눈다", () => {
    const out = layoutColumn(
      [blk(H(9), H(12)), blk(H(11), H(14)), blk(H(13), H(16))],
      dayRoot(),
      DAY_W,
    );
    expect(out.every((p) => p.depth === 0)).toBe(true);
  });
});

describe("withGhost — 드래그 중 라이브 배치", () => {
  const D = "2026-08-06";

  it("드래그 중이 아니면 원본 배열 그대로 (쓸데없는 리렌더 방지)", () => {
    const blocks = [blk(H(9), H(12))];
    expect(withGhost(blocks, null)).toBe(blocks);
  });

  it("만드는 중인 범위도 배치에 참여한다", () => {
    const out = withGhost([blk(H(9), H(12))], {
      id: null,
      date: D,
      start: H(10),
      end: H(14),
    });
    expect(out).toHaveLength(2);
    expect(out[1]).toMatchObject({
      id: GHOST_ID,
      date: D,
      start_min: H(10),
      end_min: H(14),
    });
  });

  it("옮기는 중인 블록은 원본을 건드리지 않고 좌표만 갈아끼운다", () => {
    const b = blk(H(9), H(12));
    const blocks = [b];
    const out = withGhost(blocks, {
      id: b.id,
      date: "2026-08-07",
      start: H(13),
      end: H(16),
    });
    expect(out[0]).toMatchObject({
      id: b.id,
      date: "2026-08-07",
      start_min: H(13),
      end_min: H(16),
    });
    expect(b.start_min).toBe(H(9)); // 원본 불변 (커밋은 mouseup 에서 한 번)
  });

  it("남의 시간대를 침범하는 순간 둘 다 2열이 된다 — 놓기 전에", () => {
    const parked = blk(H(9), H(12));
    const dragged = blk(H(14), H(17));
    const blocks = [parked, dragged];
    const interior = DAY_W - GUTTER;

    // 침범 전: 서로 안 겹치니 각자 컬럼을 다 쓴다
    const before = layoutColumn(blocks, dayRoot(), DAY_W);
    for (const p of before) expect(px(p.box, DAY_W).width).toBe(interior);

    // 드래그로 09–12 위에 올라탄 순간
    const during = layoutColumn(
      withGhost(blocks, {
        id: dragged.id,
        date: D,
        start: H(11),
        end: H(14),
      }),
      dayRoot(),
      DAY_W,
    );
    const byId = new Map(during.map((p) => [p.b.id, p]));
    expect(px(byId.get(parked.id)!.box, DAY_W)).toEqual({
      left: 0,
      width: interior / 2,
    });
    expect(px(byId.get(dragged.id)!.box, DAY_W)).toEqual({
      left: interior / 2,
      width: interior / 2,
    });
  });

  it("감싸는 범위로 끌면 2열이 아니라 캐스케이드로 미리 보인다", () => {
    const outer = blk(H(9), H(18));
    const dragged = blk(H(20), H(21));
    const during = layoutColumn(
      withGhost([outer, dragged], {
        id: dragged.id,
        date: D,
        start: H(10),
        end: H(11),
      }),
      dayRoot(),
      DAY_W,
    );
    const byId = new Map(during.map((p) => [p.b.id, p]));
    expect(byId.get(outer.id)!.depth).toBe(0);
    expect(px(byId.get(outer.id)!.box, DAY_W).width).toBe(DAY_W - GUTTER);
    expect(byId.get(dragged.id)!.depth).toBe(1);
  });

  it("빠져나오면 원래 폭으로 되돌아간다 (밀림이 되돌려진다)", () => {
    const parked = blk(H(9), H(12));
    const dragged = blk(H(11), H(14));
    const out = layoutColumn(
      withGhost([parked, dragged], {
        id: dragged.id,
        date: D,
        start: H(15),
        end: H(18),
      }),
      dayRoot(),
      DAY_W,
    );
    for (const p of out) {
      expect(px(p.box, DAY_W).width).toBe(DAY_W - GUTTER);
    }
  });
});

describe("boxLeft / boxWidth — CSS 합성", () => {
  it("부호를 직접 붙인다 — `calc(100% + -14px)` 같은 꼴을 만들지 않는다", () => {
    const out = layoutColumn(
      [blk(H(9), H(18)), blk(H(10), H(11))],
      dayRoot(),
      DAY_W,
    );
    const css = out.flatMap((p) => [boxLeft(p.box), boxWidth(p.box)]);
    for (const s of css) {
      expect(s).not.toMatch(/[+-]\s*-/);
      expect(s).toMatch(/^calc\(-?[\d.]+% [+-] [\d.]+px\)$/);
    }
  });

  it("주 뷰의 나눗셈 꼬리를 반올림해 CSS 를 짧게 유지한다", () => {
    const [p] = layoutColumn([blk(H(9), H(18))], weekRoot(3), WEEK_W);
    // 300/7 = 42.857142857142854 → 소수 4자리
    expect(boxLeft(p.box)).toBe("calc(42.8571% + 0px)");
    expect(boxWidth(p.box)).toBe("calc(14.2857% - 14px)");
  });

  it("겹침이 없으면 컬럼에서 거터+간격만 뺀다", () => {
    const [p] = layoutColumn([blk(H(9), H(18))], dayRoot(), DAY_W);
    expect(boxLeft(p.box)).toBe("calc(0% + 0px)");
    expect(boxWidth(p.box)).toBe("calc(100% - 14px)");
  });
});

describe("layoutColumn — 불변식", () => {
  const cases: [string, TimeBlock[]][] = [
    ["빈 날", []],
    ["하나", [blk(H(9), H(18))]],
    ["포함", [blk(H(9), H(18)), blk(H(10), H(11))]],
    ["교차", [blk(H(9), H(12)), blk(H(11), H(14))]],
    [
      "뒤섞임",
      [
        blk(H(8), H(20)),
        blk(H(9), H(10)),
        blk(H(9, 30), H(11)),
        blk(H(10), H(10, 30)),
        blk(H(13), H(17)),
        blk(H(14), H(15)),
        blk(H(14), H(15)),
      ],
    ],
  ];

  for (const [name, blocks] of cases) {
    it(`${name}: 어떤 블록도 사라지지 않는다`, () => {
      for (const [w, root] of [
        [DAY_W, dayRoot()],
        [WEEK_W, weekRoot(3)],
      ] as const) {
        const out = layoutColumn(blocks, root, w);
        expect(out).toHaveLength(blocks.length);
        expect(new Set(out.map((p) => p.b.id)).size).toBe(blocks.length);
      }
    });

    it(`${name}: 겹치는 두 블록이 같은 자리에 놓이지 않는다`, () => {
      const out = layoutColumn(blocks, dayRoot(), DAY_W);
      const overlaps = (x: TimeBlock, y: TimeBlock) =>
        x.start_min < y.end_min && y.start_min < x.end_min;
      for (const p of out) {
        for (const q of out) {
          if (p.b.id >= q.b.id || !overlaps(p.b, q.b)) continue;
          const same =
            p.box.pctL === q.box.pctL &&
            p.box.pxL === q.box.pxL &&
            p.box.pctW === q.box.pctW &&
            p.box.pxW === q.box.pxW;
          expect(same, `${p.b.id} 와 ${q.b.id} 가 완전히 포개짐`).toBe(false);
        }
      }
    });

    it(`${name}: 컬럼 밖으로 나가지 않는다`, () => {
      const out = layoutColumn(blocks, dayRoot(), DAY_W);
      for (const p of out) {
        const { left, width } = px(p.box, DAY_W);
        expect(left).toBeGreaterThanOrEqual(0);
        expect(left + width).toBeLessThanOrEqual(DAY_W - GUTTER + 0.001);
      }
    });
  }
});
