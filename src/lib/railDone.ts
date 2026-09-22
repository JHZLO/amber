// 레일의 **완료 점** — 백그라운드 작업이 끝났다는 표시.
//
// 도는 동안의 맥동 점(`.rail-busy`)과 다른 물건이다: 맥동은 "지금 돌고 있다"고, 이건
// "다 됐으니 와서 봐라"다. 그래서 색이 있고(초록) 움직이지 않는다 — 끝난 일이 계속
// 깜빡이면 아직 도는 것과 구분이 안 된다.
//
// **그 탭에 들어가면 사라진다.** 사람이 본 알림을 남겨 둘 이유가 없고, 남겨 두면
// 점을 지우는 일이 또 하나의 할 일이 된다. 그래서 지우는 버튼도 두지 않는다.
//
// 지금 보고 있는 탭에서 끝난 일에는 점이 아예 켜지지 않는다(`setRailFocus`) — 눈앞에서
// 끝난 것을 "와서 보라"고 부르는 건 군더더기고, 켰다 끄면 한 프레임 깜빡인다.

import { useSyncExternalStore } from "react";

/** 백그라운드 작업을 갖는 레일 칸 (할 일 = 리포트 생성, 노트 = AI 전문 작성) */
export type RailSection = "todo" | "notes";

const listeners = new Set<() => void>();
let done = new Set<RailSection>();
let focused: RailSection | null = null;

// getSnapshot 은 값이 같으면 **같은 참조**를 돌려줘야 한다(매번 새 Set 이면 무한 렌더).
const emit = () => {
  done = new Set(done);
  listeners.forEach((l) => l());
};

/** 지금 보고 있는 칸. 여기 있던 점은 즉시 꺼지고, 여기서 끝난 일은 점을 켜지 않는다 */
export function setRailFocus(section: RailSection | null): void {
  focused = section;
  if (section && done.has(section)) {
    done.delete(section);
    emit();
  }
}

/** 작업 하나가 성공으로 끝났다 */
export function markRailDone(section: RailSection): void {
  if (section === focused || done.has(section)) return;
  done.add(section);
  emit();
}

export function railDoneSet(): ReadonlySet<RailSection> {
  return done;
}

export function resetRailDoneForTest(): void {
  done = new Set();
  focused = null;
  emit();
}

export function useRailDone(): ReadonlySet<RailSection> {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => done,
  );
}
