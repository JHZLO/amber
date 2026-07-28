// 드래그 트리 계산 회귀 테스트. 손으로 드래그해봐야만 확인되던 판정(자기 안으로 드롭·깊이 경계·
// 형제 재정렬·레벨 이동)을 데이터로 고정한다.

import { describe, expect, it } from "vitest";
import {
  childrenOf,
  clampDropDepth,
  descendantCount,
  flattenSubset,
  flattenTree,
  resolveDrop,
  subtreeIds,
  type DropSlot,
  type TodoNode,
  type TreeRow,
} from "./todoTree";

// 1 ├ 2 ├ 4      배열 순서 = 화면 순서(sort_order, id) — childrenOf 가 그대로 쓴다
//   └ 3
// 5 └ 6
// 7
const TREE: TodoNode[] = [
  { id: 1, parent_id: null },
  { id: 2, parent_id: 1 },
  { id: 4, parent_id: 2 },
  { id: 3, parent_id: 1 },
  { id: 5, parent_id: null },
  { id: 6, parent_id: 5 },
  { id: 7, parent_id: null },
];

/** TodoView.startDrag 과 같은 후보 구성 — 드래그 서브트리를 뺀 세로 순 행 */
const candidatesFor = (nodes: TodoNode[], draggedId: number): TreeRow[] => {
  const sub = subtreeIds(nodes, draggedId);
  return flattenTree(nodes).filter((r) => !sub.has(r.id));
};
const drop = (nodes: TodoNode[], draggedId: number, slot: DropSlot) =>
  resolveDrop(nodes, candidatesFor(nodes, draggedId), draggedId, slot);

// 밀린 목록은 done=0 AND due_date<오늘 로 걸러낸 부분 집합이라 부모가 빠질 수 있다.
// 여기서 한 행이라도 누락되면 화면에서 할 일이 사라진다 — 그래서 "전부 나온다"를 못 박는다.
describe("flattenSubset", () => {
  const ids = (rows: TreeRow[]) => rows.map((r) => r.id).sort((a, b) => a - b);

  it("부모가 목록에 있으면 그 아래로 들여쓴다", () => {
    expect(flattenSubset(TREE)).toEqual(flattenTree(TREE));
  });

  it("부모가 빠진 자식을 루트로 올린다 — flattenTree 는 이걸 통째로 잃는다", () => {
    // 부모 1 이 완료돼 밀린 목록에서 빠진 상황: 2·4·3 만 남는다
    const subset: TodoNode[] = [
      { id: 2, parent_id: 1 },
      { id: 4, parent_id: 2 },
      { id: 3, parent_id: 1 },
    ];
    expect(flattenTree(subset)).toEqual([]); // 기존 함수는 전부 잃는다
    expect(flattenSubset(subset)).toEqual([
      { id: 2, parent_id: 1, depth: 0 },
      { id: 4, parent_id: 2, depth: 1 }, // 2 와의 관계는 유지
      { id: 3, parent_id: 1, depth: 0 },
    ]);
  });

  it("어떤 입력에서도 모든 행이 정확히 한 번 나온다", () => {
    for (const subset of [
      TREE,
      TREE.filter((n) => n.id !== 1), // 루트 빠짐
      TREE.filter((n) => n.id !== 2), // 중간 빠짐
      [{ id: 9, parent_id: 99 }], // 부모가 아예 없는 행
    ]) {
      const rows = flattenSubset(subset);
      expect(ids(rows)).toEqual(ids(subset.map((n) => ({ ...n, depth: 0 }))));
      expect(new Set(rows.map((r) => r.id)).size).toBe(subset.length);
    }
  });

  it("순환이 있어도 멈추고, 걸린 행도 빠뜨리지 않는다", () => {
    const cyclic: TodoNode[] = [
      { id: 1, parent_id: 2 },
      { id: 2, parent_id: 1 },
      { id: 3, parent_id: null },
    ];
    const rows = flattenSubset(cyclic);
    expect(ids(rows)).toEqual([1, 2, 3]);
  });
});

