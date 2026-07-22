// 데이 타임테이블 — 할 일 탭 좌측 pane 하단, 시간 계획(구글 캘린더식 time-blocking).
// 뷰 3종(구글 캘린더의 일/주/월): 일=선택 날짜 1컬럼, 주=일~토 7컬럼(같은 시간 그리드 지오메트리 공유,
// 컬럼이 날짜 축), 월=그 달의 블록을 날짜별로 묶은 아젠다 리스트(읽기 전용 + 클릭=날짜 선택).
// 시간 좌표는 "자정 기준 분"(로컬 벽시계) — UTC ms 재해석 금지(.claude/DESIGN.md §10).
// 인터랙션(§8 포인터 드래그 규약): 빈 곳 세로 드래그=생성(15분 스냅, 누른 컬럼 날짜), 블록 드래그=
// 이동(5px 임계, 주 뷰에선 가로로 다른 날짜 컬럼까지), 아래 가장자리=리사이즈. 드래그 중엔 ghost
// 상태만 갱신하고 놓을 때 DB 커밋 → onChanged 로 부모가 재로딩.
// 상태 문법(§3): 미완료=아웃라인+좌측 바, 진행 중=primary 필, 완료(연동)=text-3+취소선. 색 없음.

import {
  useEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
} from "react";
import type { TimeBlock, Todo } from "../types";
import {
  createBlock,
  deleteBlock,
  nowMinute,
  renameBlock,
  updateBlockTime,
} from "../lib/timeBlocks";
import { WEEKDAYS_KO, formatDayLong, parseLocalDate } from "../lib/date";
import { Icon } from "../icons";

/** 타임테이블 뷰 모드 — 부모(TodoView)가 소유·영속하고 로드 범위도 이걸로 정한다 */
export type TtView = "day" | "week" | "month";

export const TT_HOUR_H = 44; // px/시간 — styles.css --tt-hour 와 동기
const SNAP = 15; // 분 스냅 (구글 캘린더와 동일)
const MIN_DUR = 15;
const DAY_MIN = 1440;

const errMsg = (e: unknown) => (e instanceof Error ? e.message : String(e));
const pad2 = (n: number) => String(n).padStart(2, "0");
const fmtMin = (m: number) => `${pad2(Math.floor(m / 60))}:${pad2(m % 60)}`;
const minToY = (m: number) => (m / 60) * TT_HOUR_H;
const yToMin = (y: number) => (y / TT_HOUR_H) * 60;
const clampN = (v: number, lo: number, hi: number) =>
  Math.max(lo, Math.min(hi, v));
const snapFloor = (m: number) => Math.floor(m / SNAP) * SNAP;
const snapRound = (m: number) => Math.round(m / SNAP) * SNAP;

const VIEW_LABELS: Record<TtView, string> = {
  day: "일",
  week: "주",
  month: "월",
};

// 드래그 중 화면에만 반영되는 임시 상태 (커밋은 mouseup 에서 한 번). day = 컬럼 index
type Ghost =
  | { kind: "create"; day: number; start: number; end: number }
  | { kind: "move"; id: number; day: number; start: number; end: number }
  | { kind: "resize"; id: number; day: number; start: number; end: number };

/** 겹침 lane 배치 — 구글 캘린더처럼 "서로(이행적으로) 겹치는 묶음" 안에서 폭을 균등 분할.
 *  cluster = start 순으로 훑으며 진행 중인 묶음의 최대 end 이전에 시작하는 블록들. */
function layoutLanes(
  blocks: TimeBlock[],
): { b: TimeBlock; lane: number; lanes: number }[] {
  const sorted = [...blocks].sort(
    (a, b) => a.start_min - b.start_min || b.end_min - a.end_min || a.id - b.id,
  );
  const out: { b: TimeBlock; lane: number; lanes: number }[] = [];
  let cluster: { b: TimeBlock; lane: number; lanes: number }[] = [];
  let laneEnds: number[] = [];
  let clusterEnd = -1;
  const flush = () => {
    for (const item of cluster) item.lanes = laneEnds.length;
    out.push(...cluster);
    cluster = [];
    laneEnds = [];
    clusterEnd = -1;
  };
  for (const b of sorted) {
    if (cluster.length && b.start_min >= clusterEnd) flush();
    let lane = laneEnds.findIndex((end) => end <= b.start_min);
    if (lane === -1) {
      lane = laneEnds.length;
      laneEnds.push(0);
    }
    laneEnds[lane] = b.end_min;
    cluster.push({ b, lane, lanes: 1 });
    clusterEnd = Math.max(clusterEnd, b.end_min);
  }
  flush();
  return out;
}

