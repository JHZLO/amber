// 노트 본문의 "앵커" (질문을 단 문장) 를 렌더된 DOM 안의 Range 로 되찾는 순수 DOM 헬퍼.
// 마크다운은 매 렌더마다 새 DOM 이라 노드를 들고 있을 수 없다 — 대신 **텍스트 공간의 위치**로
// 기억했다가 그때그때 Range 를 다시 만든다. 질문 하이라이트(NoteComments)와 대기 중 형광펜
// (SelectionSweep)이 같은 자리를 가리켜야 해서 여기 한 벌만 둔다.

/** fullText 에서 anchor 의 occurrence 번째 출현 위치 (없으면 -1) */
export function nthIndex(fullText: string, anchor: string, occurrence: number): number {
  if (!anchor) return -1;
  let idx = -1;
  for (let i = 0; i <= occurrence; i++) {
    idx = fullText.indexOf(anchor, idx + 1);
    if (idx === -1) return -1;
  }
  return idx;
}

/** container 의 텍스트 공간에서 [idx, idx+len) 구간을 Range 로 복원 (없으면 null).
 *  위치를 따로 받는 이유: 목록 정렬에 그 위치(본문 등장 순서)를 그대로 쓴다. */
export function rangeAt(container: HTMLElement, idx: number, len: number): Range | null {
  const endIdx = idx + len;
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
  let acc = 0;
  let startNode: Text | null = null;
  let startOffset = 0;
  let endNode: Text | null = null;
  let endOffset = 0;
  let n: Node | null;
  while ((n = walker.nextNode())) {
    const t = n as Text;
    const len = t.data.length;
    if (!startNode && acc + len > idx) {
      startNode = t;
      startOffset = idx - acc;
    }
    if (startNode && acc + len >= endIdx) {
      endNode = t;
      endOffset = endIdx - acc;
      break;
    }
    acc += len;
  }
  if (!startNode || !endNode) return null;
  const r = document.createRange();
  r.setStart(startNode, startOffset);
  r.setEnd(endNode, endOffset);
  return r;
}

/** 화면 위의 사각형 하나 (getClientRects 결과를 담는 최소 모양) */
export interface Rect {
  top: number;
  left: number;
  width: number;
  height: number;
}

/** 조각난 사각형들을 **줄 단위**로 합친다.
 *
 *  `Range.getClientRects()` 는 줄마다 하나가 아니라 **인라인 상자마다** 하나를 준다 — `**굵게**` 나
 *  `` `코드` `` 가 낀 줄은 앞/굵은 조각/뒤로 쪼개진다. 그 조각마다 형광펜을 따로 그리면 경계에서
 *  띠가 뚝뚝 끊기고, 줄 단위로 주려던 지연까지 조각에 걸려 한 줄 안에서도 어긋난다.
 *  세로로 겹치는 것끼리 묶어 줄 하나당 상자 하나로 만든다. */
export function mergeLineRects(rects: Rect[]): Rect[] {
  const sorted = [...rects]
    .filter((r) => r.width > 0 && r.height > 0)
    .sort((a, b) => a.top - b.top || a.left - b.left);
  const lines: Rect[] = [];
  for (const r of sorted) {
    const last = lines[lines.length - 1];
    // 겹친 높이가 둘 중 낮은 쪽의 절반을 넘으면 같은 줄이다 — 위첨자나 다른 글자 크기가 섞여도
    // 같은 줄로 잡히고, 진짜 다음 줄과는 겹치지 않는다
    const overlap = last
      ? Math.min(last.top + last.height, r.top + r.height) - Math.max(last.top, r.top)
      : 0;
    if (last && overlap > Math.min(last.height, r.height) / 2) {
      const top = Math.min(last.top, r.top);
      const bottom = Math.max(last.top + last.height, r.top + r.height);
      const left = Math.min(last.left, r.left);
      const right = Math.max(last.left + last.width, r.left + r.width);
      lines[lines.length - 1] = { top, left, width: right - left, height: bottom - top };
    } else {
      lines.push({ top: r.top, left: r.left, width: r.width, height: r.height });
    }
  }
  return lines;
}
