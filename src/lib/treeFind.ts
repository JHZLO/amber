// 파일 트리 안에서 찾기(⌘F) 의 순수 로직 — 본문 검색(lib/pageFind)과 다른 문제를 푼다.
// 본문 검색은 **그려진 글자**를 찾아 칠하면 되지만, 트리의 목표는 접힌 폴더 안에 있는 노트다.
// 화면에 없는 행은 Range 로 잡히지 않으므로 여기서는 DOM 이 아니라 **자료구조를 걸러** 내고,
// 맞은 것의 조상 폴더를 자동으로 펼친다 — 인텔리제이 프로젝트 창의 찾기와 같은 감각.

export interface FindableNode {
  path: string;
  name: string;
  isDir: boolean;
  children?: FindableNode[];
}

/** 맞은 노드 한 건 — 폴더면 Enter 가 펼치고, 파일이면 연다 */
export interface TreeMatch {
  path: string;
  isDir: boolean;
}

export interface TreeFindResult<T> {
  /** 맞은 것과 그 조상만 남긴 트리. 검색어가 비면 원본 그대로 */
  nodes: T[];
  /** 맞은 노드 — 트리에 보이는 순서(전위). ↑↓ 로 도는 차례가 이 순서다 */
  matches: TreeMatch[];
  /** 자동으로 펼칠 폴더 — 맞은 것의 조상. 맞은 폴더 자신은 넣지 않는다(열지 말고 보여만 준다) */
  expand: Set<string>;
}

/** 이름에 검색어가 들어 있나 (대소문자 무시) */
export function hits(name: string, query: string): boolean {
  const q = query.trim().toLowerCase();
  return q.length > 0 && name.toLowerCase().includes(q);
}

/** 트리를 검색어로 거른다. 폴더가 맞으면 그 아래는 통째로 남긴다 — 찾은 폴더를 열어 볼 수 있게. */
export function filterTree<T extends FindableNode>(nodes: T[], query: string): TreeFindResult<T> {
  const q = query.trim();
  if (!q) return { nodes, matches: [], expand: new Set() };
  const matches: TreeMatch[] = [];
  const expand = new Set<string>();

  function walk(list: T[], ancestors: string[]): T[] {
    const out: T[] = [];
    for (const n of list) {
      const self = hits(n.name, q);
      if (self) {
        matches.push({ path: n.path, isDir: n.isDir });
        for (const a of ancestors) expand.add(a);
      }
      if (!n.isDir) {
        if (self) out.push(n);
        continue;
      }
      const kids = (n.children ?? []) as T[];
      // 폴더 이름이 맞으면 아래를 거르지 않는다 — 그 안을 들여다보려고 찾은 것이다
      const keptKids = self ? kids : walk(kids, [...ancestors, n.path]);
      if (self) {
        // 맞은 폴더 자신이 차례에 든다. 그 안의 것까지 차례에 넣으면 접힌 행으로 이동하게 된다
        out.push(n);
      } else if (keptKids.length) {
        out.push({ ...n, children: keptKids });
      }
    }
    return out;
  }

  return { nodes: walk(nodes, []), matches, expand };
}

/** 라벨을 맞은 조각 기준으로 쪼갠다 — 화면에서 그 글자만 칠하려고 */
export function splitHit(name: string, query: string): { text: string; hit: boolean }[] {
  const q = query.trim().toLowerCase();
  if (!q) return [{ text: name, hit: false }];
  const hay = name.toLowerCase();
  const out: { text: string; hit: boolean }[] = [];
  let at = 0;
  let i = hay.indexOf(q);
  while (i !== -1) {
    if (i > at) out.push({ text: name.slice(at, i), hit: false });
    out.push({ text: name.slice(i, i + q.length), hit: true });
    at = i + q.length;
    i = hay.indexOf(q, at);
  }
  if (at < name.length) out.push({ text: name.slice(at), hit: false });
  return out;
}
