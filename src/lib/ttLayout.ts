// 타임테이블 겹침 배치 — 한 날짜(컬럼) 안에서 블록의 가로 위치를 정하는 순수 함수.
// 화면 없이 검증되어야 하는 규칙이라 컴포넌트가 아니라 여기 둔다 (findFreeSlot 과 같은 이유).
//
// 겹침엔 의미가 다른 두 종류가 있고, 형태도 달라야 한다.
//   포함(09–18 안의 10–11) = 큰 덩어리 '안'의 일정 → 캐스케이드(들여쓰기 + 위에 얹음).
//     lane 을 먹지 않으므로 09–18 블록이 회의 하나 때문에 하루 종일 반쪽이 되지 않는다.
//   교차(09–12 와 11–14) = 대등한 둘 → lane 균등 분할(구글 캘린더식).
// 그리고 컬럼 우측 GUTTER 는 항상 맨 그리드로 남긴다 — 블록이 컬럼을 덮어도
// "빈 곳 세로 드래그 = 생성"(DayTimetable.onGridDown)이 닿을 표면이 있어야 겹쳐 추가가 된다.

import type { TimeBlock } from "../types";

export const GUTTER = 12; // 컬럼 우측 상시 여백(px) — 겹쳐 만들기 드래그가 닿는 표면
export const CASCADE_INSET = 14; // 포함 블록 들여쓰기(px)
export const MAX_DEPTH = 3; // 들여쓰기 누적 상한 (그 아래는 z 만 올린다)
export const MIN_LANE = 56; // lane 최소 폭(px) — 이보다 좁으면 제목이 한 글자도 안 남는다
export const BLOCK_GAP = 2; // 이웃 블록 사이 간격(px)

export const GHOST_ID = -1; // 아직 DB 에 없는 '만드는 중' 블록 — 배치에만 참여한다

/** 드래그 중 위치. id 가 null 이면 '만드는 중'(아직 DB 에 없음) */
export type GhostAt = {
  id: number | null;
  date: string;
  start: number;
  end: number;
};

/** 드래그 중인 위치를 반영한 '미리보기 목록'.
 *  이걸 layoutColumn 에 먹이면 침범당한 블록들이 놓기 **전에** 밀려 2열이 된다.
 *  DB 는 건드리지 않는다 — 커밋은 여전히 mouseup 에서 한 번. */
export function withGhost(
  blocks: TimeBlock[],
  ghost: GhostAt | null,
): TimeBlock[] {
  if (!ghost) return blocks;
  if (ghost.id == null) {
    return [
      ...blocks,
      {
        id: GHOST_ID,
        date: ghost.date,
        start_min: ghost.start,
        end_min: ghost.end,
        title: "",
        todo_id: null,
        created_at: 0,
        updated_at: 0,
      },
    ];
  }
  return blocks.map((b) =>
    b.id === ghost.id
      ? { ...b, date: ghost.date, start_min: ghost.start, end_min: ghost.end }
      : b,
  );
}

/** 블록 가로 위치 — 컬럼 폭은 %, 거터·들여쓰기는 고정 px 이라 둘을 함께 들고 다닌다.
 *  최종 CSS 는 `calc(pctL% + pxL px)` / `calc(pctW% + pxW px)` 로 합성. */
export type Box = { pctL: number; pxL: number; pctW: number; pxW: number };
export type Placed = { b: TimeBlock; box: Box; depth: number };

const sliceLane = (box: Box, lane: number, lanes: number): Box => ({
  pctL: box.pctL + (box.pctW * lane) / lanes,
  pxL: box.pxL + (box.pxW * lane) / lanes,
  pctW: box.pctW / lanes,
  pxW: box.pxW / lanes,
});
const insetBox = (box: Box): Box => ({
  pctL: box.pctL,
  pxL: box.pxL + CASCADE_INSET,
  pctW: box.pctW,
  pxW: box.pxW - CASCADE_INSET,
});
const boxPx = (box: Box, gridW: number) => (gridW * box.pctW) / 100 + box.pxW;

// ---- CSS 합성 ----
// lane 분할이 나누기를 하므로 값에 부동소수 꼬리가 붙는다. 그대로 쓰면 CSS 가 길어지고
// `calc(100% + -14px)` 같은 모호한 꼴도 나오므로, 반올림과 부호를 여기서 한 번에 정리한다.
const round = (n: number) => Math.round(n * 1e4) / 1e4;
const signed = (px: number) =>
  px < 0 ? `- ${round(-px)}px` : `+ ${round(px)}px`;

export const boxLeft = (box: Box) =>
  `calc(${round(box.pctL)}% ${signed(box.pxL)})`;
/** gap = 이웃 블록과의 간격.
 *  하한(아무리 좁아도 잡을 수 있게)은 여기서 `max()` 로 감싸지 않고 CSS `min-width` 로 준다 —
 *  하한은 폭 계산이 아니라 제약이라 그쪽이 제 자리이고, 이 값은 lane 밀림 때 transition 으로
 *  보간되므로 보간 대상은 단순 `calc()` 로 두는 편이 안전하다. */
export const boxWidth = (box: Box, gap = BLOCK_GAP) =>
  `calc(${round(box.pctW)}% ${signed(box.pxW - gap)})`;

/** a 가 b 를 진짜로(같은 범위는 제외) 감싸는가 — 같은 범위끼리는 교차로 봐서 나란히 놓는다 */
const swallows = (a: TimeBlock, b: TimeBlock) =>
  a.start_min <= b.start_min &&
  b.end_min <= a.end_min &&
  (a.start_min < b.start_min || b.end_min < a.end_min);

