import { describe, expect, it } from "vitest";
import { dropletPath, progressOf, thumbLength } from "./ScrollDroplet";

describe("progressOf", () => {
  it("맨 위는 0, 맨 아래는 1", () => {
    expect(progressOf(0, 1000, 400)).toBe(0);
    expect(progressOf(600, 1000, 400)).toBe(1);
  });

  it("가운데는 비율 그대로", () => {
    expect(progressOf(300, 1000, 400)).toBeCloseTo(0.5);
  });

  it("넘치는 게 없으면 표시하지 않는다", () => {
    // 스크롤할 게 없는데 물방울이 떠 있으면 그건 장식이다
    expect(progressOf(0, 400, 400)).toBeNull();
    expect(progressOf(0, 402, 400)).toBeNull(); // 한두 px 오차는 스크롤이 아니다
  });

  it("고무줄 스크롤로 범위를 넘겨도 0~1 밖으로 안 나간다", () => {
    // 트랙패드 관성은 scrollTop 을 음수나 최대값 너머로 잠깐 보낸다
    expect(progressOf(-40, 1000, 400)).toBe(0);
    expect(progressOf(900, 1000, 400)).toBe(1);
  });
});

describe("thumbLength", () => {
  it("보이는 만큼의 비율이다 — 절반만 보이면 절반 길이", () => {
    expect(thumbLength(400, 500, 1000)).toBe(200);
  });

  it("글이 길수록 짧아진다", () => {
    const short = thumbLength(400, 500, 1000);
    const long = thumbLength(400, 500, 4000);
    expect(long).toBeLessThan(short);
  });

  it("아무리 길어도 점이 되지는 않는다", () => {
    // 28px 아래로 내려가면 물방울 모양이 사라진다
    expect(thumbLength(400, 500, 100000)).toBe(28);
  });

  it("트랙보다 길어지지 않는다", () => {
    expect(thumbLength(400, 900, 1000)).toBeLessThanOrEqual(400);
  });
});

describe("dropletPath", () => {
  it("위에서 시작해 아래에서 둥글게 닫힌다", () => {
    const d = dropletPath(9, 40);
    expect(d.startsWith("M 4.5 0")).toBe(true); // 머리는 한 점
    expect(d).toContain("A 4.5 4.5"); // 꼬리는 반원
    expect(d.endsWith("Z")).toBe(true);
  });

  it("길이가 달라져도 머리와 꼬리의 생김새는 같다", () => {
    const a = dropletPath(9, 30);
    const b = dropletPath(9, 200);
    expect(a.startsWith("M 4.5 0")).toBe(true);
    expect(b.startsWith("M 4.5 0")).toBe(true);
    expect(a).toContain("A 4.5 4.5");
    expect(b).toContain("A 4.5 4.5");
  });

  it("아주 짧아도 꼬리가 몸을 넘지 않는다", () => {
    // belly 가 h - r 을 넘으면 경로가 뒤집혀 모양이 깨진다
    const d = dropletPath(9, 10);
    expect(d).not.toMatch(/-\d/); // 음수 좌표가 없다
  });
});
