// 마크다운 토글 — `<details><summary>제목</summary> … </details>` 를 접고 펴는 블록으로 그린다.
// GitHub 과 같은 문법이라(mdAlerts 와 같은 이유) 같은 노트가 GitHub 에서도 같게 접힌다.
//
// 왜 직접 처리하나: react-markdown 은 raw HTML 을 글자 그대로 보인다(스크립트를 막기 위해 — mdSvg.ts).
// rehype-raw 로 HTML 을 통째로 열면 노트 안의 모든 태그가 살아나므로, 이 두 태그만 골라 mdast 노드로 바꾼다.
//
// CommonMark 은 <details>, <summary> 로 시작하는 html 블록을 **빈 줄에서** 끊는다. 그래서 흔한 모양
//
//   <details>
//   <summary>제목</summary>
//
//   본문(마크다운)
//
//   </details>
//
// 은 [html, 본문 블록들…, html] 형제로 온다. 형제 목록을 훑으며 여는 태그와 닫는 태그 사이를 한 노드로
// 묶는다. 빈 줄 없이 한 덩어리로 쓴 본문은 html 블록 안의 글자라 마크다운으로 읽지 않고 글로 둔다(GitHub 도 같다).

import { srcAttrs } from "./mdBlocks";

// mdast 최소 형태 — 이 변환이 건드리는 필드만 (unist 타입 의존 없이 테스트 가능하게)
type Point = { line?: number; column?: number; offset?: number };
type MdNode = {
  type: string;
  value?: string;
  lang?: string | null;
  children?: MdNode[];
  position?: { start?: Point; end?: Point };
  data?: { hName?: string; hProperties?: Record<string, unknown> };
};

type Token =
  | { kind: "open"; open: boolean; at: number }
  | { kind: "summary"; text: string }
  | { kind: "close"; end: number }
  | { kind: "text"; text: string };

/** 토글 태그로 시작하는 html 블록인가 — `<details-panel>` 같은 다른 태그는 아니다 */
const STARTS = /^\s*<\/?(?:details|summary)(?=[\s/>])/i;
const TOKEN =
  /<details(\s[^>]*)?>|<\/details\s*>|<summary(?:\s[^>]*)?>([\s\S]*?)<\/summary\s*>/gi;

/** html 블록을 토글 토큰으로 쪼갠다. 토글 태그로 시작하지 않으면 null(그 블록은 건드리지 않는다) */
export function detailsTokens(html: string): Token[] | null {
  if (!STARTS.test(html)) return null;
  const out: Token[] = [];
  const pushText = (s: string) => {
    const text = s.trim();
    if (text) out.push({ kind: "text", text });
  };
  let last = 0;
  for (const m of html.matchAll(TOKEN)) {
    const at = m.index;
    pushText(html.slice(last, at));
    if (m[0].startsWith("</")) out.push({ kind: "close", end: at + m[0].length });
    else if (/^<details/i.test(m[0])) out.push({ kind: "open", open: /(^|\s)open(\s|=|$)/i.test(m[1] ?? ""), at });
    // 제목 안의 <b> 같은 태그는 걷어낸다 — 남기면 글자 그대로 보인다
    else out.push({ kind: "summary", text: (m[2] ?? "").replace(/<[^>]*>/g, "").trim() });
    last = at + m[0].length;
  }
  pushText(html.slice(last));
  return out;
}

/** 토글을 담을 수 있는 블록 — 문단 안의 인라인 `<details>` 는 토글이 아니다 */
const FLOW = new Set(["root", "blockquote", "listItem"]);

/** remark 플러그인 — 여는 태그와 닫는 태그 사이의 형제 블록을 `details` 노드 하나로 묶는다.
 *  제목, 안에 든 것, 소스 좌표는 속성(data-summary / data-blocks / data-md-*)으로 넘기고,
 *  카드는 components/Markdown.tsx 의 MdToggle 이 그린다. */
export function remarkDetails() {
  return (tree: MdNode, file: { toString(): string }) => walk(tree, String(file));
}

function walk(node: MdNode, source: string): void {
  if (!node.children) return;
  if (FLOW.has(node.type)) node.children = regroup(node.children, source);
  for (const child of node.children) walk(child, source);
}