/** over = 0 이면 제 lane 을 받은 것, 1 이상이면 lane 위에 얹힌 캐스케이드 단계 */
type Lane = { b: TimeBlock; lane: number; lanes: number; over: number };

/** 서로 포함하지 않는(=교차만 하는) 블록들의 lane 배치.
 *  cluster = start 순으로 훑으며 진행 중인 묶음의 최대 end 이전에 시작하는 블록들.
 *  maxLanes 를 넘으면 새 lane 대신 가장 먼저 비는 lane **위에** 얹는다 — 숨기지 않는다
 *  (시간 그리드의 블록은 언제나 잡아서 옮길 수 있어야 한다).
 *  얹히는 블록끼리도 first-fit 을 따로 돌려 단계를 벌린다. 안 그러면 같은 시간에 얹힌
 *  둘이 완전히 같은 자리에 겹쳐 하나가 통째로 가려진다. */
function laneSplit(blocks: TimeBlock[], maxLanes: number): Lane[] {
  const sorted = [...blocks].sort(
    (a, b) => a.start_min - b.start_min || b.end_min - a.end_min || a.id - b.id,
  );
  const out: Lane[] = [];
  let cluster: Lane[] = [];
  let laneEnds: number[] = [];
  let overEnds: number[] = []; // 얹힌 단계별 마지막 end
  let clusterEnd = -1;
  const flush = () => {
    for (const item of cluster) item.lanes = laneEnds.length;
    out.push(...cluster);
    cluster = [];
    laneEnds = [];
    overEnds = [];
    clusterEnd = -1;
  };
  for (const b of sorted) {
    if (cluster.length && b.start_min >= clusterEnd) flush();
    let over = 0;
    let lane = laneEnds.findIndex((end) => end <= b.start_min);
    if (lane === -1) {
      if (laneEnds.length < maxLanes) {
        lane = laneEnds.length;
        laneEnds.push(0);
      } else {
        // 더 쪼개면 글자가 안 남는다 — 가장 빨리 비는 lane 위에 캐스케이드로 얹는다
        let step = overEnds.findIndex((end) => end <= b.start_min);
        if (step === -1) {
          step = overEnds.length;
          overEnds.push(0);
        }
        overEnds[step] = b.end_min;
        over = step + 1;
        lane = laneEnds.reduce((m, e, i) => (e < laneEnds[m] ? i : m), 0);
      }
    }
    laneEnds[lane] = Math.max(laneEnds[lane], b.end_min);
    cluster.push({ b, lane, lanes: 1, over });
    clusterEnd = Math.max(clusterEnd, b.end_min);
  }
  flush();
  return out;
}

/** 한 날짜(컬럼)의 블록 배치.
 *  root  = 거터를 뺀 컬럼 내부 상자
 *  gridW = 그리드 실폭(px) — lane 하한(MIN_LANE) 판정에만 쓴다 */
export function layoutColumn(
  blocks: TimeBlock[],
  root: Box,
  gridW: number,
): Placed[] {
  // 부모 = 자기를 감싸는 것 중 가장 작은 것 (없으면 최상위). 포함은 길이 기준 부분순서라
  // 순환이 생기지 않는다.
  const dur = (b: TimeBlock) => b.end_min - b.start_min;
  // 최상위 센티넬은 **null** 이어야 한다. 숫자를 쓰면 그 id 를 가진 블록(GHOST_ID = -1 이 정확히
  // 그랬다)이 자기 자신이 든 최상위 그룹을 자식으로 받아 무한 재귀에 빠진다 — 화면이 먹통이 됐다.
  const kids = new Map<number | null, TimeBlock[]>(); // 부모 id → 직속 자식들 (null = 최상위)
  for (const b of blocks) {
    let best: TimeBlock | null = null;
    for (const a of blocks) {
      if (a.id === b.id || !swallows(a, b)) continue;
      if (
        !best ||
        dur(a) < dur(best) ||
        (dur(a) === dur(best) && a.id < best.id)
      )
        best = a;
    }
    const key = best?.id ?? null;
    const list = kids.get(key);
    if (list) list.push(b);
    else kids.set(key, [b]);
  }

  const out: Placed[] = [];
  const place = (group: TimeBlock[], box: Box, depth: number) => {
    // group 안엔 포함 관계가 없다(전부 같은 부모의 직속 자식) → 교차뿐 → lane 분할
    const lanes = Math.max(1, Math.floor(boxPx(box, gridW) / MIN_LANE));
    for (const item of laneSplit(group, lanes)) {
      let myBox = sliceLane(box, item.lane, item.lanes);
      let d = depth;
      // 얹힌 단계만큼 들여쓴다. 들여쓰기만 MAX_DEPTH 에서 멈추고 z(depth)는 계속 올라간다 —
      // 폭이 0 으로 수렴하는 걸 막으면서도 위아래 순서는 유지된다.
      for (let i = 0; i < item.over; i++) {
        d += 1;
        if (d <= MAX_DEPTH) myBox = insetBox(myBox);
      }
      out.push({ b: item.b, box: myBox, depth: d });
      const mine = kids.get(item.b.id);
      if (mine) place(mine, d < MAX_DEPTH ? insetBox(myBox) : myBox, d + 1);
    }
  };
  place(kids.get(null) ?? [], root, 0);
  return out;
}
