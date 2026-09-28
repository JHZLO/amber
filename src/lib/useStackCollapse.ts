// 할 일 판 오른쪽의 카드 묶음(.todo-stack) — 리포트를 읽으려고 굴리면 **리포트가 커진다.**
//
// 왜: 카드마다 안에서 구르게 했더니(카드 틀이 화면 밖으로 밀리지 않게) 리포트가 할 일 카드 아래
// 좁은 칸에서만 굴러서, 긴 리포트를 손톱만 한 창으로 읽어야 했다. 반대로 판 전체를 굴리면 카드의
// 둥근 윗변과 아랫변이 화면 가장자리에서 네모로 잘렸다. 둘 다 피하려면 굴림이 **공간을 옮겨야** 한다.
//
// 그래서 아래로 굴리면 먼저 할 일 카드를 묶음 위로 밀어 올리고(margin-top 을 음수로), 비는 만큼
// 리포트 카드(flex-grow)가 늘어난다. 할 일 카드가 다 올라가면 그다음부터 리포트 안이 구른다.
// 위로 굴리면 거꾸로 — 리포트가 맨 위에 닿은 뒤에 할 일 카드가 내려온다. 브라우저의 스크롤 체이닝과
// 같은 순서라 손이 기대하는 대로 움직인다. 묶음은 카드와 같은 둥근 틀로 잘라 밀려 나가는 카드도
// 모서리가 둥글다(styles.css .todo-stack).
//
// 매 휠 이벤트를 React 상태로 올리지 않는다 — 초당 수십 번이라 할 일 목록 전체가 다시 그려진다.
// 접힌 양은 ref 에 두고 스타일만 직접 바꾼다.

import { useEffect, useRef, type RefObject } from "react";

export interface WheelInput {
  /** 이번 휠의 세로 이동(px, 아래가 +) */
  dy: number;
  /** 지금 접힌 양(px) */
  collapse: number;
  /** 최대로 접을 수 있는 양 — 할 일 카드 높이 + 카드 사이 틈 */
  max: number;
  /** 커서가 할 일 카드 위인가(card), 리포트 쪽인가(report) */
  zone: "card" | "report";
  /** 커서 밑에서 가장 안쪽의 세로 스크롤 칸: 위로 남은 양, 아래로 남은 양 */
  innerTop: number;
  innerBelow: number;
  /** 리포트 본문이 아래로 더 숨기고 있는 양 — 이만큼만 접으면 리포트가 다 보인다 */
  reportBelow: number;
}

export interface WheelPlan {
  /** 접힌 양의 새 값 */
  collapse: number;
  /** 이 휠을 우리가 먹었는가(true 면 기본 스크롤을 막는다) */
  consumed: boolean;
  /** 먹고 남은 아래 이동 — 리포트 본문을 그만큼 굴린다 */
  spill: number;
}

const NATIVE = (collapse: number): WheelPlan => ({ collapse, consumed: false, spill: 0 });

/** 휠 한 번을 어디에 쓸지 정한다(순수 함수 — 테스트가 붙는 자리) */
export function planWheel(s: WheelInput): WheelPlan {
  const c = Math.min(Math.max(s.collapse, 0), Math.max(s.max, 0));
  if (s.dy > 0) {
    // 할 일 카드 위에서는 목록이 먼저 구른다 — 목록 끝에 닿아야 판이 움직인다(체이닝)
    if (s.zone === "card" && s.innerBelow > 0) return NATIVE(c);
    // 리포트가 더 보여 줄 게 있을 때만 접는다 — 다 보이는데 할 일을 치우면 빈 판만 늘어난다
    const room = Math.min(s.max - c, Math.max(s.reportBelow, 0));
    if (room <= 0) return NATIVE(c);
    const take = Math.min(s.dy, room);
    return {
      collapse: c + take,
      consumed: true,
      // 다 접고도 남은 만큼은 리포트 본문으로 넘긴다 — 한 번의 긴 휠이 중간에 끊기지 않게
      spill: s.zone === "report" ? s.dy - take : 0,
    };
  }
  if (s.dy < 0) {
    // 위로는 안쪽이 먼저다 — 리포트(또는 목록)가 맨 위에 닿은 뒤에 할 일 카드가 내려온다
    if (s.innerTop > 0) return NATIVE(c);
    if (c <= 0) return NATIVE(c);
    return { collapse: Math.max(0, c + s.dy), consumed: true, spill: 0 };
  }
  return NATIVE(c);
}

