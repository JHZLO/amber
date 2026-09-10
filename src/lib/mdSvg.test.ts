import { describe, expect, it } from "vitest";
import { isDangerousAttr, isSafeHref, looksLikeSvg, remarkSvgHtml, styleIsSafe } from "./mdSvg";

describe("looksLikeSvg", () => {
  it("accepts a whole svg block and rejects partial or other html", () => {
    expect(looksLikeSvg('<svg viewBox="0 0 10 10"><rect/></svg>')).toBe(true);
    expect(looksLikeSvg("\n<svg>\n</svg>\n")).toBe(true);
    expect(looksLikeSvg("<svgfoo></svgfoo>")).toBe(false);
    expect(looksLikeSvg("<div><svg></svg></div>")).toBe(false);
    expect(looksLikeSvg("<svg><rect/>")).toBe(false);
  });
});

describe("isSafeHref / styleIsSafe / isDangerousAttr", () => {
  // 스타일 가이드의 불투명 텍스트 판(plate)은 이 문법에 기댄다 — 여기서 막히면 노트가 선을 뚫고 보인다
  it("keeps the theme-following plate fill the SVG style guide relies on", () => {
    expect(styleIsSafe("fill: var(--surface, #ffffff)")).toBe(true);
    expect(isDangerousAttr("style", "fill: var(--surface, #ffffff)")).toBe(false);
  });
  it("allows fragment, http(s) and data images only", () => {
    expect(isSafeHref("#grad")).toBe(true);
    expect(isSafeHref("https://example.com")).toBe(true);
    expect(isSafeHref("data:image/png;base64,AAAA")).toBe(true);
    expect(isSafeHref("javascript:alert(1)")).toBe(false);
    expect(isSafeHref("file:///etc/passwd")).toBe(false);
    expect(isSafeHref("data:text/html,<script>")).toBe(false);
  });

  it("blocks styles that can reach outside the document", () => {
    expect(styleIsSafe("font-family: system-ui; max-width: 660px")).toBe(true);
    expect(styleIsSafe("background: url(https://x/y.png)")).toBe(false);
    expect(styleIsSafe("@import 'x.css'")).toBe(false);
  });

  it("flags event handlers, bad hrefs and unsafe styles", () => {
    expect(isDangerousAttr("onload", "alert(1)")).toBe(true);
    expect(isDangerousAttr("href", "javascript:1")).toBe(true);
    expect(isDangerousAttr("xlink:href", "#a")).toBe(false);
    expect(isDangerousAttr("style", "fill: red; background: url(x)")).toBe(true);
    expect(isDangerousAttr("fill", "currentColor")).toBe(false);
  });
});

describe("remarkSvgHtml", () => {
  it("turns a raw svg html block into a ```svg code node and leaves other html alone", () => {
    const svg = '<svg viewBox="0 0 1 1"></svg>';
    const tree = {
      type: "root",
      children: [
        { type: "html", value: `\n${svg}\n` },
        { type: "html", value: "<br/>" },
        { type: "paragraph", children: [{ type: "text", value: "x" }] },
      ],
    };
    remarkSvgHtml()(tree);
    expect(tree.children[0]).toEqual({ type: "code", lang: "svg", value: svg });
    expect(tree.children[1]).toEqual({ type: "html", value: "<br/>" });
    expect(tree.children[2].type).toBe("paragraph");
  });
});
