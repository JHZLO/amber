// 할 일 트리 계산 — 순수 함수만. DOM·DB·React 를 모르므로 평범한 데이터로 그대로 호출·검증된다.
// 드래그(TodoView.startDrag)는 실측(rect)과 커밋(DB)만 맡고, 트리 판단은 전부 여기로 모은다.

/** 트리 계산에 필요한 최소 필드 — Todo 를 그대로 넘길 수 있다(구조적 타이핑) */
export type TodoNode = { id: number; parent_id: number | null };

/** 렌더 순서대로 펼친 한 행 */
export type TreeRow = { id: number; parent_id: number | null; depth: number };

/** 드래그 중 커서가 가리키는 슬롯 — idx=후보 행 사이 삽입 위치, depth=들여쓰기 단계 */
export type DropSlot = { idx: number; depth: number };

/** 드롭 해석 결과. newSortOrder 는 orderedSiblingIds 안에서의 위치와 같다(reorderTodos 가 index 를 그대로 쓴다) */
export type DropResolution = {
  newParentId: number | null;
  newSortOrder: number;
  orderedSiblingIds: number[];
};

/** 형제 그룹 — 입력 배열 순서(sort_order, id)를 그대로 유지한다 */
export function childrenOf<T extends TodoNode>(
  nodes: readonly T[],
  parentId: number | null,
): T[] {
  return nodes.filter((n) => (n.parent_id ?? null) === parentId);
}

/** 렌더와 동일한 순서의 플랫 행 목록 (depth 포함) */
export function flattenTree(nodes: readonly TodoNode[]): TreeRow[] {
  const rows: TreeRow[] = [];
  const walk = (parent: number | null, depth: number) => {
    for (const n of childrenOf(nodes, parent)) {
      rows.push({ id: n.id, parent_id: n.parent_id ?? null, depth });
      walk(n.id, depth + 1);
    }
  };
  walk(null, 0);
  return rows;
}

/** 부분 집합용 펼치기 — 부모가 목록에 없으면 그 행을 루트로 올린다.
 *  flattenTree 는 parent=null 에서만 내려가 '부모가 빠진 자식'을 통째로 누락시키는데,
 *  밀린 목록은 `done=0 AND due_date<오늘` 로 걸러낸 부분 집합이라(부모가 완료됐거나 마감이
 *  오늘 이후면 빠진다) 그대로 쓰면 자식이 화면에서 사라진다. 여기선 입력의 모든 행이 정확히
 *  한 번 나오는 것을 보장한다 — 순환이 있어도 마찬가지. */
export function flattenSubset(nodes: readonly TodoNode[]): TreeRow[] {
  const present = new Set(nodes.map((n) => n.id));
  const rows: TreeRow[] = [];
  const seen = new Set<number>();
  const walk = (node: TodoNode, depth: number) => {
    if (seen.has(node.id)) return; // 순환 방어
    seen.add(node.id);
    rows.push({ id: node.id, parent_id: node.parent_id ?? null, depth });
    for (const c of childrenOf(nodes, node.id)) walk(c, depth + 1);
  };
  for (const n of nodes) {
    const pid = n.parent_id ?? null;
    if (pid === null || !present.has(pid)) walk(n, 0);
  }
  // 순환 뭉치는 위에서 루트를 못 찾는다 — 남은 행을 최상위로 끌어올려 누락을 막는다
  for (const n of nodes) walk(n, 0);
  return rows;
}

/** rootId 와 그 모든 자손. 드래그 후보 제외(자기 안으로 못 들어감 = 순환 방지)와
 *  삭제 서브트리 크기 계산이 같은 판정을 쓰도록 공용. 입력 순서에 의존하지 않는다. */
export function subtreeIds(
  nodes: readonly TodoNode[],
  rootId: number,
): Set<number> {
  const ids = new Set<number>([rootId]);
  const stack = [rootId];
  while (stack.length) {
    const pid = stack.pop()!;
    for (const n of nodes) {
      if ((n.parent_id ?? null) !== pid || ids.has(n.id)) continue;
      ids.add(n.id);
      stack.push(n.id);
    }
  }
  return ids;
}

/** 자손 수(자기 자신 제외) — 삭제가 실제로 지우는 행 수 */
export function descendantCount(
  nodes: readonly TodoNode[],
  rootId: number,
): number {
  return subtreeIds(nodes, rootId).size - 1;
}

/** 유효 깊이: 아래 행의 깊이 이상(그 행의 부모를 뺏지 않게), 위 행의 깊이+1 이하(그 행의 하위까지) */
export function clampDropDepth(
  desired: number,
  above: TreeRow | null,
  below: TreeRow | null,
): number {
  const maxD = above ? above.depth + 1 : 0;
  const minD = below ? below.depth : 0;
  return Math.max(minD, Math.min(maxD, desired));
}

/** 슬롯 → 새 부모 + 새 형제 순서. candidates 는 서브트리를 뺀 세로 순 후보 행.
 *  제자리(변화 없음)거나 규칙상 불가능한 슬롯이면 null → 호출부는 커밋을 생략한다. */
export function resolveDrop(
  nodes: readonly TodoNode[],
  candidates: readonly TreeRow[],
  draggedId: number,
  slot: DropSlot,
): DropResolution | null {
  const dragged = nodes.find((n) => n.id === draggedId);
  if (!dragged) return null;
  const { idx, depth } = slot;

  // 새 부모 = 슬롯 위쪽에서 가장 가까운 depth-1 행 (depth 0 이면 최상위)
  let newParentId: number | null = null;
  if (depth > 0) {
    for (let i = idx - 1; i >= 0; i--) {
      if (candidates[i].depth === depth - 1) {
        newParentId = candidates[i].id;
        break;
      }
      if (candidates[i].depth < depth - 1) break;
    }
    if (newParentId == null) return null; // 방어 — 유효 깊이 규칙상 오지 않는 경로
  }

  // 새 형제 순서: 슬롯 이전에 등장한 같은 부모의 행 수 = 삽입 위치
  const newSortOrder = candidates
    .slice(0, idx)
    .filter((r) => r.parent_id === newParentId).length;
  const orderedSiblingIds = childrenOf(nodes, newParentId)
    .map((n) => n.id)
    .filter((sid) => sid !== draggedId);
  orderedSiblingIds.splice(newSortOrder, 0, draggedId);

  const oldParentId = dragged.parent_id ?? null;
  if (newParentId === oldParentId) {
    const oldOrder = childrenOf(nodes, oldParentId).map((n) => n.id);
    if (oldOrder.join(",") === orderedSiblingIds.join(",")) return null; // 제자리
  }
  return { newParentId, newSortOrder, orderedSiblingIds };
}
