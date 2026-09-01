// 렌더된 마크다운에서 드래그한 자리를 **마크다운 소스 구간**으로 되돌린다.
//
// 왜 이런 우회가 필요한가: 읽기 모드에서 드래그하면 좌표가 렌더된 텍스트 공간이다. `**볼드**` 는
// `볼드` 로, `[제목](url)` 은 `제목` 으로 줄어들어 소스와 오프셋이 어긋난다. 그래서 선택 문자열을
// 소스에서 찾는 방식은 인라인 문법을 하나만 물어도 실패하는데, **조용히** 실패해 엉뚱한 곳을 자른다.
//
// 대신 파서가 이미 알고 있는 좌표를 쓴다: mdast 노드의 position 은 mdast-util-to-hast 가 hast 로
// 복사하고, react-markdown 은 그 노드를 컴포넌트 오버라이드에 넘긴다(passNode). Markdown.tsx 가
// 블록 요소마다 그 값을 data-md-start / data-md-end 로 심어 두고, 여기서는 그걸 읽기만 한다.
//
// 해상도는 **블록 단위**다. 문장 절반을 드래그해도 그 문단 전체가 대상이 된다 — AI 가 고쳐 쓸
// 최소 단위가 문단이라 오히려 이 편이 맞고, 문단 하나는 몇백 자라 비용도 그대로 작다.

export interface Span {
  start: number;
  end: number;
}

/** 블록 요소에 심을 소스 좌표 속성. mdast 의 position 이 hast 로 복사돼 오므로 계산하지 않는다.
 *
 *  **줄 경계에서 시작·끝나는 블록만 심는다.** 인용구 안의 문단은 `> ` 뒤에서, 느슨한 목록 안의
 *  문단은 `- ` 뒤에서 시작한다 — 그 구간만 갈아끼우면 줄머리 접두사가 남아 `> ` 하나만 덜렁 남은
 *  깨진 마크다운이 된다. 그런 블록은 심지 않아 바깥 블록(blockquote·li)이 대상이 되게 한다.
 *  좌표가 온전하지 않을 때도 아무것도 심지 않는다 — 반쪽 좌표는 없는 것보다 위험하다. */
export function srcAttrs(node: unknown, source: string): Record<string, number> {
  const pos = (
    node as { position?: { start?: { offset?: number }; end?: { offset?: number } } }
  )?.position;
  const start = pos?.start?.offset;
  const end = pos?.end?.offset;
  if (typeof start !== "number" || typeof end !== "number" || end <= start) return {};
  const atLineStart = start === 0 || source[start - 1] === "\n";
  const atLineEnd = end >= source.length || source[end] === "\n";
  if (!atLineStart || !atLineEnd) return {};
  return { "data-md-start": start, "data-md-end": end };
}

/** data-md-start / data-md-end 를 읽는다. 둘 다 온전한 수가 아니면 null */
export function readSpan(
  el: { getAttribute(name: string): string | null } | null | undefined,
): Span | null {
  if (!el) return null;
  const start = Number(el.getAttribute("data-md-start"));
  const end = Number(el.getAttribute("data-md-end"));
  if (!Number.isFinite(start) || !Number.isFinite(end)) return null;
  if (el.getAttribute("data-md-start") === null) return null;
  if (end <= start) return null;
  return { start, end };
}

/** 두 끝점이 다른 블록에 걸쳐 있으면 둘을 아우르는 구간. 한쪽만 있으면 그쪽 */
export function unionSpan(a: Span | null, b: Span | null): Span | null {
  if (!a) return b;
  if (!b) return a;
  return { start: Math.min(a.start, b.start), end: Math.max(a.end, b.end) };
}

/** 노드에서 위로 올라가 소스 좌표를 들고 있는 가장 가까운 블록 요소 */
function blockOf(node: Node | null, container: HTMLElement): Element | null {
  const el = node instanceof Element ? node : node?.parentElement;
  if (!el || !container.contains(el)) return null;
  const hit = el.closest("[data-md-start]");
  return hit && container.contains(hit) ? hit : null;
}

/** 선택 Range → 그 선택이 걸친 블록들의 마크다운 소스 구간 (못 찾으면 null) */
export function blockRangeFromSelection(
  container: HTMLElement,
  range: Range,
): Span | null {
  return unionSpan(
    readSpan(blockOf(range.startContainer, container)),
    readSpan(blockOf(range.endContainer, container)),
  );
}
