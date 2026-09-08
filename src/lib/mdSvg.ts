// 마크다운 안의 SVG 그래픽 — ```svg 펜스(그리고 노트에 이미 박힌 raw <svg> 블록)를 인라인 SVG 로 그린다.
//
// 왜 필요한가: 노트 AI 가 막대·선 그래프처럼 mermaid 로 못 그리는 것을 SVG 로 낸다. react-markdown 은
// raw HTML 을 글자 그대로 보이므로(스크립트를 막기 위해) 그대로 두면 `<svg …>` 소스가 노트에 통째로 박힌다.
// 컨벤션은 ```svg 펜스 하나다(context/note-compose.md) — 코드블록이라 마크다운 구조가 깨지지 않고,
// 렌더 쪽이 어디를 그림으로 바꿀지 분명하다. 펜스 없이 박힌 raw <svg> 는 remarkSvgHtml 이 같은 코드 노드로
// 바꿔 살린다(mdMermaid 가 태그 빠진 다이어그램을 살리는 것과 같은 이유 — 이미 쓰인 노트도 살아나야 한다).
//
// <img> 가 아니라 인라인으로 그리는 이유: 그래프의 글자·축은 fill="currentColor" 로 그려야 다크/라이트를
// 따라가는데, <img> 안의 SVG 는 문서의 color 를 상속받지 못한다. 인라인은 스크립트·이벤트·외부 자원이
// 위험하니 여기서 걷어낸다 — 허용 목록이 아니라 위험 목록 방식이다(그래프에 쓰이는 요소·속성이 너무 많다).

const BLOCKED_TAGS = new Set([
  "script",
  "foreignobject",
  "iframe",
  "object",
  "embed",
  "audio",
  "video",
  "html",
  "head",
  "body",
  "meta",
  "link",
  "base",
]);

/** raw HTML 블록이 통째로 하나의 <svg>…</svg> 인가 */
export function looksLikeSvg(html: string): boolean {
  const t = html.trim();
  return /^<svg[\s>]/i.test(t) && /<\/svg>\s*$/i.test(t);
}

/** href 로 허용하는 것: 문서 안 참조(#), http(s) 링크, data 이미지. javascript: · file: · 외부 use 는 거른다 */
export function isSafeHref(value: string): boolean {
  const v = value.trim().toLowerCase();
  return (
    v.startsWith("#") ||
    v.startsWith("http://") ||
    v.startsWith("https://") ||
    v.startsWith("data:image/")
  );
}

/** style 속성/요소에 외부를 부를 수 있는 것이 없는가 (url(), @import, expression()) */
export function styleIsSafe(css: string): boolean {
  return !/url\s*\(|@import|expression\s*\(/i.test(css);
}

/** 속성 하나를 지워야 하는가 — on* 핸들러, 위험한 href, 외부를 부르는 style */
export function isDangerousAttr(name: string, value: string): boolean {
  const n = name.toLowerCase();
  if (n.startsWith("on")) return true;
  if (n === "href" || n === "xlink:href") return !isSafeHref(value);
  if (n === "style") return !styleIsSafe(value);
  return false;
}

/** 파싱된 SVG 트리를 제자리에서 소독한다 — 위험 요소는 통째로, 위험 속성은 속성만 */
export function sanitizeSvgTree(root: Element): void {
  const stack: Element[] = [root];
  while (stack.length) {
    const el = stack.pop()!;
    for (const child of Array.from(el.children)) {
      const tag = child.tagName.toLowerCase();
      if (BLOCKED_TAGS.has(tag)) {
        child.remove();
        continue;
      }
      if (tag === "style" && !styleIsSafe(child.textContent ?? "")) {
        child.remove();
        continue;
      }
      stack.push(child);
    }
    for (const attr of Array.from(el.attributes)) {
      if (isDangerousAttr(attr.name, attr.value)) el.removeAttribute(attr.name);
    }
  }
}

/** ```svg 본문 → 그릴 수 있는 SVG 문자열. <svg> 를 못 찾으면 null — 호출부가 코드블록으로 보여 준다(잃지 않게).
 *  HTML 파서로 읽는다: XML 파서는 `&nbsp;` 하나에도 통째로 실패하는데, 모델이 낸 SVG 는 그 정도 느슨함이 흔하다. */
export function sanitizeSvg(source: string): string | null {
  if (typeof DOMParser === "undefined") return null;
  const doc = new DOMParser().parseFromString(`<body>${source.trim()}</body>`, "text/html");
  const root = doc.body.querySelector("svg");
  if (!root) return null;
  sanitizeSvgTree(root);
  // viewBox 만 있고 폭이 없으면 100% — 컨테이너에 맞춰 줄어들게. 명시한 폭은 존중한다
  if (!root.getAttribute("width")) root.setAttribute("width", "100%");
  if (!root.getAttribute("role")) root.setAttribute("role", "img");
  return root.outerHTML;
}

// mdast 최소 형태 — 이 변환이 건드리는 필드만 (unist 타입 의존 없이 테스트 가능하게)
type MdNode = {
  type: string;
  value?: string;
  lang?: string | null;
  children?: MdNode[];
};

/** remark 플러그인 — 펜스 없이 박힌 raw <svg>…</svg> HTML 블록을 ```svg 코드 노드로 바꾼다 */
export function remarkSvgHtml() {
  return (tree: MdNode) => {
    walk(tree, (node) => {
      if (node.type !== "html" || typeof node.value !== "string") return;
      if (!looksLikeSvg(node.value)) return;
      node.type = "code";
      node.lang = "svg";
      node.value = node.value.trim();
    });
  };
}

function walk(node: MdNode, fn: (n: MdNode) => void): void {
  fn(node);
  node.children?.forEach((c) => walk(c, fn));
}