/** target 에서 stop 까지 올라가며 실제로 세로로 구르는 첫 칸을 찾는다 */
function innerScroller(target: Element | null, stop: Element): HTMLElement | null {
  for (let el = target; el && el !== stop; el = el.parentElement) {
    if (!(el instanceof HTMLElement)) continue;
    const oy = getComputedStyle(el).overflowY;
    if ((oy === "auto" || oy === "scroll") && el.scrollHeight > el.clientHeight + 1) return el;
  }
  return null;
}

const below = (el: HTMLElement) => Math.max(0, el.scrollHeight - el.clientHeight - el.scrollTop);

/** 휠 한 번의 세로 이동을 px 로 — 줄/쪽 단위로 오는 장치도 있다 */
function wheelPx(e: WheelEvent, page: number): number {
  if (e.deltaMode === 1) return e.deltaY * 16;
  if (e.deltaMode === 2) return e.deltaY * page;
  return e.deltaY;
}

/**
 * stackRef 안에서 휠을 받아 cardRef(할 일 카드)를 밀어 올리고 되돌린다.
 * resetKey 가 바뀌면(날짜/단위 이동) 펼친 상태로 돌아간다.
 */
export function useStackCollapse(
  stackRef: RefObject<HTMLElement | null>,
  cardRef: RefObject<HTMLElement | null>,
  resetKey: string,
) {
  const collapse = useRef(0);

  const apply = (c: number) => {
    collapse.current = c;
    const card = cardRef.current;
    if (card) card.style.marginTop = c > 0 ? `${-c}px` : "";
  };

  // 다른 날로 가면 새 목록을 위에서부터 본다
  useEffect(() => {
    apply(0);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [resetKey]);

  useEffect(() => {
    const stack = stackRef.current;
    const card = cardRef.current;
    if (!stack || !card) return;

    const maxOf = () => {
      const report = stack.querySelector<HTMLElement>(".report");
      if (!report) return 0;
      return card.offsetHeight + (parseFloat(getComputedStyle(report).marginTop) || 0);
    };

    const onWheel = (e: WheelEvent) => {
      if (e.ctrlKey) return; // 트랙패드 핀치(확대)
      if (Math.abs(e.deltaX) > Math.abs(e.deltaY)) return; // 가로 굴림(표, 코드)
      const report = stack.querySelector<HTMLElement>(".report-scroll");
      if (!report) return;
      const target = e.target instanceof Element ? e.target : null;
      const zone = target && card.contains(target) ? "card" : "report";
      const inner = innerScroller(target, stack);
      const plan = planWheel({
        dy: wheelPx(e, stack.clientHeight),
        collapse: collapse.current,
        max: maxOf(),
        zone,
        innerTop: inner ? inner.scrollTop : 0,
        innerBelow: inner ? below(inner) : 0,
        reportBelow: below(report),
      });
      if (plan.collapse !== collapse.current) apply(plan.collapse);
      if (!plan.consumed) return;
      e.preventDefault();
      if (plan.spill > 0) report.scrollTop += plan.spill;
    };

    // 할 일이 줄어 카드가 낮아졌는데 예전만큼 접혀 있으면 리포트 윗부분까지 틀 밖으로 밀린다
    const ro = new ResizeObserver(() => {
      const max = maxOf();
      if (collapse.current > max) apply(max);
    });
    ro.observe(card);
    ro.observe(stack);

    // passive: false — 먹은 휠은 기본 스크롤을 막아야 한다
    stack.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      stack.removeEventListener("wheel", onWheel);
      ro.disconnect();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}
