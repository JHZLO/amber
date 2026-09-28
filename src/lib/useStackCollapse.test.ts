// 카드 묶음 휠 규칙 — "리포트를 굴리면 리포트가 커진다"가 깨지지 않게 고정한다.
// 예전 구현은 리포트가 할 일 카드 아래 좁은 칸에서만 굴러 긴 리포트를 작은 창으로 읽어야 했다.

import { describe, expect, it } from "vitest";
import { planWheel, type WheelInput } from "./useStackCollapse";

const base: WheelInput = {
  dy: 0,
  collapse: 0,
  max: 300,
  zone: "report",
  innerTop: 0,
  innerBelow: 0,
  reportBelow: 1000,
};
const plan = (over: Partial<WheelInput>) => planWheel({ ...base, ...over });

describe("아래로 굴릴 때", () => {
  it("리포트 위에서는 리포트 안보다 할 일 카드를 먼저 밀어 올린다 — 리포트가 커진다", () => {
    const p = plan({ dy: 80, innerBelow: 900 });
    expect(p).toEqual({ collapse: 80, consumed: true, spill: 0 });
  });

  it("다 접고 남은 만큼은 리포트 본문으로 넘긴다", () => {
    const p = plan({ dy: 120, collapse: 250 });
    expect(p).toEqual({ collapse: 300, consumed: true, spill: 70 });
  });

  it("다 접힌 뒤에는 리포트가 스스로 구른다(기본 스크롤)", () => {
    const p = plan({ dy: 80, collapse: 300, innerBelow: 500 });
    expect(p.consumed).toBe(false);
    expect(p.collapse).toBe(300);
  });

  it("리포트가 이미 다 보이면 할 일을 치우지 않는다 — 빈 판만 늘어난다", () => {
    expect(plan({ dy: 80, reportBelow: 0 }).consumed).toBe(false);
  });

  it("리포트가 조금만 숨어 있으면 그만큼만 접는다", () => {
    expect(plan({ dy: 80, reportBelow: 30 }).collapse).toBe(30);
  });

  it("할 일 카드 위에서는 목록이 먼저 구르고, 목록 끝에 닿으면 접기 시작한다", () => {
    expect(plan({ dy: 80, zone: "card", innerBelow: 200 }).consumed).toBe(false);
    const p = plan({ dy: 80, zone: "card", innerBelow: 0 });
    expect(p.collapse).toBe(80);
    expect(p.spill).toBe(0); // 할 일 카드 위의 휠을 리포트 본문으로 흘리지는 않는다
  });
});

describe("위로 굴릴 때", () => {
  it("리포트가 맨 위에 닿기 전에는 리포트가 먼저 구른다", () => {
    const p = plan({ dy: -80, collapse: 300, innerTop: 400 });
    expect(p.consumed).toBe(false);
    expect(p.collapse).toBe(300);
  });

  it("리포트가 맨 위면 할 일 카드가 내려온다", () => {
    expect(plan({ dy: -80, collapse: 300 })).toEqual({ collapse: 220, consumed: true, spill: 0 });
  });

  it("다 펼쳐지면 더 먹지 않는다", () => {
    expect(plan({ dy: -500, collapse: 100 }).collapse).toBe(0);
    expect(plan({ dy: -80, collapse: 0 }).consumed).toBe(false);
  });
});

describe("경계", () => {
  it("할 일 카드가 줄어 최대치가 작아졌으면 그 안으로 되돌린다", () => {
    expect(plan({ dy: 10, collapse: 500, max: 200 }).collapse).toBe(200);
  });

  it("리포트가 없으면(최대 0) 접지 않는다", () => {
    expect(plan({ dy: 80, max: 0 }).consumed).toBe(false);
  });
});
