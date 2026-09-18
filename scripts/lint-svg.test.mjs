import { describe, expect, it } from "vitest";
import { lintSvg, pathSegments, rowMismatches, segmentHitsBox, svgBlocks, textWidth } from "./lint-svg.mjs";

/** 같은 뼈대에 한 가지만 어기게 해서, 잡히는 게 그 한 가지인지 본다 */
const figure = (body, { w = 680, h = 200 } = {}) =>
  `<svg viewBox="0 0 ${w} ${h}" width="100%" role="img" aria-labelledby="t"><title id="t">t</title>${body}</svg>`;

const NODE = '<rect x="24" y="80" width="112" height="40" rx="4" stroke="currentColor" stroke-opacity="0.28"/>';
const NODE_R = '<rect x="240" y="80" width="112" height="40" rx="4" stroke="currentColor" stroke-opacity="0.28"/>';
const ARROW = '<g stroke="currentColor" stroke-opacity="0.62" stroke-width="1" fill="none"><path d="M136 100 H240"/><path d="m233 94 7 6 -7 6"/></g>';
const errors = (body, opts) => lintSvg(figure(body, opts)).issues;

describe("lint-svg — 잡아야 할 것", () => {
  it("깨끗한 그림은 아무것도 내지 않는다", () => {
    expect(errors(NODE + NODE_R + ARROW)).toEqual([]);
  });

  it("캔버스를 벗어난 글자", () => {
    expect(errors('<text x="8" y="196" font-size="11" opacity="0.62">바닥에 닿은 캡션</text>')).toEqual([
      expect.stringContaining("캔버스 벗어남"),
    ]);
  });

  it("상자 밖으로 삐져나온 글자", () => {
    expect(errors(NODE + '<text x="120" y="104" font-size="11" opacity="0.85">상자보다 긴 라벨이다</text>')).toContainEqual(
      expect.stringContaining("텍스트/상자 겹침"),
    );
  });

  it("글자를 가로지르는 선", () => {
    const line = '<line x1="24" y1="100" x2="300" y2="100" stroke="currentColor" stroke-opacity="0.28"/>';
    expect(errors(line + '<text x="120" y="104" font-size="11" opacity="0.62">선 위의 글자</text>')).toContainEqual(
      expect.stringContaining("선이 텍스트를 지나감"),
    );
  });

  it("겹쳐 놓인 두 글자", () => {
    const body = '<text x="24" y="100" font-size="11" opacity="0.62">앞의 라벨</text><text x="60" y="100" font-size="11" opacity="0.62">뒤의 라벨</text>';
    expect(errors(body)).toContainEqual(expect.stringContaining("텍스트끼리 겹침"));
  });

  it("상자를 관통하는 선", () => {
    const through = '<path d="M8 100 H400" stroke="currentColor" stroke-opacity="0.62" stroke-width="1" fill="none"/>';
    expect(errors(NODE + NODE_R + through)).toContainEqual(expect.stringContaining("선이 상자를 관통"));
  });

  it("상자 중앙에서 벗어난 화살표 끝점 (불변식 6)", () => {
    const slanted = '<g stroke="currentColor" stroke-opacity="0.62" stroke-width="1" fill="none"><path d="M136 92 H240"/></g>';
    expect(errors(NODE + NODE_R + slanted)).toEqual(["끝점 y=92 이 닿은 상자의 세로 중앙 100 과 어긋남"]);
  });

  it("이어 붙는 막대의 높이가 다르면 (불변식 5)", () => {
    const body = '<rect x="24" y="80" width="80" height="24" rx="0"/><rect x="104" y="80" width="80" height="16" rx="0"/>';
    expect(errors(body)).toContainEqual(expect.stringContaining("이어 붙는 사각형의 y/높이가 어긋남"));
  });

  it("이음매에 둥근 모서리가 남으면 (불변식 5)", () => {
    const body = '<rect x="24" y="80" width="80" height="24" rx="3"/><rect x="104" y="80" width="80" height="24" rx="3"/>';
    expect(errors(body)).toEqual([expect.stringContaining("이음매에 둥근 모서리")]);
    expect(errors(body.replaceAll('rx="3"', 'rx="0"'))).toEqual([]);
  });

  it("강조색이 칠해진 화살촉 (불변식 1)", () => {
    const body = NODE + NODE_R + ARROW.replace('d="m233 94 7 6 -7 6"', 'd="m233 94 7 6 -7 6" stroke="#ea580c"');
    expect(errors(body)).toEqual(["화살촉에 강조색 #ea580c — 연결선은 ink.muted 회색이다"]);
  });

  it("하이라이트는 하나, 전후 한 쌍까지 (불변식 2)", () => {
    const at = (x, hue) => `<rect x="${x}" y="80" width="112" height="40" rx="4" fill="${hue}" fill-opacity="0.08" stroke="${hue}" stroke-width="1.5"/>`;
    expect(errors(at(24, "#3b82f6"))).toEqual([]);
    expect(errors(at(24, "#ea580c") + at(240, "#3b82f6"))).toEqual([]); // 전후 한 쌍
    expect(errors(at(24, "#3b82f6") + at(240, "#3b82f6"))).toEqual([expect.stringContaining("하이라이트는 그림당 하나")]);
    expect(errors(at(24, "#3b82f6") + at(240, "#ea580c") + at(456, "#3b82f6"))).toEqual([expect.stringContaining("강조한 상자가 3개")]);
  });

  it("팔레트 밖의 색 (불변식 3)", () => {
    expect(errors('<rect x="24" y="80" width="80" height="24" fill="#ffffff"/>')).toContainEqual(
      expect.stringContaining("팔레트에 없는 색 #ffffff"),
    );
  });

  it("토큰에 없는 글자 크기와 톤", () => {
    expect(errors('<text x="24" y="100" font-size="13" opacity="0.62">큰 글자</text>')).toEqual([
      expect.stringContaining("글자 크기 13"),
    ]);
    expect(errors('<text x="24" y="100" font-size="11" opacity="0.5">어중간한 톤</text>')).toEqual([
      expect.stringContaining("글자 투명도 0.5"),
    ]);
  });
});

