// 할 일 섹션: 좌측 미니 캘린더 + 우측 선택 날짜 체크리스트 (2-pane, .claude/DESIGN.md §7).
// 오늘 우선 — 탭을 열면 오늘 + 빠른 추가 입력에 포커스. 정본은 til.db 의 todos 테이블(lib/todos.ts).

import {
  Fragment,
  useCallback,
  useEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
} from "react";
import type { DayTodoCount, TimeBlock, Todo } from "../types";
import {
  createTodo,
  deleteTodo,
  listMonthCounts,
  listOverdueOpen,
  listTodos,
  moveTodos,
  recomputeChainFrom,
  reorderTodos,
  reparentTodo,
  setSubtreeDone,
  toggleTodo,
  updateTodoContent,
} from "../lib/todos";
import {
  createBlock,
  findFreeSlot,
  listBlocks,
  nowMinute,
} from "../lib/timeBlocks";
import { conceptsLearnedOn } from "../lib/db";
import {
  addMonths,
  dayRangeMs,
  formatDayLong,
  formatDayShort,
  localDateStr,
  monthGridDates,
  monthOf,
  parseLocalDate,
  shiftDay,
  todayStr,
  weekStartOf,
} from "../lib/date";
import { Checkbox } from "../ui";
import { Icon } from "../icons";
import { MiniCalendar } from "./MiniCalendar";
import { DayTimetable, type TtView } from "./DayTimetable";
import { DailyReportPanel } from "./DailyReportPanel";
import { openConceptInApp } from "../lib/nav";
import type { AppConfig } from "../lib/config";

const errMsg = (e: unknown) => (e instanceof Error ? e.message : String(e));

const OVERDUE_LIMIT = 20;
// 중첩 단계별 들여쓰기(px). 유닛 marginLeft 로 겹쳐 적용돼 단계마다 이만큼 더 들어간다.
const INDENT = 24;

// 타임테이블 뷰 모드 (일/주/월, localStorage 영속)
const TT_VIEW_KEY = "amber.todo.tt-view";

/** 뷰별 블록 로드 범위 [from, to] — 일=선택일, 주=일~토, 월=그 달 1일~말일 */
function ttRange(view: TtView, selected: string): [string, string] {
  if (view === "week") {
    const start = weekStartOf(selected);
    return [start, shiftDay(start, 6)];
  }
  if (view === "month") {
    const d = parseLocalDate(selected);
    return [
      localDateStr(new Date(d.getFullYear(), d.getMonth(), 1)),
      localDateStr(new Date(d.getFullYear(), d.getMonth() + 1, 0)),
    ];
  }
  return [selected, selected];
}

// 캘린더 pane 너비(드래그 조절, localStorage 영속). 가로 비중은 사용자가 직접 정한다.
const CAL_W_KEY = "amber.todo.cal-width";
const CAL_W_MIN = 260; // 캘린더 최소 폭 (셀 안 날짜 원이 들어가는 하한)
const CAL_W_MAX = 780;
const CAL_W_DEFAULT = 460;
// 오른쪽 체크리스트가 항상 확보할 최소 폭 — 창이 좁아지면 캘린더가 이만큼 양보(반응형)
const DETAIL_MIN = 340;