function regroup(nodes: MdNode[], source: string): MdNode[] {
  const out: MdNode[] = [];
  // 열린 토글들 — 닫는 태그를 만나기 전까지 형제가 맨 위 토글 안으로 들어간다.
  // 닫는 태그가 끝내 없으면 부모 끝에서 닫힌다(브라우저와 같다). AI 가 쓰는 도중의 노트도 그래서 접힌다.
  const stack: MdNode[] = [];
  const add = (n: MdNode) => (stack.at(-1)?.children ?? out).push(n);
  const extendTo = (end: Point | undefined) => {
    for (const d of stack) d.position = { start: d.position?.start, end };
  };

  for (const node of nodes) {
    const tokens =
      node.type === "html" && typeof node.value === "string" ? detailsTokens(node.value) : null;
    if (!tokens) {
      add(node);
      extendTo(node.position?.end);
      continue;
    }
    const value = node.value!;
    for (const tk of tokens) {
      if (tk.kind === "open") {
        const d: MdNode = {
          type: "details",
          children: [],
          data: { hName: "details", hProperties: tk.open ? { open: true } : {} },
          // 소스 좌표는 블록 머리에서 열렸을 때만 안다(드래그 → 부분 수정이 쓴다, mdBlocks.ts).
          // 블록 중간이면 줄과 칸을 셈해야 하는데 인용구 안에선 `> ` 가 빠진 값이라 어긋난다 — 반쪽 좌표는 두지 않는다.
          position: { start: value.slice(0, tk.at).trim() ? undefined : node.position?.start },
        };
        add(d);
        stack.push(d);
      } else if (tk.kind === "summary") {
        const top = stack.at(-1);
        const props = top?.data?.hProperties;
        // 제목은 토글의 첫 자식일 때만 — 본문 중간의 <summary> 는 글로 둔다
        if (top && props && top.children!.length === 0 && !("data-summary" in props)) {
          props["data-summary"] = tk.text;
        } else add(paragraph(tk.text));
      } else if (tk.kind === "close") {
        // 짝 없는 닫는 태그는 버린다(브라우저도 무시한다)
        if (stack.length === 0) continue;
        extendTo(value.slice(tk.end).trim() ? undefined : node.position?.end);
        stack.pop();
      } else add(paragraph(tk.text));
    }
  }
  for (const d of out.flatMap(collectDetails)) {
    // 좌표가 반쪽이면(시작이나 끝을 모르면) 통째로 뺀다 — mdBlocks.srcAttrs 와 같은 원칙
    if (!d.position?.start || !d.position?.end) delete d.position;
    const props = d.data!.hProperties!;
    Object.assign(props, srcAttrs(d, source));
    const blocks = countBlocks(d.children!);
    if (blocks) props["data-blocks"] = blocks;
  }
  return out;
}

/** 카드 머리에 흐리게 적는 '안에 든 것'의 종류 — 이 순서로 적는다 */
export const BLOCK_KINDS = ["paragraph", "list", "code", "figure", "table", "toggle"] as const;
export type BlockKind = (typeof BLOCK_KINDS)[number];

function blockKind(n: MdNode): BlockKind | null {
  if (n.type === "paragraph" || n.type === "list" || n.type === "table") return n.type;
  if (n.type === "details") return "toggle";
  // mermaid, svg 펜스는 코드가 아니라 그림으로 그려진다(Markdown.tsx)
  if (n.type === "code") return n.lang === "mermaid" || n.lang === "svg" ? "figure" : "code";
  return null;
}

/** "paragraph:2,list:1" — 셀 것이 없으면 빈 문자열 */
function countBlocks(children: MdNode[]): string {
  const n = new Map<BlockKind, number>();
  for (const c of children) {
    const k = blockKind(c);
    if (k) n.set(k, (n.get(k) ?? 0) + 1);
  }
  return BLOCK_KINDS.filter((k) => n.has(k))
    .map((k) => `${k}:${n.get(k)}`)
    .join(",");
}

/** data-blocks 를 되읽는다 — 모르는 종류, 깨진 조각은 건너뛴다 */
export function parseBlocks(s: string | undefined): [BlockKind, number][] {
  if (!s) return [];
  return s.split(",").flatMap((part) => {
    const [k, v] = part.split(":");
    const count = Number(v);
    return (BLOCK_KINDS as readonly string[]).includes(k) && count > 0
      ? [[k as BlockKind, count] as [BlockKind, number]]
      : [];
  });
}

function collectDetails(n: MdNode): MdNode[] {
  if (n.type !== "details") return [];
  return [n, ...(n.children ?? []).flatMap(collectDetails)];
}

function paragraph(text: string): MdNode {
  return { type: "paragraph", children: [{ type: "text", value: text }] };
}

// ---- 편집기 입력 — `<details>` 를 손으로 다 치지 않게 한다(파일에는 GitHub 문법이 그대로 남는다) ----

