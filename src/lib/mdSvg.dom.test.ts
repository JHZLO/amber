// @vitest-environment jsdom
//
// 살균기가 **실제로 도는지** 확인하는 유일한 자리. 전역 환경은 node 로 둔다(vite.config 의 판단)
// — 그래서 mdSvg.test.ts 는 술어 함수(isDangerousAttr 등)만 볼 수 있었고, 그 술어들이 실제로
// 호출되는지는 아무도 확인하지 않았다. 더 나쁜 건 sanitizeSvg 가 `typeof DOMParser === "undefined"`
// 에서 곧바로 null 을 돌려주는 탓에, node 환경에서 쓴 살균 테스트는 **null 덕분에 통과**한다는 점이다.
//
// 이 입력은 AI 가 만든 SVG 이고 싱크는 Markdown.tsx 의 dangerouslySetInnerHTML 이다.
// 그 웹뷰는 Tauri IPC(move_to_trash 포함)를 들고 있으므로, 살균 누락은 화면 깨짐이 아니라
// 파일 삭제로 이어질 수 있는 경로다.

import { describe, expect, it } from "vitest";
import { sanitizeSvg } from "./mdSvg";

/** 환경이 잘못 잡히면 sanitizeSvg 가 전부 null 을 돌려주고 아래 테스트가 **조용히 통과**한다.
 *  그래서 먼저 "살균기가 실제로 돌고 있다"를 확인한다. */
describe("환경", () => {
  it("jsdom 이라 살균기가 실제로 실행된다", () => {
    expect(typeof DOMParser).not.toBe("undefined");
    expect(sanitizeSvg('<svg viewBox="0 0 10 10"></svg>')).not.toBeNull();
  });
});

describe("sanitizeSvg — 위험 요소 제거", () => {
  it("<script> 를 걷어낸다", () => {
    const out = sanitizeSvg(
      '<svg viewBox="0 0 10 10"><script>alert(1)</script><rect width="5" height="5"/></svg>',
    );
    expect(out).not.toBeNull();
    expect(out!.toLowerCase()).not.toContain("<script");
    expect(out!).toContain("<rect"); // 그림은 남아야 한다
  });

  it("<foreignObject> 를 걷어낸다 (임의 HTML 주입 통로)", () => {
    const out = sanitizeSvg(
      '<svg viewBox="0 0 10 10"><foreignObject><div>hi</div></foreignObject></svg>',
    );
    expect(out!.toLowerCase()).not.toContain("foreignobject");
  });

  it("중첩된 <script> 도 걷어낸다", () => {
    const out = sanitizeSvg(
      '<svg viewBox="0 0 10 10"><g><g><script>alert(1)</script></g></g></svg>',
    );
    expect(out!.toLowerCase()).not.toContain("<script");
  });
});

describe("sanitizeSvg — 위험 속성 제거", () => {
  it("이벤트 핸들러 속성을 지운다", () => {
    const out = sanitizeSvg(
      '<svg viewBox="0 0 10 10" onload="alert(1)"><rect onclick="alert(2)" width="5" height="5"/></svg>',
    );
    expect(out!.toLowerCase()).not.toContain("onload");
    expect(out!.toLowerCase()).not.toContain("onclick");
  });

  it("javascript: href 를 지운다", () => {
    const out = sanitizeSvg(
      '<svg viewBox="0 0 10 10"><a href="javascript:alert(1)"><rect width="5" height="5"/></a></svg>',
    );
    expect(out!.toLowerCase()).not.toContain("javascript:");
  });

  it("로컬 파일을 가리키는 href 를 지운다", () => {
    const out = sanitizeSvg(
      '<svg viewBox="0 0 10 10"><use href="file:///etc/passwd"/></svg>',
    );
    expect(out!.toLowerCase()).not.toContain("file://");
  });

  it("style 에 든 url() 을 막는다", () => {
    const out = sanitizeSvg(
      '<svg viewBox="0 0 10 10"><rect style="fill:url(javascript:alert(1))" width="5" height="5"/></svg>',
    );
    expect(out!.toLowerCase()).not.toContain("javascript:");
  });
});

describe("sanitizeSvg — 정상 그림은 보존한다", () => {
  it("도형·좌표·색을 그대로 둔다", () => {
    const out = sanitizeSvg(
      '<svg viewBox="0 0 100 50"><rect x="1" y="2" width="10" height="20" fill="#18181b"/><text x="0" y="40">안녕</text></svg>',
    );
    expect(out!).toContain('viewBox="0 0 100 50"');
    expect(out!).toContain('fill="#18181b"');
    expect(out!).toContain("안녕");
  });

  it("폭이 없으면 100% 를 넣고 role=img 를 붙인다", () => {
    const out = sanitizeSvg('<svg viewBox="0 0 10 10"></svg>');
    expect(out!).toContain('width="100%"');
    expect(out!).toContain('role="img"');
  });

  it("명시한 폭은 존중한다", () => {
    const out = sanitizeSvg('<svg viewBox="0 0 10 10" width="320"></svg>');
    expect(out!).toContain('width="320"');
    expect(out!).not.toContain('width="100%"');
  });

  it("<svg> 가 없으면 null (호출부가 코드블록으로 보여 준다)", () => {
    expect(sanitizeSvg("그냥 텍스트")).toBeNull();
  });
});

describe("sanitizeSvg — SMIL 애니메이션 (속성을 런타임에 갈아끼우는 통로)", () => {
  // <set>/<animate> 는 attributeName 으로 **아무 속성이나** 나중에 설정할 수 있다.
  // 속성 이름이 on* 이 아니라 attributeName/to 라서 isDangerousAttr 의 on* 규칙에 걸리지 않고,
  // 요소 자체도 차단 목록에 없었다. 노트 그래프에 애니메이션이 필요한 적이 없으므로 통째로 막는다.
  it("<set> 으로 이벤트 핸들러를 심는 걸 막는다", () => {
    const out = sanitizeSvg(
      '<svg viewBox="0 0 10 10"><rect width="5" height="5"><set attributeName="onload" to="alert(1)"/></rect></svg>',
    );
    expect(out!.toLowerCase()).not.toContain("<set");
    expect(out!.toLowerCase()).not.toContain("alert(1)");
  });

  it("<animate> 로 href 를 갈아끼우는 걸 막는다", () => {
    const out = sanitizeSvg(
      '<svg viewBox="0 0 10 10"><a href="#x"><animate attributeName="href" to="javascript:alert(1)"/></a></svg>',
    );
    expect(out!.toLowerCase()).not.toContain("<animate");
    expect(out!.toLowerCase()).not.toContain("javascript:");
  });
});