export function DayTimetable({
  view,
  onViewChange,
  date,
  days,
  today,
  active,
  blocks,
  todos,
  focus,
  onChanged,
  onError,
  onSelectDate,
}: {
  view: TtView;
  onViewChange: (v: TtView) => void;
  date: string; // 선택 날짜
  days: string[]; // 시간 그리드 컬럼 (일: [date], 주: 일~토 7개) — 부모가 계산
  today: string;
  active: boolean;
  blocks: TimeBlock[]; // 로드된 범위 전체 (뷰 범위와 일치)
  todos: Todo[]; // 연동 블록 표시용(내용·완료 미러)
  focus: { min: number; nonce: number } | null; // "시간표에 넣기" 후 스크롤 타깃
  onChanged: () => void;
  onError: (msg: string) => void;
  onSelectDate: (d: string) => void;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<HTMLDivElement>(null);
  const [ghost, setGhost] = useState<Ghost | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editText, setEditText] = useState("");

  const isTimeGrid = view !== "month";
  const todoById = new Map(todos.map((t) => [t.id, t]));
  const dayIdx = new Map(days.map((d, i) => [d, i]));
  const todayCol = dayIdx.get(today);
  const colW = 100 / days.length;

  // '지금' 분 — 오늘 컬럼이 보일 때만 30초마다 갱신 (라인·진행 중 필 표시)
  const [nowMin, setNowMin] = useState(nowMinute);
  useEffect(() => {
    if (!active || !isTimeGrid || todayCol == null) return;
    setNowMin(nowMinute());
    const t = setInterval(() => setNowMin(nowMinute()), 30_000);
    return () => clearInterval(t);
  }, [active, isTimeGrid, todayCol]);

  // 뷰/날짜가 바뀌면 자동 스크롤: 오늘이 보이면 지금−1h, 아니면 09:00.
  // 주 뷰에선 같은 주 안의 날짜 클릭으로 리스크롤되지 않게 주 시작을 키로 쓴다.
  const scrollKey = isTimeGrid ? `${view}:${days[0]}` : view;
  const scrolledFor = useRef<string | null>(null);
  useEffect(() => {
    if (!active || !isTimeGrid) return;
    if (scrolledFor.current === scrollKey) return;
    scrolledFor.current = scrollKey;
    const el = scrollRef.current;
    if (!el) return;
    const target = todayCol != null ? Math.max(nowMinute() - 60, 0) : 9 * 60;
    el.scrollTop = minToY(target);
  }, [active, isTimeGrid, scrollKey, todayCol]);

  // "시간표에 넣기"로 만든 블록이 화면 밖이면 그리로 스크롤 (시간 그리드 뷰에서만)
  useEffect(() => {
    if (!focus || !isTimeGrid) return;
    const el = scrollRef.current;
    if (!el) return;
    const top = minToY(Math.max(focus.min - 30, 0));
    const bottom = top + minToY(90);
    if (top < el.scrollTop || bottom > el.scrollTop + el.clientHeight) {
      el.scrollTop = top;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focus?.nonce]);

  // 그리드 좌표: 세로=분, 가로=컬럼(날짜) index
  const gridPos = (clientX: number, clientY: number) => {
    const rect = gridRef.current!.getBoundingClientRect();
    return {
      min: yToMin(clientY - rect.top),
      day: clampN(
        Math.floor(((clientX - rect.left) / rect.width) * days.length),
        0,
        days.length - 1,
      ),
    };
  };

  // 빈 곳 세로 드래그 = 생성 (누른 컬럼의 날짜로). 클릭만으론 만들지 않는다(오조작 방지) — 4px 임계.
  function onGridDown(e: ReactMouseEvent) {
    if (e.button !== 0 || editingId != null) return;
    const target = e.target as HTMLElement;
    if (target.closest(".tt-block")) return;
    e.preventDefault(); // 텍스트 선택 방지(§8)
    const y0 = e.clientY;
    const press = gridPos(e.clientX, y0);
    const anchor = clampN(snapFloor(press.min), 0, DAY_MIN - SNAP);
    let moved = false;
    const calc = (clientY: number) => {
      const cur = clampN(snapFloor(gridPos(e.clientX, clientY).min), 0, DAY_MIN - SNAP);
      return {
        start: Math.min(anchor, cur),
        end: Math.max(anchor, cur) + SNAP,
      };
    };
    const onMove = (ev: MouseEvent) => {
      if (!moved && Math.abs(ev.clientY - y0) < 4) return;
      if (!moved) {
        moved = true;
        document.body.classList.add("dragging-rows");
      }
      const { start, end } = calc(ev.clientY);
      setGhost({ kind: "create", day: press.day, start, end });
    };
    const onUp = (ev: MouseEvent) => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      document.body.classList.remove("dragging-rows");
      setGhost(null);
      if (!moved) return;
      const { start, end } = calc(ev.clientY);
      void (async () => {
        try {
          const id = await createBlock(days[press.day], start, end, "");
          onChanged();
          setEditingId(id); // 생성 직후 제목 인라인 입력
          setEditText("");
        } catch (err) {
          onError(errMsg(err));
        }
      })();
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  // 블록 몸통 드래그 = 이동(5px 임계, 주 뷰에선 가로로 날짜 이동). 임계 미만 = 클릭 → 인라인 편집.
  function onBlockDown(e: ReactMouseEvent, b: TimeBlock) {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest(".tt-resize") || target.closest(".tt-del")) return;
    if (target.closest("input")) return; // 편집 중인 입력은 드래그로 뺏지 않는다
    e.preventDefault();
    e.stopPropagation();
    const dur = b.end_min - b.start_min;
    const press = gridPos(e.clientX, e.clientY);
    const grabOffset = press.min - b.start_min;
    const x0 = e.clientX;
    const y0 = e.clientY;
    let moved = false;
    let lastStart = b.start_min;
    let lastDay = dayIdx.get(b.date) ?? press.day;
    const onMove = (ev: MouseEvent) => {
      if (
        !moved &&
        Math.abs(ev.clientY - y0) < 5 &&
        Math.abs(ev.clientX - x0) < 5
      )
        return;
      if (!moved) {
        moved = true;
        document.body.classList.add("dragging-rows");
      }
      const pos = gridPos(ev.clientX, ev.clientY);
      lastStart = clampN(snapRound(pos.min - grabOffset), 0, DAY_MIN - dur);
      lastDay = pos.day;
      setGhost({
        kind: "move",
        id: b.id,
        day: lastDay,
        start: lastStart,
        end: lastStart + dur,
      });
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      document.body.classList.remove("dragging-rows");
      setGhost(null);
      if (!moved) {
        // 클릭 = 이름 편집 (연동 블록 제목은 할 일 내용을 미러하므로 편집 없음)
        if (b.todo_id == null) {
          setEditingId(b.id);
          setEditText(b.title);
        }
        return;
      }
      if (lastStart === b.start_min && days[lastDay] === b.date) return;
      void updateBlockTime(b.id, days[lastDay], lastStart, lastStart + dur)
        .then(onChanged)
        .catch((err) => onError(errMsg(err)));
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  // 아래 가장자리 드래그 = 끝 시각 리사이즈 (최소 15분, 날짜 고정)
  function onResizeDown(e: ReactMouseEvent, b: TimeBlock) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    document.body.classList.add("resizing-tt");
    const col = dayIdx.get(b.date) ?? 0;
    let lastEnd = b.end_min;
    const onMove = (ev: MouseEvent) => {
      lastEnd = clampN(
        snapRound(gridPos(ev.clientX, ev.clientY).min),
        b.start_min + MIN_DUR,
        DAY_MIN,
      );
      setGhost({
        kind: "resize",
        id: b.id,
        day: col,
        start: b.start_min,
        end: lastEnd,
      });
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      document.body.classList.remove("resizing-tt");
      setGhost(null);
      if (lastEnd === b.end_min) return;
      void updateBlockTime(b.id, b.date, b.start_min, lastEnd)
        .then(onChanged)
        .catch((err) => onError(errMsg(err)));
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  async function commitEdit(b: TimeBlock) {
    const title = editText.trim();
    setEditingId(null);
    if (title === b.title) return;
    try {
      await renameBlock(b.id, title);
      onChanged();
    } catch (err) {
      onError(errMsg(err));
    }
  }

  function removeBlock(b: TimeBlock) {
    void deleteBlock(b.id)
      .then(onChanged)
      .catch((err) => onError(errMsg(err)));
  }

  // 겹침 lane 은 날짜(컬럼)별로 독립 계산
  const laneById = new Map<number, { lane: number; lanes: number }>();
  if (isTimeGrid) {
    for (const d of days) {
      for (const item of layoutLanes(blocks.filter((b) => b.date === d))) {
        laneById.set(item.b.id, { lane: item.lane, lanes: item.lanes });
      }
    }
  }

  const plannedMin = blocks.reduce((s, b) => s + (b.end_min - b.start_min), 0);
  const plannedLabel =
    plannedMin > 0
      ? `계획 ${Math.floor(plannedMin / 60) > 0 ? `${Math.floor(plannedMin / 60)}시간` : ""}${
          plannedMin % 60 > 0 ? ` ${plannedMin % 60}분` : ""
        }`.trim()
      : null;

  // 월 아젠다: 날짜별 그룹 (blocks 는 date, start_min 순으로 로드됨)
  const agenda: [string, TimeBlock[]][] = [];
  if (!isTimeGrid) {
    for (const b of blocks) {
      const last = agenda[agenda.length - 1];
      if (last && last[0] === b.date) last[1].push(b);
      else agenda.push([b.date, [b]]);
    }
  }

  const blockTitle = (b: TimeBlock) =>
    b.todo_id != null
      ? (todoById.get(b.todo_id)?.content ?? "(삭제된 할 일)")
      : b.title;
  const blockDone = (b: TimeBlock) =>
    b.todo_id != null && todoById.get(b.todo_id)?.done === 1;

  return (
    <div className="day-tt">
      {/* 섹션 헤더 밴드 — 캘린더와의 구분(라벨 + 계획 합계 + 뷰 전환) */}
      <div className="day-tt-head">
        <span className="day-tt-label">타임테이블</span>
        <span className="spacer" />
        {plannedLabel && <span className="day-tt-plan">{plannedLabel}</span>}
        <div className="segmented day-tt-seg">
          {(Object.keys(VIEW_LABELS) as TtView[]).map((v) => (
            <button
              key={v}
              className={`tab ${view === v ? "active" : ""}`}
              onClick={() => onViewChange(v)}
            >
              {VIEW_LABELS[v]}
            </button>
          ))}
        </div>
      </div>

      {/* 주 뷰: 요일·날짜 헤더 (그리드 컬럼과 정렬, 클릭=날짜 선택) */}
      {view === "week" && (
        <div className="day-tt-days">
          <span className="tt-days-gutter" aria-hidden="true" />
          {days.map((d) => {
            const dd = parseLocalDate(d);
            const cls =
              d === today ? "today" : d === date ? "selected" : "";
            return (
              <button
                key={d}
                className={`tt-day-cell ${cls}`}
                onClick={() => onSelectDate(d)}
              >
                <span className="tt-day-dow">{WEEKDAYS_KO[dd.getDay()]}</span>
                <span className="tt-day-num">{dd.getDate()}</span>
              </button>
            );
          })}
        </div>
      )}

      {isTimeGrid ? (
        <div className="day-tt-scroll" ref={scrollRef}>
          <div className="day-tt-inner">
            <div className="day-tt-hours" aria-hidden="true">
              {Array.from({ length: 23 }, (_, i) => i + 1).map((h) => (
                <span
                  key={h}
                  className="tt-hlabel"
                  style={{ top: h * TT_HOUR_H }}
                >
                  {pad2(h)}:00
                </span>
              ))}
            </div>
            <div className="day-tt-grid" ref={gridRef} onMouseDown={onGridDown}>
              {days.length > 1 &&
                days.slice(1).map((d, i) => (
                  <div
                    key={d}
                    className="tt-vline"
                    style={{ left: `${(i + 1) * colW}%` }}
                  />
                ))}
              {blocks.map((b) => {
                const col = dayIdx.get(b.date);
                if (col == null) return null;
                // 드래그 중인 블록은 ghost 위치 + 컬럼 전체 폭으로 '들려서' 움직인다
                const g =
                  ghost && "id" in ghost && ghost.id === b.id ? ghost : null;
                const start = g ? g.start : b.start_min;
                const end = g ? g.end : b.end_min;
                const dCol = g ? g.day : col;
                const { lane, lanes } = laneById.get(b.id) ?? {
                  lane: 0,
                  lanes: 1,
                };
                const title = blockTitle(b);
                const done = blockDone(b);
                const running =
                  days[dCol] === today && !done && nowMin >= start && nowMin < end;
                const h = minToY(end - start);
                return (
                  <div
                    key={b.id}
                    className={`tt-block ${running ? "now" : ""} ${done ? "done" : ""} ${g ? "dragging" : ""} ${title ? "" : "untitled"}`}
                    style={{
                      top: minToY(start),
                      height: Math.max(h - 2, 12),
                      left: g
                        ? `${dCol * colW}%`
                        : `${col * colW + (lane / lanes) * colW}%`,
                      width: g
                        ? `calc(${colW}% - 2px)`
                        : `calc(${colW / lanes}% - 2px)`,
                    }}
                    onMouseDown={(e) => onBlockDown(e, b)}
                  >
                    {editingId === b.id ? (
                      <input
                        className="tt-edit"
                        autoFocus
                        placeholder="제목"
                        value={editText}
                        onChange={(e) => setEditText(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.nativeEvent.isComposing) return;
                          if (e.key === "Enter") void commitEdit(b);
                          if (e.key === "Escape") setEditingId(null);
                        }}
                        onBlur={() => void commitEdit(b)}
                      />
                    ) : (
                      <>
                        <span className="tt-title">
                          {title || "(제목 없음)"}
                        </span>
                        {h >= 34 && (
                          <span className="tt-time">
                            {fmtMin(start)} – {fmtMin(end)}
                          </span>
                        )}
                        <button
                          className="tt-del"
                          aria-label="블록 삭제"
                          onMouseDown={(e) => e.stopPropagation()}
                          onClick={() => removeBlock(b)}
                        >
                          <Icon name="trash" size={11} />
                        </button>
                      </>
                    )}
                    <div
                      className="tt-resize"
                      onMouseDown={(e) => onResizeDown(e, b)}
                    />
                  </div>
                );
              })}
              {ghost?.kind === "create" && (
                <div
                  className="tt-block tt-ghost"
                  style={{
                    top: minToY(ghost.start),
                    height: Math.max(minToY(ghost.end - ghost.start) - 2, 12),
                    left: `${ghost.day * colW}%`,
                    width: `calc(${colW}% - 2px)`,
                  }}
                >
                  <span className="tt-time">
                    {fmtMin(ghost.start)} – {fmtMin(ghost.end)}
                  </span>
                </div>
              )}
              {todayCol != null && (
                <div
                  className="tt-now"
                  style={{
                    top: minToY(nowMin),
                    left: `${todayCol * colW}%`,
                    width: `${colW}%`,
                  }}
                />
              )}
            </div>
          </div>
        </div>
      ) : (
        // 월 뷰: 아젠다 리스트 — 그 달의 블록을 날짜별로. 클릭=그 날짜 선택(우측 목록·일 뷰 연동)
        <div className="day-tt-agenda">
          {agenda.length === 0 ? (
            <div className="hint day-tt-empty">
              이번 달 계획이 없어요 — 일/주 뷰에서 드래그로 추가하세요.
            </div>
          ) : (
            agenda.map(([d, list]) => (
              <div key={d} className="tt-ag-day">
                <button
                  className={`tt-ag-date ${d === today ? "today" : ""}`}
                  onClick={() => onSelectDate(d)}
                >
                  {formatDayLong(d)}
                  {d === today && <span className="tt-ag-today">오늘</span>}
                </button>
                {list.map((b) => (
                  <div
                    key={b.id}
                    className={`tt-ag-row ${blockDone(b) ? "done" : ""}`}
                    onClick={() => onSelectDate(d)}
                  >
                    <span className="tt-ag-time">
                      {fmtMin(b.start_min)} – {fmtMin(b.end_min)}
                    </span>
                    <span className="tt-ag-title">
                      {blockTitle(b) || "(제목 없음)"}
                    </span>
                  </div>
                ))}
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}