describe("flattenTree", () => {
  it("렌더 순서 그대로 depth 를 매긴다", () => {
    expect(flattenTree(TREE)).toEqual([
      { id: 1, parent_id: null, depth: 0 },
      { id: 2, parent_id: 1, depth: 1 },
      { id: 4, parent_id: 2, depth: 2 },
      { id: 3, parent_id: 1, depth: 1 },
      { id: 5, parent_id: null, depth: 0 },
      { id: 6, parent_id: 5, depth: 1 },
      { id: 7, parent_id: null, depth: 0 },
    ]);
  });

  it("부모가 목록에 없는 행(고아)은 빠진다 — 호출부는 완전한 집합을 넘겨야 한다", () => {
    const rows = flattenTree([
      { id: 1, parent_id: null },
      { id: 9, parent_id: 99 }, // 다른 날에 남은 부모
    ]);
    expect(rows.map((r) => r.id)).toEqual([1]);
  });

  it("순환(자기 자신이 부모)은 루트에서 닿지 않아 무한 재귀가 되지 않는다", () => {
    const rows = flattenTree([
      { id: 1, parent_id: null },
      { id: 2, parent_id: 2 },
    ]);
    expect(rows.map((r) => r.id)).toEqual([1]);
  });
});

describe("subtreeIds / descendantCount", () => {
  it("자손 전체를 모은다 (삭제·드래그가 같은 판정을 쓴다)", () => {
    expect([...subtreeIds(TREE, 1)].sort()).toEqual([1, 2, 3, 4]);
    expect(descendantCount(TREE, 1)).toBe(3);
    expect(descendantCount(TREE, 2)).toBe(1);
    expect(descendantCount(TREE, 7)).toBe(0);
  });

  it("자기 자신을 부모로 가리키는 행이 있어도 멈춘다", () => {
    expect([...subtreeIds([{ id: 1, parent_id: 1 }], 1)]).toEqual([1]);
  });

  it("배열 순서(자식이 부모보다 앞)에 의존하지 않는다", () => {
    const reversed = [...TREE].reverse();
    expect([...subtreeIds(reversed, 1)].sort()).toEqual([1, 2, 3, 4]);
  });
});

describe("clampDropDepth", () => {
  const row = (depth: number): TreeRow => ({ id: 0, parent_id: null, depth });

  it("위 행의 하위까지만 들어갈 수 있다 (max = above.depth + 1)", () => {
    expect(clampDropDepth(9, row(2), null)).toBe(3);
    expect(clampDropDepth(0, row(2), null)).toBe(0);
  });

  it("아래 행의 부모를 뺏지 않는다 (min = below.depth)", () => {
    expect(clampDropDepth(0, row(2), row(2))).toBe(2);
    expect(clampDropDepth(3, row(2), row(1))).toBe(3);
  });

  it("목록 맨 위(above 없음)는 최상위뿐", () => {
    expect(clampDropDepth(5, null, row(0))).toBe(0);
    expect(clampDropDepth(-3, null, null)).toBe(0);
  });

  it("실제 행 배치에선 min > max 가 나오지 않는다 (below.depth <= above.depth + 1)", () => {
    const rows = flattenTree(TREE);
    rows.forEach((below, i) => {
      const above = rows[i - 1] ?? null;
      expect(below.depth).toBeLessThanOrEqual((above?.depth ?? -1) + 1);
    });
  });
});

