// findFreeSlot(빈 슬롯 찾기) 회귀 테스트. gap 경계 1분 차이와 하루 끝 클램프가 핵심 —
// 화면에선 "왜 여기 놓였지" 정도로만 보여서 눈으로는 안 잡힌다.
// nowMinute 는 Date 얇은 래퍼라 테스트하지 않는다.

import { describe, expect, it } from "vitest";
import { findFreeSlot } from "./timeBlocks";
import type { TimeBlock } from "../types";

const blk = (start_min: number, end_min: number): TimeBlock => ({
  id: start_min,
  date: "2026-07-27",
  start_min,
  end_min,
  title: "",
  todo_id: null,
  created_at: 0,
  updated_at: 0,
});

describe("findFreeSlot", () => {
  it("블록이 없으면 요청 시각 그대로", () => {
    expect(findFreeSlot([], 540, 60)).toBe(540);
  });

  it("이미 끝난 블록은 건너뛴다", () => {
    expect(findFreeSlot([blk(400, 500)], 540, 30)).toBe(540);
  });

  it("요청 시각이 블록 안이면 그 블록 뒤로 민다", () => {
    expect(findFreeSlot([blk(500, 600)], 540, 30)).toBe(600);
  });

  it("연속된 블록 체인을 끝까지 민다", () => {
    expect(findFreeSlot([blk(540, 600), blk(600, 660), blk(660, 720)], 540, 30)).toBe(720);
  });

  it("gap 이 딱 맞으면 거기에 넣는다 (경계 포함)", () => {
    expect(findFreeSlot([blk(540, 600), blk(660, 720)], 540, 60)).toBe(600);
  });

  it("gap 이 1분이라도 모자라면 건너뛴다", () => {
    expect(findFreeSlot([blk(540, 600), blk(660, 720)], 540, 61)).toBe(720);
  });

  it("겹쳐 있는(포함된) 블록이 커서를 되돌리지 않는다", () => {
    expect(findFreeSlot([blk(540, 700), blk(560, 580)], 540, 30)).toBe(700);
  });

  it("입력 순서와 무관하고, 입력 배열을 건드리지 않는다", () => {
    const blocks = [blk(660, 720), blk(540, 600)];
    expect(findFreeSlot(blocks, 540, 60)).toBe(600);
    expect(blocks.map((b) => b.start_min)).toEqual([660, 540]);
  });

  it("자정을 넘지 않게 시작 시각을 당긴다", () => {
    expect(findFreeSlot([], 1400, 60)).toBe(1380);
    expect(findFreeSlot([], 1439, 1)).toBe(1439);
  });

  it("하루가 꽉 찼으면 마지막 가능 시각에 겹쳐서 둔다 (lane 이 처리)", () => {
    expect(findFreeSlot([blk(0, 1440)], 540, 30)).toBe(1410);
  });
});
