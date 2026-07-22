// 데이 타임테이블 — 할 일 탭 좌측 pane 하단, 선택 날짜의 시간 계획(구글 캘린더식 time-blocking).
// 시간 좌표는 "자정 기준 분"(로컬 벽시계) — UTC ms 재해석 금지(.claude/DESIGN.md §10).
// 인터랙션(§8 포인터 드래그 규약): 빈 곳 세로 드래그=생성(15분 스냅), 블록 드래그=이동(5px 임계,
// 그 이하는 클릭=인라인 이름 편집), 아래 가장자리=리사이즈. 드래그 중엔 ghost 상태만 갱신하고
// 놓을 때 DB 커밋 → onChanged 로 부모가 재로딩.
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
import { Icon } from "../icons";

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

// 드래그 중 화면에만 반영되는 임시 범위 (커밋은 mouseup 에서 한 번)
type Ghost =
  | { kind: "create"; start: number; end: number }
  | { kind: "move"; id: number; start: number; end: number }
  | { kind: "resize"; id: number; start: number; end: number };

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
  date,
  isToday,
  active,
  blocks,
  todos,
  focus,
  onChanged,
  onError,
}: {
  date: string;
  isToday: boolean;
  active: boolean;
  blocks: TimeBlock[];
  todos: Todo[]; // 연동 블록 표시용(내용·완료 미러)
  focus: { min: number; nonce: number } | null; // "시간표에 넣기" 후 스크롤 타깃
  onChanged: () => void;
  onError: (msg: string) => void;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<HTMLDivElement>(null);
  const [ghost, setGhost] = useState<Ghost | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editText, setEditText] = useState("");

  const todoById = new Map(todos.map((t) => [t.id, t]));

  // '지금' 분 — 오늘 + 탭 활성일 때만 30초마다 갱신 (라인·진행 중 필 표시)
  const [nowMin, setNowMin] = useState(nowMinute);
  useEffect(() => {
    if (!active || !isToday) return;
    setNowMin(nowMinute());
    const t = setInterval(() => setNowMin(nowMinute()), 30_000);
    return () => clearInterval(t);
  }, [active, isToday]);

  // 날짜가 바뀌면 자동 스크롤: 오늘=지금−1h, 다른 날=09:00 (같은 날 재로딩엔 유지)
  const scrolledFor = useRef<string | null>(null);
  useEffect(() => {
    if (!active) return;
    if (scrolledFor.current === date) return;
    scrolledFor.current = date;
    const el = scrollRef.current;
    if (!el) return;
    const target = isToday ? Math.max(nowMinute() - 60, 0) : 9 * 60;
    el.scrollTop = minToY(target);
  }, [active, date, isToday]);

  // "시간표에 넣기"로 만든 블록이 화면 밖이면 그리로 스크롤
  useEffect(() => {
    if (!focus) return;
    const el = scrollRef.current;
    if (!el) return;
    const top = minToY(Math.max(focus.min - 30, 0));
    const bottom = top + minToY(90);
    if (top < el.scrollTop || bottom > el.scrollTop + el.clientHeight) {
      el.scrollTop = top;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focus?.nonce]);

  const gridMin = (clientY: number) => {
    const rect = gridRef.current!.getBoundingClientRect();
    return yToMin(clientY - rect.top);
  };

  // 빈 곳 세로 드래그 = 생성. 클릭만으론 만들지 않는다(오조작 방지) — 4px 임계.
  function onGridDown(e: ReactMouseEvent) {
    if (e.button !== 0 || editingId != null) return;
    const target = e.target as HTMLElement;
    if (target.closest(".tt-block")) return;
    e.preventDefault(); // 텍스트 선택 방지(§8)
    const y0 = e.clientY;
    const anchor = clampN(snapFloor(gridMin(y0)), 0, DAY_MIN - SNAP);
    let moved = false;
    const calc = (clientY: number) => {
      const cur = clampN(snapFloor(gridMin(clientY)), 0, DAY_MIN - SNAP);
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
      setGhost({ kind: "create", start, end });
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
          const id = await createBlock(date, start, end, "");
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

  // 블록 몸통 드래그 = 이동(5px 임계). 임계 미만 = 클릭 → 인라인 이름 편집(연동 블록 제외).
  function onBlockDown(e: ReactMouseEvent, b: TimeBlock) {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest(".tt-resize") || target.closest(".tt-del")) return;
    if (target.closest("input")) return; // 편집 중인 입력은 드래그로 뺏지 않는다
    e.preventDefault();
    e.stopPropagation();
    const dur = b.end_min - b.start_min;
    const grabOffset = gridMin(e.clientY) - b.start_min;
    const y0 = e.clientY;
    let moved = false;
    let lastStart = b.start_min;
    const onMove = (ev: MouseEvent) => {
      if (!moved && Math.abs(ev.clientY - y0) < 5) return;
      if (!moved) {
        moved = true;
        document.body.classList.add("dragging-rows");
      }
      lastStart = clampN(
        snapRound(gridMin(ev.clientY) - grabOffset),
        0,
        DAY_MIN - dur,
      );
      setGhost({ kind: "move", id: b.id, start: lastStart, end: lastStart + dur });
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
      if (lastStart === b.start_min) return;
      void updateBlockTime(b.id, lastStart, lastStart + dur)
        .then(onChanged)
        .catch((err) => onError(errMsg(err)));
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  // 아래 가장자리 드래그 = 끝 시각 리사이즈 (최소 15분)
  function onResizeDown(e: ReactMouseEvent, b: TimeBlock) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    document.body.classList.add("resizing-tt");
    let lastEnd = b.end_min;
    const onMove = (ev: MouseEvent) => {
      lastEnd = clampN(
        snapRound(gridMin(ev.clientY)),
        b.start_min + MIN_DUR,
        DAY_MIN,
      );
      setGhost({ kind: "resize", id: b.id, start: b.start_min, end: lastEnd });
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      document.body.classList.remove("resizing-tt");
      setGhost(null);
      if (lastEnd === b.end_min) return;
      void updateBlockTime(b.id, b.start_min, lastEnd)
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

  const laid = layoutLanes(blocks);
  const plannedMin = blocks.reduce((s, b) => s + (b.end_min - b.start_min), 0);
  const plannedLabel =
    plannedMin > 0
      ? `계획 ${Math.floor(plannedMin / 60) > 0 ? `${Math.floor(plannedMin / 60)}시간` : ""}${
          plannedMin % 60 > 0 ? ` ${plannedMin % 60}분` : ""
        }`.trim()
      : null;

  return (
    <div className="day-tt">
      {/* 섹션 헤더 밴드 — 캘린더와 시간축의 구분선 역할(라벨 + 계획 합계) */}
      <div className="day-tt-head">
        <span className="day-tt-label">타임테이블</span>
        {plannedLabel && <span className="day-tt-plan">{plannedLabel}</span>}
      </div>
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
            {laid.map(({ b, lane, lanes }) => {
              // 드래그 중인 블록은 ghost 범위 + 전체 폭으로 '들려서' 움직인다
              const g = ghost && "id" in ghost && ghost.id === b.id ? ghost : null;
              const start = g ? g.start : b.start_min;
              const end = g ? g.end : b.end_min;
              const todo = b.todo_id != null ? todoById.get(b.todo_id) : undefined;
              const title =
                b.todo_id != null
                  ? (todo?.content ?? "(삭제된 할 일)")
                  : b.title;
              const done = todo?.done === 1;
              const running = isToday && !done && nowMin >= start && nowMin < end;
              const h = minToY(end - start);
              return (
                <div
                  key={b.id}
                  className={`tt-block ${running ? "now" : ""} ${done ? "done" : ""} ${g ? "dragging" : ""} ${title ? "" : "untitled"}`}
                  style={{
                    top: minToY(start),
                    height: Math.max(h - 2, 12),
                    left: g ? 0 : `${(lane / lanes) * 100}%`,
                    width: g ? "100%" : `calc(${100 / lanes}% - 2px)`,
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
                      <span className="tt-title">{title || "(제목 없음)"}</span>
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
                  left: 0,
                  width: "100%",
                }}
              >
                <span className="tt-time">
                  {fmtMin(ghost.start)} – {fmtMin(ghost.end)}
                </span>
              </div>
            )}
            {isToday && (
              <div className="tt-now" style={{ top: minToY(nowMin) }} />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