describe("resolveDrop — 같은 부모 안 재정렬", () => {
  it("3 을 형제 2 앞으로 (idx=2 위치의 행 2 앞)", () => {
    expect(drop(TREE, 3, { idx: 1, depth: 1 })).toEqual({
      newParentId: 1,
      newSortOrder: 0,
      orderedSiblingIds: [3, 2],
    });
  });

  it("서브트리를 통째로 옮겨도 형제 순서만 바뀐다 (2 를 3 뒤로)", () => {
    // 후보: 1(0) 3(1) 5(0) 6(1) 7(0) — 2 와 자식 4 는 빠져 있다
    expect(drop(TREE, 2, { idx: 2, depth: 1 })).toEqual({
      newParentId: 1,
      newSortOrder: 1,
      orderedSiblingIds: [3, 2],
    });
  });

  it("제자리면 null — 커밋 자체를 생략한다", () => {
    expect(drop(TREE, 3, { idx: 3, depth: 1 })).toBeNull(); // 4 뒤 = 원래 자리
    expect(drop(TREE, 2, { idx: 1, depth: 1 })).toBeNull(); // 1 바로 아래 = 원래 자리
  });
});

describe("resolveDrop — 레벨 이동", () => {
  it("최상위 7 을 2 의 마지막 자식으로", () => {
    expect(drop(TREE, 7, { idx: 3, depth: 2 })).toEqual({
      newParentId: 2,
      newSortOrder: 1,
      orderedSiblingIds: [4, 7],
    });
  });

  it("깊은 4 를 최상위 끝으로 승격", () => {
    expect(drop(TREE, 4, { idx: 6, depth: 0 })).toEqual({
      newParentId: null,
      newSortOrder: 3,
      orderedSiblingIds: [1, 5, 7, 4],
    });
  });

  it("서브트리를 가진 5 를 1 의 자식으로 강등", () => {
    // 후보: 1(0) 2(1) 4(2) 3(1) 7(0) — 5,6 제외. idx=4 = 3 뒤
    expect(drop(TREE, 5, { idx: 4, depth: 1 })).toEqual({
      newParentId: 1,
      newSortOrder: 2,
      orderedSiblingIds: [2, 3, 5],
    });
  });
});

describe("resolveDrop — 방어", () => {
  it("자기 서브트리 안으로는 어떤 슬롯으로도 들어갈 수 없다", () => {
    for (const id of [1, 2, 5]) {
      const cand = candidatesFor(TREE, id);
      const sub = subtreeIds(TREE, id);
      for (let idx = 0; idx <= cand.length; idx++) {
        for (let depth = 0; depth <= 3; depth++) {
          const r = resolveDrop(TREE, cand, id, { idx, depth });
          if (r?.newParentId != null) expect(sub.has(r.newParentId)).toBe(false);
        }
      }
    }
  });

  it("깊이에 맞는 부모가 없으면 null (clamp 를 거치면 오지 않는 슬롯)", () => {
    expect(drop(TREE, 7, { idx: 0, depth: 1 })).toBeNull(); // 위에 아무것도 없음
    expect(drop(TREE, 7, { idx: 1, depth: 2 })).toBeNull(); // 1(depth 0) 바로 아래 = 손자 자리 없음
  });

  it("목록에 없는 id 는 null", () => {
    expect(drop(TREE, 999, { idx: 0, depth: 0 })).toBeNull();
  });

  it("모든 슬롯에서 newSortOrder 가 orderedSiblingIds 안 실제 위치와 일치한다", () => {
    // reorderTodos 가 배열 index 를 sort_order 로 쓰므로 둘이 어긋나면 조용히 순서가 틀어진다
    for (const node of TREE) {
      const cand = candidatesFor(TREE, node.id);
      for (let idx = 0; idx <= cand.length; idx++) {
        const above = cand[idx - 1] ?? null;
        const below = cand[idx] ?? null;
        for (let desired = -1; desired <= 4; desired++) {
          const depth = clampDropDepth(desired, above, below);
          const r = resolveDrop(TREE, cand, node.id, { idx, depth });
          if (!r) continue;
          expect(r.orderedSiblingIds[r.newSortOrder]).toBe(node.id);
          expect([...r.orderedSiblingIds].sort()).toEqual(
            [
              ...childrenOf(TREE, r.newParentId)
                .map((n) => n.id)
                .filter((id) => id !== node.id),
              node.id,
            ].sort(),
          );
        }
      }
    }
  });
});