const SUMMARY_OPEN = "<details>\n<summary>";
/** 제목 뒤 뼈대. 본문 앞뒤 빈 줄이 있어야 본문을 마크다운으로 읽는다(위 머리 주석) */
const SUMMARY_CLOSE = "</summary>\n\n\n\n</details>";

/** 줄 맨 앞의 `>>` 뒤에서 스페이스를 치면 토글 뼈대로 바꾼다 — 그 줄에 이어 쓴 글은 제목이 된다.
 *  `> ` 는 인용구라 그대로 둔다. 바꿀 게 없으면 null, 있으면 [start, end) 를 text 로 갈아 끼우고
 *  커서를 caret(제목 앞)에 둔다. */
export function toggleShortcut(
  value: string,
  caret: number,
): { start: number; end: number; text: string; caret: number } | null {
  const start = value.lastIndexOf("\n", caret - 1) + 1;
  if (value.slice(start, caret) !== ">>") return null;
  const lineEnd = value.indexOf("\n", caret);
  const end = lineEnd === -1 ? value.length : lineEnd;
  const text = SUMMARY_OPEN + value.slice(caret, end) + SUMMARY_CLOSE;
  return { start, end, text, caret: start + SUMMARY_OPEN.length };
}

/** 토글 제목 줄에서 Enter — 제목에 줄바꿈을 넣는 대신 본문 자리로 커서를 옮긴다.
 *  뼈대 모양(제목 줄 뒤에 빈 줄 둘)일 때만, 그 둘째 빈 줄 머리를 돌려준다. 아니면 null(평소 Enter) */
export function toggleTitleEnter(value: string, caret: number): number | null {
  const start = value.lastIndexOf("\n", caret - 1) + 1;
  const lineEnd = value.indexOf("\n", caret);
  if (lineEnd === -1 || !/^<summary>.*<\/summary>$/.test(value.slice(start, lineEnd))) return null;
  return value.startsWith("\n\n\n", lineEnd) ? lineEnd + 2 : null;
}

// ---- 화면 쪽 — 펼침 상태는 카드(MdToggle)의 React state 라 밖에서는 이벤트로 부탁한다 ----

const REVEAL = "md-toggle-reveal";

/** el 을 감싼 접힌 토글을 전부 편다 — 접힌 토글 안으로는 스크롤해도 보이지 않는다(목차, 찾기, 코멘트 이동).
 *  펼치는 동안에도 안쪽 요소의 위치는 그대로라(트랙이 아래로 자랄 뿐) 바로 스크롤해도 맞는 자리로 간다. */
export function openEnclosingToggles(el: Element | null | undefined): void {
  for (let t = el?.closest(".md-toggle"); t; t = t.parentElement?.closest(".md-toggle")) {
    t.dispatchEvent(new Event(REVEAL));
  }
}

/** 카드가 펼침 부탁을 받는다. 정리 함수를 돌려준다(useEffect 용) */
export function onToggleReveal(el: Element | null, open: () => void): (() => void) | undefined {
  if (!el) return;
  el.addEventListener(REVEAL, open);
  return () => el.removeEventListener(REVEAL, open);
}

/** 접힌 토글 안에 있나 — 그 안의 제목은 화면에 없다(스크롤 스파이가 건너뛴다) */
export function inClosedToggle(el: Element): boolean {
  return !!el.closest('.md-toggle[data-open="false"]');
}

/** Range 의 한쪽 끝이 접힌 토글의 몸통 안인가 — 그런 구간은 칠하지 않는다.
 *  WebKit 은 visibility:hidden 안의 Custom Highlight 를 숨기지 않고 **그다음 보이는 글자**에 칠한다
 *  (접힌 토글 안의 질문 형광펜이 아래 토글 제목에 붙었다). 제목 줄은 접혀도 보이므로 몸통만 본다. */
export function inCollapsedBody(r: Range): boolean {
  return [r.startContainer, r.endContainer].some(
    (n) =>
      !!(n instanceof Element ? n : n.parentElement)?.closest(
        '.md-toggle[data-open="false"] > .md-toggle-body',
      ),
  );
}

/** container 안의 토글이 접히거나 펴질 때마다 부른다. 정리 함수를 돌려준다(useEffect 용) */
export function onToggleChange(container: Element | null, cb: () => void): (() => void) | undefined {
  if (!container) return;
  const mo = new MutationObserver(cb);
  mo.observe(container, { subtree: true, attributes: true, attributeFilter: ["data-open"] });
  return () => mo.disconnect();
}
