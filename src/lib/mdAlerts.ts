// 마크다운 알림 블록(GitHub alert 문법) — `> [!NOTE]` 로 시작하는 인용구를 콜아웃으로 바꾼다.
// remark-gfm 에는 이 문법이 없어서(GFM 스펙 밖) 직접 처리한다. 순수 mdast 변환만 담고,
// 렌더(아이콘·라벨·CSS)는 components/Markdown.tsx 가 맡는다.
//
// 왜 필요한가: 노트 AI 가 이 문법으로 '참고/주의'를 표시하는데(context/note-compose.md),
// 지원이 없으면 `[!NOTE]` 가 본문에 글자 그대로 박혀 보인다. Paper(사내 문서 허브)도 같은
// 문법을 렌더하므로, 같은 노트가 두 곳에서 같게 보인다.

/** 지원하는 알림 종류 — Paper 쪽 렌더러(front-web-paper)와 같은 집합으로 맞춘다 */
export const ALERT_KINDS = [
  "NOTE",
  "TIP",
  "IMPORTANT",
  "WARNING",
  "CAUTION",
] as const;

export type AlertKind = (typeof ALERT_KINDS)[number];

const MARKER = new RegExp(`^\\[!(${ALERT_KINDS.join("|")})\\][ \\t]*(?:\\r?\\n|$)`);

/** 첫 줄이 알림 마커면 그 종류와 마커 길이를 돌려준다.
 *  마커는 **자기 줄에 혼자** 있어야 한다 — `> [!NOTE] 본문` 처럼 같은 줄에 이어 쓰면
 *  일반 인용구로 남긴다(Paper 렌더러와 같은 판정). */
export function matchAlertMarker(
  text: string,
): { kind: AlertKind; length: number } | null {
  const m = MARKER.exec(text);
  if (!m) return null;
  return { kind: m[1] as AlertKind, length: m[0].length };
}

// mdast 최소 형태 — 이 변환이 건드리는 필드만 (unist 타입 의존 없이 테스트 가능하게)
type MdNode = {
  type: string;
  value?: string;
  children?: MdNode[];
  data?: { hProperties?: Record<string, string> };
};

/** remark 플러그인 — 알림 인용구에서 마커를 떼고 `data-alert` 속성을 남긴다.
 *  마커를 떼는 이유: 종류는 속성으로 옮겨 라벨·아이콘으로 다시 그리므로, 본문에 두면 중복된다. */
export function remarkAlerts() {
  return (tree: MdNode) => {
    walk(tree, (node) => {
      if (node.type !== "blockquote") return;
      const first = node.children?.[0];
      if (first?.type !== "paragraph") return;
      const lead = first.children?.[0];
      if (lead?.type !== "text" || typeof lead.value !== "string") return;

      const hit = matchAlertMarker(lead.value);
      if (!hit) return;

      lead.value = lead.value.slice(hit.length);
      // 마커만 있던 문단·텍스트 노드는 비게 되므로 걷어낸다(빈 <p> 가 남지 않게)
      if (!lead.value) first.children!.shift();
      if (first.children!.length === 0) node.children!.shift();

      node.data = {
        ...node.data,
        hProperties: { ...node.data?.hProperties, "data-alert": hit.kind },
      };
    });
  };
}

function walk(node: MdNode, visit: (n: MdNode) => void): void {
  visit(node);
  for (const child of node.children ?? []) walk(child, visit);
}
