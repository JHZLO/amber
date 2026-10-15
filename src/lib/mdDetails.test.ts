// 토글 변환 회귀 테스트. 손으로 만든 트리가 아니라 **실제 파서**를 거친다 — 토글은 CommonMark 이
// html 블록을 빈 줄에서 끊는 방식에 기대므로, 그 끊김이 바뀌면 손 트리 테스트는 그대로 통과한다.
// 카드(Markdown.tsx 의 MdToggle)는 여기서 그리지 않는다 — 플러그인이 넘기는 속성까지만 본다.

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { describe, expect, it } from "vitest";
import { detailsTokens, parseBlocks, remarkDetails } from "./mdDetails";

const render = (md: string) =>
  renderToStaticMarkup(
    createElement(ReactMarkdown, { remarkPlugins: [remarkGfm, remarkDetails], children: md }),
  ).replace(/\n/g, "");

/** 첫 토글의 속성 하나 */
const attr = (html: string, name: string) =>
  new RegExp(`<details[^>]*\\s${name}="([^"]*)"`).exec(html)?.[1];

describe("remarkDetails", () => {
  it("GitHub 모양: 제목과 마크다운 본문을 토글로 묶는다", () => {
    const html = render("<details>\n<summary>제목</summary>\n\n본문 **굵게**\n\n</details>");
    expect(attr(html, "data-summary")).toBe("제목");
    // 본문은 마크다운으로 읽혀야 한다 — 글자 그대로의 `**` 가 보이면 실패
    expect(html).toMatch(/<details[^>]*><p>본문 <strong>굵게<\/strong><\/p><\/details>/);
    expect(html).not.toContain("&lt;details");
    expect(html).not.toContain("<summary");
  });

  it("open 속성을 지킨다", () => {
    expect(render("<details open>\n<summary>T</summary>\n\n본문\n\n</details>")).toMatch(
      /<details[^>]*\sopen=""/,
    );
    expect(render("<details>\n<summary>T</summary>\n\n본문\n\n</details>")).not.toMatch(/\sopen=/);
  });

  it("안에 든 것을 종류별로 센다: mermaid, svg 는 코드가 아니라 그림", () => {
    const md = [
      "<details>",
      "<summary>T</summary>",
      "",
      "문단 하나",
      "",
      "- 목록",
      "",
      "```ts",
      "const x = 1",
      "```",
      "",
      "```mermaid",
      "graph LR; A-->B",
      "```",
      "",
      "둘째 문단",
      "",
      "</details>",
    ].join("\n");
    expect(attr(render(md), "data-blocks")).toBe("paragraph:2,list:1,code:1,figure:1");
  });

  it("토글 안의 토글", () => {
    const html = render(
      [
        "<details>",
        "<summary>바깥</summary>",
        "",
        "<details>",
        "<summary>안쪽</summary>",
        "",
        "깊은 본문",
        "",
        "</details>",
        "",
        "바깥 본문",
        "",
        "</details>",
      ].join("\n"),
    );
    expect(html).toMatch(
      /<details[^>]*data-summary="바깥"[^>]*><details[^>]*data-summary="안쪽"[^>]*><p>깊은 본문<\/p><\/details><p>바깥 본문<\/p><\/details>/,
    );
    expect(attr(html, "data-blocks")).toBe("paragraph:1,toggle:1");
  });

  it("빈 줄 없이 한 덩어리로 쓴 본문은 글로 보인다", () => {
    const html = render("<details>\n<summary>T</summary>\n한 줄 본문\n</details>");
    expect(html).toMatch(/<details[^>]*data-summary="T"[^>]*><p>한 줄 본문<\/p><\/details>/);
  });

  it("닫는 태그가 아직 없으면(AI 가 쓰는 중) 끝까지를 토글로 본다", () => {
    const html = render("<details>\n<summary>T</summary>\n\n첫 문단\n\n둘째 문단");
    expect(html).toMatch(/<details[^>]*><p>첫 문단<\/p><p>둘째 문단<\/p><\/details>$/);
  });

  it("인용구, 목록 안의 토글도 묶는다", () => {
    const quoted = render("> <details>\n> <summary>T</summary>\n>\n> 본문\n>\n> </details>");
    expect(quoted).toMatch(/<blockquote><details[^>]*><p>본문<\/p><\/details><\/blockquote>/);
    const listed = render("- <details>\n  <summary>T</summary>\n\n  본문\n\n  </details>");
    expect(attr(listed, "data-summary")).toBe("T");
  });

  it("제목 안의 태그는 걷어낸다 — 남기면 <b> 가 글자로 보인다", () => {
    const html = render("<details>\n<summary><b>굵은</b> 제목</summary>\n\n본문\n\n</details>");
    expect(attr(html, "data-summary")).toBe("굵은 제목");
  });

  it("제목이 없으면 속성도 없다(이름은 카드가 정한다)", () => {
    expect(render("<details>\n\n본문\n\n</details>")).not.toContain("data-summary");
  });

  it("문단 안의 인라인 태그, 다른 html 블록은 건드리지 않는다", () => {
    expect(render("글 <details> 글")).toContain("<p>글 &lt;details&gt; 글</p>");
    expect(render("<div>\n안\n</div>")).not.toContain("<details");
  });

  it("짝 없는 닫는 태그는 버린다", () => {
    expect(render("본문\n\n</details>")).toBe("<p>본문</p>");
  });

  it("줄 머리에서 열고 닫힌 토글에 소스 좌표를 심는다 — 드래그 → 부분 수정이 쓴다", () => {
    const md = "앞\n\n<details>\n<summary>T</summary>\n\n본문\n\n</details>\n\n뒤";
    const html = render(md);
    expect(md.slice(Number(attr(html, "data-md-start")), Number(attr(html, "data-md-end")))).toBe(
      "<details>\n<summary>T</summary>\n\n본문\n\n</details>",
    );
  });

  it("블록 중간에서 열린 토글엔 좌표를 심지 않는다 — 반쪽 좌표는 없는 것보다 위험하다", () => {
    const html = render(
      "<details>\n<summary>A</summary>\n\n가\n\n</details>\n<details>\n<summary>B</summary>\n\n나\n\n</details>",
    );
    const second = html.slice(html.lastIndexOf("<details"));
    expect(html.match(/<details/g)).toHaveLength(2);
    expect(second).toContain('data-summary="B"');
    expect(second).not.toContain("data-md-start");
  });
});

describe("detailsTokens", () => {
  it("토글 태그로 시작하지 않는 블록은 건드리지 않는다", () => {
    expect(detailsTokens("<div>안</div>")).toBeNull();
    // 이름이 details 로 시작하는 다른 태그
    expect(detailsTokens("<details-panel>안</details-panel>")).toBeNull();
  });
});

describe("parseBlocks", () => {
  it("순서를 지키고 모르는 종류, 깨진 조각은 건너뛴다", () => {
    expect(parseBlocks("paragraph:2,list:1")).toEqual([
      ["paragraph", 2],
      ["list", 1],
    ]);
    expect(parseBlocks("nope:3,code:x,table:1")).toEqual([["table", 1]]);
    expect(parseBlocks(undefined)).toEqual([]);
  });
});
