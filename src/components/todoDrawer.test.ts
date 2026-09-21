import { describe, expect, it } from "vitest";
import { groupParked, parkedDays, parkedRoots } from "./TodoDrawer";
import type { Todo } from "../types";

const at = (id: number, parent: number | null, content = `t${id}`): Todo =>
  ({
    id,
    content,
    due_date: "2026-09-21",
    scope: "day",
    done: 0,
    completed_at: null,
    parent_id: parent,
    sort_order: id,
    created_at: 0,
    updated_at: 0,
    parked_at: 1,
  }) as Todo;

describe("parkedRoots", () => {
  it("부모가 함께 내려와 있으면 자식은 카드로 세우지 않는다", () => {
    // parkSubtree 는 서브트리째 내려놓으므로 부모와 자식이 같이 들어온다.
    // 둘 다 카드로 세우면 같은 덩어리가 두 번 보인다.
    const rows = [at(1, null), at(2, 1), at(3, 2)];
    expect(parkedRoots(rows).map((r) => r.id)).toEqual([1]);
  });

  it("부모가 오늘에 남아 있으면 그 자식이 곧 맨 위다", () => {
    // 자식만 내려놓은 경우 — 부모는 목록에 없으므로 자식이 스스로 카드가 된다
    const rows = [at(7, 99)];
    expect(parkedRoots(rows).map((r) => r.id)).toEqual([7]);
  });

  it("형제는 각각 카드가 된다", () => {
    expect(parkedRoots([at(1, null), at(2, null)]).map((r) => r.id)).toEqual([1, 2]);
  });

  it("빈 목록은 빈 결과", () => {
    expect(parkedRoots([])).toEqual([]);
  });
});

describe("parkedDays", () => {
  const DAY = 86_400_000;
  it("내려놓은 그날은 0 이다", () => {
    expect(parkedDays(1000, 1000)).toBe(0);
    expect(parkedDays(1000, 1000 + DAY - 1)).toBe(0);
  });

  it("하루를 꽉 채우면 1 이다", () => {
    expect(parkedDays(0, DAY)).toBe(1);
    expect(parkedDays(0, DAY * 17 + 5)).toBe(17);
  });

  it("시계가 뒤로 간 경우에도 음수를 내지 않는다", () => {
    // 기기 시간 변경이나 타임존 보정으로 now 가 과거가 될 수 있다 — "-3일"은 표시할 말이 아니다
    expect(parkedDays(DAY * 5, 0)).toBe(0);
  });
});

describe("groupParked", () => {
  const anc = [
    { id: 1, content: "Devops", parent_id: null },
    { id: 2, content: "Amber", parent_id: null },
    { id: 3, content: "릴리스", parent_id: 2 },
  ];

  it("같은 부모에서 내려온 것은 머리글 하나로 묶는다", () => {
    const g = groupParked([at(10, 1), at(11, 1)], anc);
    expect(g).toHaveLength(1);
    expect(g[0].key).toBe("Devops");
    expect(g[0].items.map((r) => r.id)).toEqual([10, 11]);
  });

  it("경로 전체가 머리글이 된다 — 같은 이름이라도 자리가 다르면 다른 묶음이다", () => {
    expect(groupParked([at(10, 3)], anc)[0].key).toBe("Amber › 릴리스");
  });

  it("원래 최상위였으면 머리글이 없다", () => {
    const g = groupParked([at(10, null)], anc);
    expect(g[0].key).toBe("");
  });

  it("떨어져 있으면 합치지 않는다 — 목록 순서가 곧 나이다", () => {
    // 오래 묵은 순서(listParked 의 parked_at ASC)를 흔들면 "17일째" 가 위에서 밀려난다
    const g = groupParked([at(10, 1), at(11, 2), at(12, 1)], anc);
    expect(g.map((x) => x.key)).toEqual(["Devops", "Amber", "Devops"]);
  });

  it("빈 목록은 빈 묶음", () => {
    expect(groupParked([], anc)).toEqual([]);
  });
});
