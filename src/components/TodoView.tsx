// 할 일 섹션: 좌측 미니 캘린더 + 우측 선택 날짜 체크리스트 (2-pane, docs/DESIGN.md §7).
// 오늘 우선 — 탭을 열면 오늘 + 빠른 추가 입력에 포커스. 정본은 til.db 의 todos 테이블(lib/todos.ts).

import {
  Fragment,
  useCallback,
  useEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
} from "react";
import type { DayTodoCount, Todo } from "../types";
import {
  createTodo,
  deleteTodo,
  listMonthCounts,
  listOverdueOpen,
  listTodos,
  moveTodos,
  reorderTodos,
  toggleTodo,
  updateTodoContent,
} from "../lib/todos";
import { conceptsLearnedOn } from "../lib/db";
import {
  addMonths,
  dayRangeMs,
  formatDayLong,
  formatDayShort,
  monthGridDates,
  monthOf,
  shiftDay,
  todayStr,
} from "../lib/date";
import { Checkbox } from "../ui";
import { Icon } from "../icons";
import { MiniCalendar } from "./MiniCalendar";
import { openConceptInApp } from "../lib/nav";

const errMsg = (e: unknown) => (e instanceof Error ? e.message : String(e));

const OVERDUE_LIMIT = 20;

// 캘린더 pane 너비(드래그 조절, localStorage 영속). 가로 비중은 사용자가 직접 정한다.
const CAL_W_KEY = "amber.todo.cal-width";
const CAL_W_MIN = 340;
const CAL_W_MAX = 780;
const CAL_W_DEFAULT = 460;

export function TodoView({ active }: { active: boolean }) {
  const [selected, setSelected] = useState(() => todayStr());
  const [cursor, setCursor] = useState(() => monthOf(todayStr()));

  const [todos, setTodos] = useState<Todo[]>([]);
  const [counts, setCounts] = useState<Record<string, DayTodoCount>>({});
  const [overdue, setOverdue] = useState<Todo[]>([]);
  const [overdueOpen, setOverdueOpen] = useState(false);
  const [learned, setLearned] = useState<{ id: number; title: string }[]>([]);

  const [quick, setQuick] = useState("");
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editText, setEditText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dragId, setDragId] = useState<number | null>(null);

  const quickRef = useRef<HTMLInputElement>(null);
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

  function startResize(e: ReactMouseEvent) {
    e.preventDefault();
    const rect = bodyRef.current?.getBoundingClientRect();
    if (!rect) return;
    document.body.classList.add("resizing-col");
    const onMove = (ev: MouseEvent) => {
      setCalWidth(Math.min(CAL_W_MAX, Math.max(CAL_W_MIN, ev.clientX - rect.left)));
    };
    const onUp = () => {
      document.body.classList.remove("resizing-col");
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  // 할 일 순서 드래그 (포인터 기반 — WKWebView 에서 HTML5 DnD 보다 안정적).
  // 커서 Y 로 대상 행을 찾아 배열을 실시간 재배치하고, 놓을 때 sort_order 를 저장.
  function startDrag(e: ReactMouseEvent, id: number) {
    e.preventDefault();
    setDragId(id);
    document.body.classList.add("dragging-rows");
    const onMove = (ev: MouseEvent) => {
      const rows = Array.from(
        listRef.current?.querySelectorAll<HTMLElement>("[data-todo-id]") ?? [],
      );
      let targetId: number | null = null;
      for (const row of rows) {
        const r = row.getBoundingClientRect();
        if (ev.clientY < r.top + r.height / 2) {
          targetId = Number(row.dataset.todoId);
          break;
        }
      }
      setTodos((prev) => {
        const from = prev.findIndex((x) => x.id === id);
        const to =
          targetId === null
            ? prev.length - 1
            : prev.findIndex((x) => x.id === targetId);
        if (from === -1 || to === -1 || from === to) return prev;
        const next = prev.slice();
        const [moved] = next.splice(from, 1);
        next.splice(from < to ? to - 1 : to, 0, moved);
        return next;
      });
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      document.body.classList.remove("dragging-rows");
      setDragId(null);
      void reorderTodos(todosRef.current.map((t) => t.id)).catch((err) =>
        setError(errMsg(err)),
      );
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
      setOverdue(selected === today ? await listOverdueOpen(today) : []);
      const [start, end] = dayRangeMs(selected);
      setLearned(await conceptsLearnedOn(start, end));
      setError(null);
    } catch (e) {
      setError(errMsg(e));
    }
  }, [selected]);

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

  // 메인 목록 토글 — 낙관적 반영 후 실패 시 롤백. 행 재정렬 없음(제자리 유지), 캘린더 점만 갱신.
  async function toggle(t: Todo) {
    const next: 0 | 1 = t.done === 1 ? 0 : 1;
    setTodos((prev) =>
      prev.map((x) => (x.id === t.id ? { ...x, done: next } : x)),
    );
    try {
      await toggleTodo(t.id, next);
      void reloadCounts();
    } catch (e) {
      setTodos((prev) =>
        prev.map((x) => (x.id === t.id ? { ...x, done: t.done } : x)),
      );
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

  const today = todayStr();
  const isToday = selected === today;
  const doneCount = todos.filter((t) => t.done === 1).length;

  function renderRow(t: Todo, opts?: { overdue?: boolean }) {
    const isOverdue = opts?.overdue ?? false;
    return (
      <div
        className={`todo-row ${t.done === 1 ? "done" : ""} ${dragId === t.id ? "dragging" : ""}`}
        data-todo-id={t.id}
      >
        {!isOverdue && (
          <span
            className="todo-grip"
            title="드래그해서 순서 변경"
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
            onClick={() => (isOverdue ? toggleOverdue(t) : toggle(t))}
          >
            {t.content}
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
          // 메인 목록: 편집/삭제는 hover 오버레이(레이아웃을 밀지 않음)
          <span className="row-actions" onClick={(e) => e.stopPropagation()}>
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

  return (
    <div
      className="body todo-body"
      ref={bodyRef}
      style={{ gridTemplateColumns: `${calWidth}px 1fr` }}
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
      </aside>

      <div
        className="todo-resizer"
        style={{ left: calWidth }}
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
            todos.map((t) => <Fragment key={t.id}>{renderRow(t)}</Fragment>)
          )}
        </div>

        {todos.length > 0 && (
          <div className="detail-meta">
            {todos.length}개 중 {doneCount}개 완료
          </div>
        )}

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
