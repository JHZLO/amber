// 본문 안 절 참조 — `[[1-2]]` 를 그 절로 뛰는 링크로 바꾼다. `[[1-2|앞 절]]` 이면 '앞 절'로 보인다.
//
// 왜 전용 문법인가: 본문의 모든 `1-2` 를 자동으로 링크하면 날짜(3-4월)·범위(2-3배)·버전까지
// 걸린다. 명시적 표기라야 글쓴이가 의도한 것만 링크된다.
//
// 왜 `[1-2](#...)` 표준 링크가 아닌가: 앵커 id 를 글쓴이가 알아야 한다. 제목 id 는 렌더 시점에
// 붙는 값이라(NotesView 가 note-h-N 으로 매긴다) 글에 적을 수 없다. 번호만 쓰면 되게 한다.

/** `[[번호]]` 또는 `[[번호|별칭]]` — 번호는 1, 1-2, 1-2-3 형태 */
const REF = /\[\[\s*(\d+(?:-\d+)*)\s*(?:\|\s*([^\]|]+?)\s*)?\]\]/g;

export type SecRef = { sec: string; label: string; start: number; end: number };

/** 한 문자열에서 절 참조를 모두 찾는다(순수 — 렌더러 없이 검증된다) */
export function findSecRefs(text: string): SecRef[] {
  const out: SecRef[] = [];
  for (const m of text.matchAll(REF)) {
    const sec = m[1];
    const label = (m[2] ?? "").trim() || sec;
    out.push({ sec, label, start: m.index, end: m.index + m[0].length });
  }
  return out;
}

/** 제목 문구 앞의 번호를 뽑는다 — "1-2. 커밋은 스칼라" → "1-2" (없으면 null) */
export function headingSection(text: string): string | null {
  return /^\s*(\d+(?:-\d+)*)\s*\./.exec(text)?.[1] ?? null;
}

// mdast 최소 형태 — 이 변환이 건드리는 것만
type MdNode = {
  type: string;
  value?: string;
  url?: string;
  children?: MdNode[];
  data?: { hProperties?: Record<string, string> };
};

/** remark 플러그인 — 텍스트 노드 안의 `[[1-2]]` 를 링크 노드로 갈아 끼운다.
 *  링크에 `data-secref` 를 남기고, 클릭 처리는 NotesView 가 위임으로 맡는다
 *  (그쪽이 제목 id·스크롤 컨테이너를 안다). */
export function remarkSecRefs() {
  return (tree: MdNode) => {
    walk(tree, (node) => {
      const kids = node.children;
      if (!kids) return;
      // 링크 안의 텍스트는 건드리지 않는다 — 링크 속 링크가 된다
      if (node.type === "link" || node.type === "linkReference") return;
      let i = 0;
      while (i < kids.length) {
        const child = kids[i];
        if (child.type !== "text" || typeof child.value !== "string") {
          i += 1;
          continue;
        }
        const hits = findSecRefs(child.value);
        if (!hits.length) {
          i += 1;
          continue;
        }
        const parts: MdNode[] = [];
        let at = 0;
        for (const h of hits) {
          if (h.start > at) {
            parts.push({ type: "text", value: child.value.slice(at, h.start) });
          }
          parts.push({
            type: "link",
            // href 는 앵커 형태로 남겨 둔다 — 다른 렌더러(문서 허브)에서도 링크로는 읽힌다
            url: `#${h.sec}`,
            children: [{ type: "text", value: h.label }],
            data: { hProperties: { "data-secref": h.sec, class: "sec-ref" } },
          });
          at = h.end;
        }
        if (at < child.value.length) {
          parts.push({ type: "text", value: child.value.slice(at) });
        }
        kids.splice(i, 1, ...parts);
        i += parts.length;
      }
    });
  };
}

function walk(node: MdNode, visit: (n: MdNode) => void): void {
  visit(node);
  for (const child of node.children ?? []) walk(child, visit);
}
