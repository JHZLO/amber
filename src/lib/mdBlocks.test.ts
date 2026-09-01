import { describe, expect, it } from "vitest";
import { readSpan, srcAttrs, unionSpan } from "./mdBlocks";

const fake = (attrs: Record<string, string>) => ({
  getAttribute: (n: string) => attrs[n] ?? null,
});

describe("readSpan", () => {
  it("심긴 소스 좌표를 읽는다", () => {
    expect(readSpan(fake({ "data-md-start": "12", "data-md-end": "40" }))).toEqual({
      start: 12,
      end: 40,
    });
  });

  it("속성이 없거나 수가 아니면 null — NaN 구간으로 본문을 자르지 않게", () => {
    expect(readSpan(null)).toBeNull();
    expect(readSpan(fake({}))).toBeNull();
    expect(readSpan(fake({ "data-md-start": "x", "data-md-end": "40" }))).toBeNull();
    expect(readSpan(fake({ "data-md-start": "12" }))).toBeNull();
  });

  it("빈 구간·거꾸로 된 구간은 null", () => {
    expect(readSpan(fake({ "data-md-start": "40", "data-md-end": "40" }))).toBeNull();
    expect(readSpan(fake({ "data-md-start": "40", "data-md-end": "12" }))).toBeNull();
  });
});

describe("unionSpan", () => {
  it("두 블록에 걸친 선택은 둘을 아우른다", () => {
    expect(unionSpan({ start: 10, end: 20 }, { start: 30, end: 45 })).toEqual({
      start: 10,
      end: 45,
    });
  });

  it("끝점 순서가 뒤바뀌어 있어도 같은 구간", () => {
    expect(unionSpan({ start: 30, end: 45 }, { start: 10, end: 20 })).toEqual({
      start: 10,
      end: 45,
    });
  });

  it("한쪽만 블록이면 그쪽을 쓴다", () => {
    expect(unionSpan(null, { start: 5, end: 9 })).toEqual({ start: 5, end: 9 });
    expect(unionSpan({ start: 5, end: 9 }, null)).toEqual({ start: 5, end: 9 });
    expect(unionSpan(null, null)).toBeNull();
  });
});

describe("srcAttrs", () => {
  const md = "# 제목\n\n문단.\n\n> [!NOTE]\n> 알림.\n";
  const at = (start: number, end: number) => ({ position: { start: { offset: start }, end: { offset: end } } });

  it("줄 경계에서 시작·끝나는 블록은 좌표를 심는다", () => {
    expect(srcAttrs(at(0, 4), md)).toEqual({ "data-md-start": 0, "data-md-end": 4 });
    expect(srcAttrs(at(6, 9), md)).toEqual({ "data-md-start": 6, "data-md-end": 9 });
    // 문서 끝까지 가는 블록도 끝이 줄 경계다
    expect(srcAttrs(at(11, md.length), md)).toEqual({
      "data-md-start": 11,
      "data-md-end": md.length,
    });
  });

  it("줄 중간에서 시작하는 블록은 심지 않는다 — `> `·`- ` 접두사가 남아 깨진다", () => {
    // 인용구 안 문단(`> ` 뒤에서 시작)
    expect(srcAttrs(at(13, 21), md)).toEqual({});
  });

  it("좌표가 없거나 거꾸로면 심지 않는다", () => {
    expect(srcAttrs(undefined, md)).toEqual({});
    expect(srcAttrs({}, md)).toEqual({});
    expect(srcAttrs(at(9, 9), md)).toEqual({});
    expect(srcAttrs(at(9, 4), md)).toEqual({});
  });
});
