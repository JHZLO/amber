import { describe, expect, it } from "vitest";
import { FIND_LIMIT, findInText } from "./pageFind";

describe("findInText", () => {
  it("모든 출현 자리를 [시작, 끝) 으로 준다", () => {
    expect(findInText("global 설정과 global 정책", "global")).toEqual([
      [0, 6],
      [11, 17],
    ]);
  });

  it("대소문자를 가리지 않되 오프셋은 원문 기준이다", () => {
    const text = "Global 과 GLOBAL";
    expect(findInText(text, "global")).toEqual([
      [0, 6],
      [9, 15],
    ]);
    expect(text.slice(9, 15)).toBe("GLOBAL");
  });

  it("겹치는 자리는 한 번만 — 찾은 뒤부터 다시 본다", () => {
    expect(findInText("aaaa", "aa")).toEqual([
      [0, 2],
      [2, 4],
    ]);
  });

  it("빈 검색어와 공백은 아무것도 찾지 않는다", () => {
    expect(findInText("아무 글", "")).toEqual([]);
    expect(findInText("아무 글", "   ")).toEqual([]);
  });

  it("검색어 앞뒤 공백은 떼고 찾는다 — 입력칸에서 흔한 실수다", () => {
    expect(findInText("한 줄", " 줄 ")).toEqual([[2, 3]]);
  });

  it("줄바꿈을 넘어 있는 글자도 오프셋이 맞는다", () => {
    const md = "# 제목\n\n본문에 global 하나";
    const [hit] = findInText(md, "global");
    expect(md.slice(hit[0], hit[1])).toBe("global");
  });

  it("상한을 넘기지 않는다 — 한 글자를 치면 수만 건이 된다", () => {
    expect(findInText("x".repeat(FIND_LIMIT + 50), "x")).toHaveLength(FIND_LIMIT);
  });

  it("toLowerCase 가 길이를 바꾸는 글자가 섞이면 원문 기준으로 찾는다", () => {
    // "İ".toLowerCase() 는 두 글자라 소문자 오프셋을 쓰면 뒤가 전부 밀린다
    const text = "İstanbul 과 global";
    const [hit] = findInText(text, "global");
    expect(text.slice(hit[0], hit[1])).toBe("global");
  });
});
