// 레일 완료 점의 규칙 — 언제 켜지고 언제 꺼지나.
// 알림은 "놓치지 않는 것"과 "쌓이지 않는 것" 둘 다 지켜야 쓸모가 있다.

import { beforeEach, describe, expect, it } from "vitest";
import {
  markRailDone,
  railDoneSet,
  resetRailDoneForTest,
  setRailFocus,
} from "./railDone";

describe("railDone", () => {
  beforeEach(() => resetRailDoneForTest());

  it("다른 칸에서 끝나면 점이 켜진다", () => {
    setRailFocus("notes");
    markRailDone("todo");
    expect([...railDoneSet()]).toEqual(["todo"]);
  });

  it("보고 있는 칸에서 끝나면 켜지지 않는다 — 눈앞에서 끝난 걸 부를 이유가 없다", () => {
    setRailFocus("todo");
    markRailDone("todo");
    expect(railDoneSet().size).toBe(0);
  });

  it("그 칸에 들어가면 꺼진다 — 읽은 알림은 남기지 않는다", () => {
    setRailFocus(null);
    markRailDone("notes");
    expect(railDoneSet().has("notes")).toBe(true);
    setRailFocus("notes");
    expect(railDoneSet().has("notes")).toBe(false);
  });

  it("다른 칸으로 옮겨도 남의 점은 그대로다", () => {
    setRailFocus(null);
    markRailDone("todo");
    markRailDone("notes");
    setRailFocus("todo");
    expect([...railDoneSet()]).toEqual(["notes"]);
  });

  it("같은 칸이 여러 번 끝나도 점은 하나다", () => {
    setRailFocus(null);
    markRailDone("todo");
    markRailDone("todo");
    expect(railDoneSet().size).toBe(1);
  });

  it("스냅샷은 바뀔 때만 새 참조다 — 매번 새 Set 이면 무한 렌더가 된다", () => {
    setRailFocus(null);
    markRailDone("todo");
    const a = railDoneSet();
    markRailDone("todo"); // 이미 켜져 있어 아무 일도 없어야 한다
    expect(railDoneSet()).toBe(a);
    markRailDone("notes");
    expect(railDoneSet()).not.toBe(a);
  });
});