describe("lint-svg — 잡으면 안 되는 것", () => {
  it("상자 안에서 출발하는 선은 연결선이 아니다 (격자 스포크)", () => {
    const spoke = '<path d="M80 100 H240" stroke="currentColor" stroke-opacity="0.28" stroke-width="1" fill="none"/>';
    expect(errors(NODE + spoke)).toEqual([]);
  });

  it("폭이 1 이 아닌 화살촉은 연결선이 아니라 마크다 (축을 넘어간 구간 표시)", () => {
    const cap = '<path d="m600 94 7 6 -7 6" stroke="#ea580c" stroke-width="1.5" fill="none"/>';
    expect(errors(cap)).toEqual([]);
  });

  it("상자를 감싸는 group 은 관통 판정에서 빠진다", () => {
    const group = '<rect x="8" y="64" width="360" height="72" rx="8" stroke="currentColor" stroke-opacity="0.28" fill="none"/>';
    expect(errors(group + NODE + NODE_R + ARROW)).toEqual([]);
  });
});

describe("rowMismatches", () => {
  const text = (label, x, y, over = {}) => ({
    label,
    x,
    y,
    fs: 11.5,
    weight: "400",
    anchor: "middle",
    opacity: 0.62,
    box: [x - 40, y - 9, x + 40, y + 2],
    ...over,
  });

  it("같은 역할의 두 캡션이 16 단위 어긋나면 후보로 올린다", () => {
    expect(rowMismatches([text("왼쪽 캡션", 168, 184), text("오른쪽 캡션", 512, 200)])).toEqual([
      expect.stringContaining("한 줄로 읽히는데 y 가 어긋남"),
    ]);
  });

  it("y 가 같으면 조용하다", () => {
    expect(rowMismatches([text("왼쪽 캡션", 168, 200), text("오른쪽 캡션", 512, 200)])).toEqual([]);
  });

  it("아예 다른 줄이면 조용하다", () => {
    expect(rowMismatches([text("위", 168, 120), text("아래", 512, 200)])).toEqual([]);
  });

  it("역할이 다르면 비교하지 않는다", () => {
    expect(rowMismatches([text("캡션", 168, 184), text("값", 512, 200, { fs: 12, weight: "600" })])).toEqual([]);
  });

  it("한 칸에 두 줄짜리 라벨이면 줄 간격이지 어긋남이 아니다", () => {
    const stacked = [text("왼쪽 첫 줄", 168, 184), text("왼쪽 둘째 줄", 168, 200), text("오른쪽 첫 줄", 512, 184), text("오른쪽 둘째 줄", 512, 200)];
    expect(rowMismatches(stacked)).toEqual([]);
  });
});

describe("도우미", () => {
  it("한글은 라틴보다 넓게 어림한다", () => {
    expect(textWidth("가나다", 12)).toBeCloseTo(36);
    expect(textWidth("abc", 12)).toBeCloseTo(19.8);
  });

  it("path 의 d 를 직선 구간으로 읽는다", () => {
    expect(pathSegments("M136 100 H240 V60")).toEqual([
      [136, 100, 240, 100],
      [240, 100, 240, 60],
    ]);
    expect(pathSegments("M0 0 L10 10")).toEqual([[0, 0, 10, 10]]);
  });

  it("가로/세로 구간만 상자와 견준다", () => {
    const box = [10, 10, 50, 30];
    expect(segmentHitsBox(box, [0, 20, 100, 20])).toBe(true);
    expect(segmentHitsBox(box, [0, 40, 100, 40])).toBe(false);
    expect(segmentHitsBox(box, [0, 0, 100, 40])).toBe(false);
  });

  it("노트에서 svg 블록만 뽑는다", () => {
    const md = "글\n\n```svg\n<svg/>\n```\n\n```mermaid\ngraph TD\n```\n\n```svg\n<svg id=\"2\"/>\n```\n";
    expect(svgBlocks(md)).toEqual(["<svg/>", '<svg id="2"/>']);
  });
});
