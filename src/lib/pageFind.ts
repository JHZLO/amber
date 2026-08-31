// 페이지 내 검색(⌘F) 의 순수 로직 — 컨테이너 안 텍스트에서 검색어 위치를 Range 로 찾는다.
// DOM 을 고치지 않는다: 하이라이트는 CSS Custom Highlight API 가 Range 만 받아 칠하므로
// (NoteComments 의 질문 하이라이트와 같은 방식) 마크다운 렌더 결과를 건드릴 필요가 없다.
// 본문을 감싸면 React 가 다시 그릴 때 검색 흔적이 남거나 스크롤이 튄다.

/** 컨테이너 안에서 검색어가 나오는 모든 자리를 Range 로. 대소문자는 무시한다. */
export function findRanges(root: HTMLElement, query: string): Range[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const out: Range[] = [];
  let node: Node | null;
  while ((node = walker.nextNode())) {
    const text = node as Text;
    // 화면에 없는 글자는 찾아도 갈 곳이 없다(접힌 섹션·display:none)
    if (!text.parentElement?.offsetParent && text.parentElement?.tagName !== "BODY") {
      if (!isRendered(text.parentElement)) continue;
    }
    const hay = text.data.toLowerCase();
    let i = hay.indexOf(q);
    while (i !== -1) {
      const r = document.createRange();
      r.setStart(text, i);
      r.setEnd(text, i + q.length);
      out.push(r);
      i = hay.indexOf(q, i + q.length);
    }
  }
  return out;
}

/** offsetParent 는 position:fixed 요소에서 null 이라 그것만으로 숨김 판정을 하면 오탐이다 */
function isRendered(el: Element | null): boolean {
  if (!el) return false;
  const rect = (el as HTMLElement).getBoundingClientRect?.();
  return !!rect && (rect.width > 0 || rect.height > 0);
}

/** 브라우저가 Custom Highlight API 를 지원하는지 (미지원이면 검색은 이동만 한다) */
export function highlightRegistry(): {
  set: (k: string, h: unknown) => void;
  delete: (k: string) => void;
} | null {
  const css = CSS as unknown as {
    highlights?: { set: (k: string, h: unknown) => void; delete: (k: string) => void };
  };
  const HL = (window as unknown as { Highlight?: unknown }).Highlight;
  return css.highlights && HL ? css.highlights : null;
}

/** Range 목록을 하이라이트로 등록한다. 지원 안 하면 조용히 넘어간다. */
export function paint(key: string, ranges: Range[]): void {
  const reg = highlightRegistry();
  if (!reg) return;
  if (!ranges.length) {
    reg.delete(key);
    return;
  }
  const HL = (window as unknown as { Highlight: new (...r: Range[]) => unknown }).Highlight;
  reg.set(key, new HL(...ranges));
}

/** 하이라이트 지우기 */
export function clearPaint(...keys: string[]): void {
  const reg = highlightRegistry();
  if (!reg) return;
  for (const k of keys) {
    try {
      reg.delete(k);
    } catch {
      /* noop */
    }
  }
}