export function TodoView({
  active,
  config,
  onOpenSettings,
}: {
  active: boolean;
  config: AppConfig | null;
  onOpenSettings: () => void;
}) {
  const [selected, setSelected] = useState(() => todayStr());
  const [cursor, setCursor] = useState(() => monthOf(todayStr()));

  const [todos, setTodos] = useState<Todo[]>([]);
  const [blocks, setBlocks] = useState<TimeBlock[]>([]);
  const [ttView, setTtView] = useState<TtView>(() => {
    const s = localStorage.getItem(TT_VIEW_KEY);
    return s === "week" || s === "month" ? s : "day";
  });
  useEffect(() => {
    localStorage.setItem(TT_VIEW_KEY, ttView);
  }, [ttView]);
  const [ttFocus, setTtFocus] = useState<{ min: number; nonce: number } | null>(
    null,
  );
  const [counts, setCounts] = useState<Record<string, DayTodoCount>>({});
  const [overdue, setOverdue] = useState<Todo[]>([]);
  const [overdueOpen, setOverdueOpen] = useState(false);
  const [learned, setLearned] = useState<{ id: number; title: string }[]>([]);

  const [quick, setQuick] = useState("");
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editText, setEditText] = useState("");
  const [addingChildFor, setAddingChildFor] = useState<number | null>(null);
  const [childInput, setChildInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dragId, setDragId] = useState<number | null>(null);

  const quickRef = useRef<HTMLInputElement>(null);
  const childInputRef = useRef<HTMLInputElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const todosRef = useRef(todos);
  todosRef.current = todos;

  const [calWidth, setCalWidth] = useState(() => {
    const s = Number(localStorage.getItem(CAL_W_KEY));
    return s >= CAL_W_MIN && s <= CAL_W_MAX ? s : CAL_W_DEFAULT;
  });
  useEffect(() => {
    localStorage.setItem(CAL_W_KEY, String(Math.round(calWidth)));
  }, [calWidth]);

  // 반응형: .body 실측 폭을 추적. calWidth 는 '희망 폭'으로 두고, 실제 렌더 폭(calWEff)은
  // 창이 좁으면 줄고 넓어지면 다시 커진다 — 체크리스트는 항상 DETAIL_MIN 이상 확보.
  const [bodyW, setBodyW] = useState(0);
  useEffect(() => {
    if (!active) return;
    const measure = () => setBodyW(bodyRef.current?.clientWidth ?? 0);
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [active]);

  function startResize(e: ReactMouseEvent) {
    e.preventDefault();
    const rect = bodyRef.current?.getBoundingClientRect();
    if (!rect) return;
    document.body.classList.add("resizing-col");
    const onMove = (ev: MouseEvent) => {
      const maxCal = Math.min(CAL_W_MAX, rect.width - DETAIL_MIN);
      setCalWidth(Math.max(CAL_W_MIN, Math.min(maxCal, ev.clientX - rect.left)));
    };
    const onUp = () => {
      document.body.classList.remove("resizing-col");
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  // 할 일 트리 드래그 (포인터 기반 — WKWebView 에서 HTML5 DnD 보다 안정적).
  // Todoist/Notion 문법: **세로 = 삽입 위치, 가로 = 깊이(들여쓰기)**. 한 제스처로 재정렬과
  // 부모 변경(하위로 넣기·꺼내기·다른 부모로)을 모두 처리한다.
  // 파일 트리 규약(§8) 재사용: 원본(서브트리째)은 자리에 흐리게, 행 복제 오버레이가 커서를
  // 1:1 로 따라오고, 깊이만큼 들여쓴 2px 삽입선이 떨어질 곳을 보여준다. 커밋은 놓을 때 한 번.
  function startDrag(e: ReactMouseEvent, id: number) {
    e.preventDefault();
    const listEl = listRef.current;
    if (!listEl) return;
    const cur = todosRef.current;
    const node = cur.find((t) => t.id === id);
    if (!node) return;

    // 렌더와 동일한 순서의 플랫 행 목록 (depth 포함)
    type RowMeta = { id: number; parent_id: number | null; depth: number };
    const rows: RowMeta[] = [];
    const walk = (parent: number | null, depth: number) => {
      for (const t of cur.filter((x) => (x.parent_id ?? null) === parent)) {
        rows.push({ id: t.id, parent_id: t.parent_id ?? null, depth });
        walk(t.id, depth + 1);
      }
    };
    walk(null, 0);
    const srcDepth = rows.find((r) => r.id === id)?.depth ?? 0;

    // 드래그 서브트리 — 자기 자신/자손 안으로는 못 들어간다 (후보에서 제외 = 순환 방지)
    const subtree = new Set<number>([id]);
    for (const r of rows) if (r.parent_id != null && subtree.has(r.parent_id)) subtree.add(r.id);

    // DOM 실측 (메인 목록의 실제 행만 — 문서 순서 = rows 순서)
    const rects = new Map<number, DOMRect>();
    for (const el of Array.from(
      listEl.querySelectorAll<HTMLElement>(".todo-row[data-todo-id]"),
    )) {
      rects.set(Number(el.dataset.todoId), el.getBoundingClientRect());
    }
    const srcRect = rects.get(id);
    if (!srcRect) return;
    // 삽입 후보 = 서브트리 제외 행들 (세로 순서 유지)
    const others = rows.filter((r) => !subtree.has(r.id) && rects.has(r.id));
    const listRect = listEl.getBoundingClientRect();

    const startX = e.clientX;
    const startY = e.clientY;
    const grabX = e.clientX - srcRect.left;
    const grabY = e.clientY - srcRect.top;
    let moved = false;
    let overlay: HTMLDivElement | null = null;
    let line: HTMLDivElement | null = null;
    let slot: { idx: number; depth: number } | null = null;

    const beginVisuals = () => {
      document.body.classList.add("dragging-rows");
      setDragId(id); // 원본(서브트리째) 흐림 — renderUnit 이 .dragging 부여
      // 커서를 따라오는 행 복제 오버레이 (본문 + 자손 수 칩)
      overlay = document.createElement("div");
      overlay.className = "todo-drag-overlay";
      overlay.style.width = `${Math.min(srcRect.width, 420)}px`;
      const label = document.createElement("span");
      label.className = "todo-drag-label";
      label.textContent = node.content;
      overlay.appendChild(label);
      const descCount = subtree.size - 1;
      if (descCount > 0) {
        const chip = document.createElement("span");
        chip.className = "todo-drag-count";
        chip.textContent = `+${descCount}`;
        overlay.appendChild(chip);
      }
      document.body.appendChild(overlay);
      line = document.createElement("div");
      line.className = "todo-drop-line";
      listEl.appendChild(line);
    };

    const apply = (clientX: number, clientY: number) => {
      if (!overlay || !line) return;
      overlay.style.transform = `translate(${clientX - grabX}px, ${clientY - grabY}px)`;
      // 삽입 index: 커서 Y 가 중심을 지난 행 수 (others 는 세로 정렬 상태)
      let idx = 0;
      for (const r of others) {
        const rc = rects.get(r.id)!;
        if (clientY > rc.top + rc.height / 2) idx++;
        else break;
      }
      const above = others[idx - 1] ?? null;
      const below = others[idx] ?? null;
      // 유효 깊이: 아래 행의 깊이 이상(그 행의 부모를 뺏지 않게), 위 행의 깊이+1 이하(그 행의 하위까지)
      const maxD = above ? above.depth + 1 : 0;
      const minD = below ? below.depth : 0;
      const desired = srcDepth + Math.round((clientX - startX) / INDENT);
      const depth = Math.max(minD, Math.min(maxD, desired));
      slot = { idx, depth };
      // 삽입선: 슬롯 경계 Y + 깊이만큼 들여쓴 왼쪽 (listing 좌표계)
      const y = above
        ? rects.get(above.id)!.bottom
        : below
          ? rects.get(below.id)!.top
          : listRect.top + 2;
      line.style.top = `${y - listRect.top - 1}px`;
      line.style.left = `${10 + depth * INDENT}px`;
      line.style.right = "8px";
    };

    const onMove = (ev: MouseEvent) => {
      if (!moved) {
        if (
          Math.abs(ev.clientY - startY) < 5 &&
          Math.abs(ev.clientX - startX) < 5
        )
          return;
        moved = true;
        beginVisuals();
      }
      apply(ev.clientX, ev.clientY);
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      document.body.classList.remove("dragging-rows");
      overlay?.remove();
      line?.remove();
      setDragId(null);
      if (!moved || !slot) return;

      const { idx, depth } = slot;
      // 새 부모 = 슬롯 위쪽에서 가장 가까운 depth-1 행 (depth 0 이면 최상위)
      let newParent: number | null = null;
      if (depth > 0) {
        for (let i = idx - 1; i >= 0; i--) {
          if (others[i].depth === depth - 1) {
            newParent = others[i].id;
            break;
          }
          if (others[i].depth < depth - 1) break;
        }
        if (newParent == null) return; // 방어 — 유효 깊이 규칙상 오지 않는 경로
      }
      // 새 형제 순서: 슬롯 이전에 등장한 같은 부모의 행 수 = 삽입 위치
      const beforeCnt = others
        .slice(0, idx)
        .filter((r) => r.parent_id === newParent).length;
      const groupIds = cur
        .filter((t) => (t.parent_id ?? null) === newParent && t.id !== id)
        .map((t) => t.id);
      groupIds.splice(beforeCnt, 0, id);

      const oldParent = node.parent_id ?? null;
      if (newParent === oldParent) {
        const oldOrder = cur
          .filter((t) => (t.parent_id ?? null) === oldParent)
          .map((t) => t.id);
        if (oldOrder.join(",") === groupIds.join(",")) return; // 제자리 — 커밋 생략
      }
      void (async () => {
        try {
          if (newParent !== oldParent) await reparentTodo(id, newParent);
          await reorderTodos(groupIds);
          // 완료 상태 재계산: 떠난 그룹(마지막 미완료가 빠졌을 수 있음) + 새 그룹
          await recomputeChainFrom(oldParent);
          if (newParent !== oldParent) await recomputeChainFrom(newParent);
          await reloadDay();
          void reloadCounts();
        } catch (err) {
          setError(errMsg(err));
        }
      })();
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  // 선택 날짜의 목록 + 밀린 할 일 + 이날 학습완료 개념
  const reloadDay = useCallback(async () => {
    try {
      const today = todayStr();
      const rows = await listTodos(selected);
      setTodos(rows);
      const [ttFrom, ttTo] = ttRange(ttView, selected);
      setBlocks(await listBlocks(ttFrom, ttTo));
      setOverdue(selected === today ? await listOverdueOpen(today) : []);
      const [start, end] = dayRangeMs(selected);
      setLearned(await conceptsLearnedOn(start, end));
      setError(null);
    } catch (e) {
      setError(errMsg(e));
    }
  }, [selected, ttView]);

  // 표시 중인 달 그리드 범위의 날짜별 개수 (캘린더 점·월 요약)
  const reloadCounts = useCallback(async () => {
    try {
      const dates = monthGridDates(cursor.year, cursor.month);
      const rows = await listMonthCounts(dates[0], dates[dates.length - 1]);
      const map: Record<string, DayTodoCount> = {};
      for (const r of rows) map[r.due_date] = r;
      setCounts(map);
    } catch (e) {
      setError(errMsg(e));
    }
  }, [cursor]);

  useEffect(() => {
    if (active) void reloadDay();
  }, [active, reloadDay]);
  useEffect(() => {
    if (active) void reloadCounts();
  }, [active, reloadCounts]);

  // 탭 진입 시 빠른 추가 입력에 포커스 (마찰 0)
  useEffect(() => {
    if (active) quickRef.current?.focus();
  }, [active]);

  // 창 포커스 복귀 시 갱신(다른 창 변경·자정 넘김 흡수). stale closure 방지용 ref.
  const activeRef = useRef(active);
  activeRef.current = active;
  const reloadDayRef = useRef(reloadDay);
  reloadDayRef.current = reloadDay;
  const reloadCountsRef = useRef(reloadCounts);
  reloadCountsRef.current = reloadCounts;
  useEffect(() => {
    const onFocus = () => {
      if (!activeRef.current) return;
      void reloadDayRef.current();
      void reloadCountsRef.current();
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  const refreshAll = () => {
    void reloadDay();
    void reloadCounts();
  };

  const goDate = (date: string) => {
    setSelected(date);
    setCursor(monthOf(date));
    setEditingId(null);
  };

  async function add() {
    const content = quick.trim();
    if (!content || busy) return;
    setBusy(true);
    try {
      await createTodo(content, selected);
      setQuick("");
      refreshAll();
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
      quickRef.current?.focus();
    }
  }

  // 메인 목록 토글 — 다단계 완료 전파. 어떤 노드든 그 서브트리 전체를 같은 상태로 하향 전파하고,
  // 위로는 조상 체인의 완료 상태를 재계산(자식이 모두 완료면 부모도 완료).
  // 전파 결과를 정확히 반영하려고 토글 후 reloadDay (정렬은 sort_order 고정이라 행 안 튐).
  async function toggle(t: Todo) {
    const next: 0 | 1 = t.done === 1 ? 0 : 1;
    try {
      await setSubtreeDone(t.id, next);
      await recomputeChainFrom(t.parent_id);
      await reloadDay();
      void reloadCounts();
    } catch (e) {
      setError(errMsg(e));
    }
  }

  // 밀린 스트립 항목 토글 — 완료하면 스트립에서 빠지므로 전체 갱신
  async function toggleOverdue(t: Todo) {
    try {
      await toggleTodo(t.id, t.done === 1 ? 0 : 1);
      refreshAll();
    } catch (e) {
      setError(errMsg(e));
    }
  }

  function startEdit(t: Todo) {
    setEditingId(t.id);
    setEditText(t.content);
  }

  async function saveEdit(t: Todo) {
    const content = editText.trim();
    setEditingId(null);
    if (!content || content === t.content) return; // 빈 값·무변경은 취소로 처리
    try {
      await updateTodoContent(t.id, content);
      void reloadDay();
    } catch (e) {
      setError(errMsg(e));
    }
  }

  async function remove(t: Todo) {
    try {
      await deleteTodo(t.id);
      // 서브트리를 지웠으니 부모 완료 상태 재계산(마지막 미완료 자식이 사라졌을 수 있음)
      await recomputeChainFrom(t.parent_id);
      refreshAll();
    } catch (e) {
      setError(errMsg(e));
    }
  }

  async function moveToday(ids: number[]) {
    if (!ids.length) return;
    try {
      await moveTodos(ids, todayStr());
      refreshAll();
    } catch (e) {
      setError(errMsg(e));
    }
  }

  // 할 일을 타임테이블에 배치 — 다음 빈 슬롯에 1시간 블록 생성 후 그리로 스크롤.
  // 오늘이면 지금 이후(15분 올림), 다른 날이면 09:00 부터 빈 자리를 찾는다.
  async function scheduleTodo(t: Todo) {
    try {
      const from =
        selected === todayStr()
          ? Math.min(Math.ceil(nowMinute() / 15) * 15, 1440 - 60)
          : 9 * 60;
      const start = findFreeSlot(
        blocks.filter((b) => b.date === selected),
        from,
        60,
      );
      await createBlock(selected, start, start + 60, "", t.id);
      setTtFocus({ min: start, nonce: Date.now() });
      await reloadDay();
    } catch (e) {
      setError(errMsg(e));
    }
  }

  // 하위 항목 추가 (부모 hover '+하위'). 추가 후 부모 완료 재계산 + 입력 유지(연속 추가)
  async function addChild(parentId: number) {
    const content = childInput.trim();
    if (!content || busy) return;
    setBusy(true);
    try {
      await createTodo(content, selected, parentId);
      await recomputeChainFrom(parentId);
      setChildInput("");
      await reloadDay();
      void reloadCounts();
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
      childInputRef.current?.focus();
    }
  }

  const today = todayStr();
  const isToday = selected === today;
  // 렌더용 캘린더 폭: 희망 폭(calWidth)을 창 폭에 맞춰 clamp (오른쪽 최소 DETAIL_MIN 보장)
  const calWEff = bodyW
    ? Math.max(CAL_W_MIN, Math.min(calWidth, bodyW - DETAIL_MIN))
    : calWidth;
  const topLevel = todos.filter((t) => t.parent_id == null);
  const childrenOf = (pid: number) => todos.filter((t) => t.parent_id === pid);
  const doneTop = topLevel.filter((t) => t.done === 1).length;

  function renderRow(t: Todo, opts?: { overdue?: boolean }) {
    const isOverdue = opts?.overdue ?? false;
    const kids = isOverdue ? [] : childrenOf(t.id);
    const kidsDone = kids.filter((k) => k.done === 1).length;
    return (
      <div
        className={`todo-row ${t.done === 1 ? "done" : ""}`}
        data-todo-id={t.id}
      >
        {!isOverdue && (
          <span
            className="todo-grip"
            title="드래그해서 이동 · 가로로 깊이 조절"
            aria-hidden="true"
            onMouseDown={(e) => startDrag(e, t.id)}
          >
            <Icon name="grip" size={14} />
          </span>
        )}
        <Checkbox
          checked={t.done === 1}
          onChange={() => (isOverdue ? toggleOverdue(t) : toggle(t))}
          label={t.content}
        />
        {editingId === t.id ? (
          <input
            className="input todo-edit"
            autoFocus
            value={editText}
            onChange={(e) => setEditText(e.target.value)}
            onKeyDown={(e) => {
              if (e.nativeEvent.isComposing) return;
              if (e.key === "Enter") void saveEdit(t);
              if (e.key === "Escape") setEditingId(null);
            }}
            onBlur={() => setEditingId(null)}
          />
        ) : (
          <span
            className="todo-text"
            onClick={() => (isOverdue ? toggleOverdue(t) : startEdit(t))}
          >
            {t.content}
          </span>
        )}

        {kids.length > 0 && (
          <span className="todo-progress" title="완료 하위 / 전체">
            {kidsDone}/{kids.length}
          </span>
        )}

        {isOverdue ? (
          // 밀린 스트립: 원래 날짜 + 가져오기/버리기 — 발견성 위해 항상 표시(hover 오버레이 아님)
          <span className="todo-overdue-actions">
            <span className="todo-row-date">{formatDayShort(t.due_date)}</span>
            <button className="btn btn-sm" onClick={() => void moveToday([t.id])}>
              오늘로
            </button>
            <button
              className="icon-btn sm danger"
              title="삭제"
              onClick={() => void remove(t)}
            >
              <Icon name="trash" size={13} />
            </button>
          </span>
        ) : (
          // 메인 목록: 하위추가/편집/삭제는 hover 오버레이(레이아웃을 밀지 않음)
          <span className="row-actions" onClick={(e) => e.stopPropagation()}>
            <button
              className="icon-btn sm"
              title="하위 추가"
              onClick={() => {
                setAddingChildFor(t.id);
                setChildInput("");
              }}
            >
              <Icon name="plus" size={13} />
            </button>
            <button
              className="icon-btn sm"
              title="시간표에 넣기"
              onClick={() => void scheduleTodo(t)}
            >
              <Icon name="clock" size={13} />
            </button>
            <button
              className="icon-btn sm"
              title="이름 변경"
              onClick={() => startEdit(t)}
            >
              <Icon name="pencil" size={13} />
            </button>
            <button
              className="icon-btn sm danger"
              title="삭제"
              onClick={() => void remove(t)}
            >
              <Icon name="trash" size={13} />
            </button>
          </span>
        )}
      </div>
    );
  }

  // 항목 유닛(항목 + 그 서브트리) 재귀 렌더. 각 유닛은 data-parent-id 로 형제 그룹을 표시(드래그용),
  // depth>0 이면 marginLeft 로 들여쓴다(중첩이 겹쳐 단계마다 더 들어간다).
  function renderUnit(node: Todo, depth: number) {
    return (
      <div
        key={node.id}
        className={`todo-unit ${dragId === node.id ? "dragging" : ""}`}
        data-unit-id={node.id}
        data-parent-id={node.parent_id == null ? "root" : String(node.parent_id)}
        style={depth > 0 ? { marginLeft: INDENT } : undefined}
      >
        {renderRow(node)}
        {childrenOf(node.id).map((c) => renderUnit(c, depth + 1))}
        {addingChildFor === node.id && (
          <div className="todo-row todo-subadd" style={{ marginLeft: INDENT }}>
            <input
              ref={childInputRef}
              className="input todo-edit"
              autoFocus
              placeholder="하위 항목 — Enter 로 추가"
              value={childInput}
              onChange={(e) => setChildInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.nativeEvent.isComposing) return;
                if (e.key === "Enter") void addChild(node.id);
                if (e.key === "Escape") {
                  setAddingChildFor(null);
                  setChildInput("");
                }
              }}
              onBlur={() => {
                setAddingChildFor(null);
                setChildInput("");
              }}
            />
          </div>
        )}
      </div>
    );
  }

  return (
    <div
      className="body todo-body"
      ref={bodyRef}
      style={{ gridTemplateColumns: `${calWEff}px 1fr` }}
    >
      <aside className="list todo-cal-pane">
        <MiniCalendar
          year={cursor.year}
          month={cursor.month}
          selected={selected}
          today={today}
          counts={counts}
          onSelect={goDate}
          onPrevMonth={() => setCursor((c) => addMonths(c, -1))}
          onNextMonth={() => setCursor((c) => addMonths(c, 1))}
          onToday={() => goDate(today)}
        />
        <DayTimetable
          view={ttView}
          onViewChange={setTtView}
          date={selected}
          days={
            ttView === "week"
              ? Array.from({ length: 7 }, (_, i) =>
                  shiftDay(weekStartOf(selected), i),
                )
              : [selected]
          }
          today={today}
          active={active}
          blocks={blocks}
          todos={todos}
          focus={ttFocus}
          onChanged={() => void reloadDay()}
          onError={setError}
          onSelectDate={goDate}
        />
      </aside>

      <div
        className="todo-resizer"
        style={{ left: calWEff }}
        onMouseDown={startResize}
        title="드래그해서 캘린더 너비 조절"
      />

      <section className="detail">
        <div className="detail-head todo-head">
          <h1 className="detail-title">{formatDayLong(selected)}</h1>
          <span className="spacer" />
          {/* 캘린더 앱 표준: [오늘] ‹ › — 오늘이면 '오늘' 버튼 비활성(이미 오늘임을 표시) */}
          <div className="todo-nav">
            <button
              className="btn btn-sm"
              onClick={() => goDate(today)}
              disabled={isToday}
            >
              오늘
            </button>
            <button
              className="icon-btn ghost"
              title="전날"
              onClick={() => goDate(shiftDay(selected, -1))}
            >
              <Icon name="chevron-left" size={16} />
            </button>
            <button
              className="icon-btn ghost"
              title="다음날"
              onClick={() => goDate(shiftDay(selected, 1))}
            >
              <Icon name="chevron-right" size={16} />
            </button>
          </div>
        </div>

        {error && <div className="error-note">{error}</div>}

        <div className="todo-quick">
          <span className="todo-quick-ico">
            <Icon name="plus" size={15} />
          </span>
          <input
            ref={quickRef}
            className="input todo-quick-input"
            placeholder="할 일을 적고 Enter"
            value={quick}
            onChange={(e) => setQuick(e.target.value)}
            onKeyDown={(e) => {
              if (e.nativeEvent.isComposing) return;
              if (e.key === "Enter") void add();
            }}
          />
        </div>

        {isToday && overdue.length > 0 && (
          <div className={`todo-overdue ${overdueOpen ? "open" : ""}`}>
            <div
              className="todo-overdue-head"
              onClick={() => setOverdueOpen((v) => !v)}
            >
              <span className="todo-overdue-caret">
                <Icon name="chevron-right" size={14} />
              </span>
              <b>밀린 할 일</b>
              <span className="todo-overdue-cnt">{overdue.length}</span>
              <span className="spacer" />
              <button
                className="btn btn-sm"
                onClick={(e) => {
                  e.stopPropagation();
                  void moveToday(overdue.map((t) => t.id));
                }}
              >
                모두 오늘로 가져오기
              </button>
            </div>
            <div className="todo-overdue-body">
              <div className="todo-overdue-inner">
                {overdue.slice(0, OVERDUE_LIMIT).map((t) => (
                  <Fragment key={t.id}>{renderRow(t, { overdue: true })}</Fragment>
                ))}
                {overdue.length > OVERDUE_LIMIT && (
                  <div className="hint todo-overdue-more">
                    외 {overdue.length - OVERDUE_LIMIT}개
                  </div>
                )}
              </div>
            </div>
          </div>
        )}

        <div className="todo-listing" ref={listRef}>
          {todos.length === 0 ? (
            <div className="hint todo-day-empty">
              이 날의 할 일이 없어요 — 위 입력창에 적고 Enter.
            </div>
          ) : (
            topLevel.map((p) => renderUnit(p, 0))
          )}
        </div>

        {topLevel.length > 0 && (
          <div className="detail-meta">
            {topLevel.length}개 중 {doneTop}개 완료
          </div>
        )}

        {/* key={selected} — 날짜별로 패널을 격리. 생성 중 다른 날짜로 넘어가도 로딩/스트리밍 상태가
            새 날짜로 새지 않는다(진행 중 생성은 백그라운드에서 계속 저장됨) */}
        <DailyReportPanel
          key={selected}
          date={selected}
          config={config}
          active={active}
          onOpenSettings={onOpenSettings}
        />

        {learned.length > 0 && (
          <div className="todo-learned">
            <div className="todo-learned-label">이날 학습완료 {learned.length}</div>
            <div className="todo-learned-chips">
              {learned.map((c) => (
                <button
                  key={c.id}
                  className="chip btn-like"
                  title="개념 열기"
                  onClick={() => openConceptInApp(c.id)}
                >
                  {c.title}
                </button>
              ))}
            </div>
          </div>
        )}
      </section>
    </div>
  );
}
