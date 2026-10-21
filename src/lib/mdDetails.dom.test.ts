// @vitest-environment jsdom
//
// 접힌 토글의 몸통 판정 — 질문 형광펜이 이걸로 칠할지 말지를 고른다. WebKit 은 접힌 몸통 안의
// 구간을 숨기지 않고 다음에 보이는 글자(아래 토글 제목)에 칠했다. 제목 줄은 접혀도 보이므로
// "토글 안"이 아니라 "접힌 토글의 몸통 안"이어야 한다.

import { describe, expect, it } from "vitest";
import { inCollapsedBody } from "./mdDetails";

function toggle(open: boolean, title: string, inner: string): string {
  return `<div class="md-toggle" data-open="${open}"><button class="md-toggle-sum"><span class="md-toggle-title">${title}</span></button><div class="md-toggle-body"><div class="md-toggle-inner">${inner}</div></div></div>`;
}

function rangeOver(root: HTMLElement, id: string): Range {
  const r = document.createRange();
  r.selectNodeContents(root.querySelector(`#${id}`)!);
  return r;
}

describe("inCollapsedBody", () => {
  const root = document.createElement("div");
  root.innerHTML =
    toggle(false, '<b id="t1">미션 1</b>', '<p id="b1">본문</p>' + toggle(true, "Q1", '<p id="n1">안쪽</p>')) +
    toggle(true, "미션 2", '<p id="b2">본문</p>' + toggle(false, "Q2", '<p id="n2">안쪽</p>')) +
    '<p id="out">밖</p>';

  it("접힌 토글의 몸통 안이면 참", () => {
    expect(inCollapsedBody(rangeOver(root, "b1"))).toBe(true);
  });
  it("접힌 토글 안의 펼친 토글도 보이지 않는다", () => {
    expect(inCollapsedBody(rangeOver(root, "n1"))).toBe(true);
  });
  it("펼친 토글 안의 접힌 토글 몸통도 참", () => {
    expect(inCollapsedBody(rangeOver(root, "n2"))).toBe(true);
  });
  it("접힌 토글의 제목 줄은 보인다", () => {
    expect(inCollapsedBody(rangeOver(root, "t1"))).toBe(false);
  });
  it("펼친 토글의 몸통과 토글 밖은 보인다", () => {
    expect(inCollapsedBody(rangeOver(root, "b2"))).toBe(false);
    expect(inCollapsedBody(rangeOver(root, "out"))).toBe(false);
  });
  it("한쪽 끝만 접힌 몸통에 걸려도 참", () => {
    const r = document.createRange();
    r.setStart(root.querySelector("#out")!.firstChild!, 0);
    r.setEnd(root.querySelector("#out")!.firstChild!, 1);
    expect(inCollapsedBody(r)).toBe(false);
    r.setStart(root.querySelector("#b1")!.firstChild!, 0);
    expect(inCollapsedBody(r)).toBe(true);
  });
});
