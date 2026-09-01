import { describe, expect, it } from "vitest";
import { mappedScrollTop } from "./useScrollSync";

describe("mappedScrollTop", () => {
  it("같은 비율의 위치로 옮긴다", () => {
    expect(mappedScrollTop(50, 100, 400)).toBe(200);
    expect(mappedScrollTop(0, 100, 400)).toBe(0);
    expect(mappedScrollTop(100, 100, 400)).toBe(400);
  });

  it("못 굴리는 칸이 있으면 null — 0 으로 나눠 NaN 을 대입하지 않는다", () => {
    expect(mappedScrollTop(0, 0, 400)).toBeNull();
    expect(mappedScrollTop(50, 100, 0)).toBeNull();
  });

  it("범위를 넘은 위치(고무줄 스크롤)는 끝으로 clamp 한다", () => {
    expect(mappedScrollTop(-30, 100, 400)).toBe(0);
    expect(mappedScrollTop(130, 100, 400)).toBe(400);
  });
});
