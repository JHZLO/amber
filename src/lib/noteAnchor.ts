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
