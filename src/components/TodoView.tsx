// 할 일 섹션: 좌측 미니 캘린더 + 우측 선택 날짜 체크리스트 (2-pane, .claude/DESIGN.md §7).
// 오늘 우선 — 탭을 열면 오늘 + 빠른 추가 입력에 포커스. 정본은 amber.db 의 todos 테이블(lib/todos.ts).

import {
  useCallback,
  useEffect,
  useMemo,
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
  updateTodoContent,
} from "../lib/todos";
import {
  childrenOf as childrenIn,
  clampDropDepth,
  descendantCount,
  flattenSubset,
  resolveDrop,
  subtreeIds,
  visibleRoots,
} from "../lib/todoTree";
import {
  createBlock,
  findFreeSlot,
  listBlocks,
  nowMinute,
} from "../lib/timeBlocks";
import {
  DEFAULT_KIND,
  VACATION_KINDS,
  listVacations,
  removeVacation,
  setVacation,
  vacationLabel,
  type VacationKind,
} from "../lib/vacations";
import { conceptsLearnedOn } from "../lib/db";
import {
  dayRangeMs,
  formatDayLong,
  formatDayShort,
  localDateStr,
  mondayOf,
  monthGridDates,
  monthOf,
  parseLocalDate,
  shiftDay,
  todayStr,
  weekStartOf,
} from "../lib/date";
import { t } from "../lib/i18n";
import { errText } from "../lib/errors";
import { Checkbox, Modal, Select, Tooltip } from "../ui";
import { Icon } from "../icons";
import { MiniCalendar } from "./MiniCalendar";
import { DayTimetable, type TtView } from "./DayTimetable";
import { DailyReportPanel } from "./DailyReportPanel";
import { WeeklyReportPanel } from "./WeeklyReportPanel";
import { useReportGeneratingDates } from "../lib/reportRun";
import { usePaneResize } from "../lib/usePaneResize";
import { openConceptInApp } from "../lib/nav";
import type { AppConfig } from "../lib/config";

const errMsg = errText; // Rust 코드화 에러까지 번역 (lib/errors.ts)

/** 번역 문자열의 {name} 자리에 <b>제목</b> 을 끼워 넣는다 — 어순(굵힘 위치)은 언어별 사전이 정한다 */
function withBoldName(template: string, name: string) {
  const [before, after] = template.split("{name}");
  return (
    <>
      {before}
      <b>{name}</b>
      {after}
    </>
  );
}

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

// 캘린더 pane 너비(드래그 조절 — 공용 usePaneResize). 캘린더는 트리보다 넓어야 읽히므로
// 기본 치수만 따로 준다: 최소 260 은 셀 안 날짜 원이 들어가는 하한.
const CAL_W_KEY = "amber.todo.cal-width";
const CAL_W_MIN = 260;
const CAL_W_MAX = 780;
const CAL_W_DEFAULT = 460;

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
  // 휴가로 표시한 날짜 → 종류. 캘린더 그리드 범위만 들고 있는다(counts 와 같은 주기로 갱신)
  const [vacations, setVacations] = useState<Record<string, VacationKind>>({});
  // 리포트를 생성 중인 날짜 — 캘린더의 그날 점을 깜빡이게 한다
  const generatingDates = useReportGeneratingDates();
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
  // 자손이 딸린 항목 삭제 확인 — count 는 함께 사라질 자손 수
  const [confirmDelete, setConfirmDelete] = useState<{
    todo: Todo;
    count: number;
  } | null>(null);

  const quickRef = useRef<HTMLInputElement>(null);
  const childInputRef = useRef<HTMLInputElement>(null);
  // 입력창은 blur 에도 저장한다(§ Esc 만 취소). Enter·Esc 로 이미 끝난 세션에서 unmount 시
  // blur 가 한 번 더 와도 중복 저장/되살아나지 않도록 '이미 정리됨'을 ref 로 기억한다.
  // reloadDay/reloadCounts 세대 번호 — 날짜·달을 빠르게 넘기면 옛 응답이 최신 화면을 덮는다
  const daySeq = useRef(0);
  const countsSeq = useRef(0);
  const editDone = useRef(false);
  const childDone = useRef(false);
  const listRef = useRef<HTMLDivElement>(null);
  const todosRef = useRef(todos);
  todosRef.current = todos;

  const pane = usePaneResize({
    storageKey: CAL_W_KEY,
    active,
    min: CAL_W_MIN,
    max: CAL_W_MAX,
    defaultWidth: CAL_W_DEFAULT,
  });

  // 할 일 트리 드래그 (포인터 기반 — WKWebView 에서 HTML5 DnD 보다 안정적).
  // Todoist 문법: **세로 = 삽입 위치, 가로 = 깊이(들여쓰기)**. 한 제스처로 재정렬과
  // 부모 변경(하위로 넣기·꺼내기·다른 부모로)을 모두 처리한다.
  // 피드백: 원본(서브트리째)은 접히고, 삽입 지점 아래 행들이 **실시간으로 밀려 gap 을 연다**
  // (0.18s ease-out). 행 복제 오버레이가 커서를 1:1 로 따라오고, gap 안의 깊이만큼 들여쓴
  // 2px 삽입선이 떨어질 곳을 보여준다. 드래그 중 리렌더 없이 transform 명령형, 커밋은 놓을 때 한 번.
  function startDrag(e: ReactMouseEvent, id: number) {
    e.preventDefault();
    const listEl = listRef.current;
    if (!listEl) return;
    // 이월 고스트(이 날짜에서 다른 날로 옮겨간 행)는 드래그 모델에서 제외한다 — 이 날짜의
    // 형제 그룹이 아니라, 함께 sort_order 를 다시 매기면 실제 날짜에서의 순서가 망가진다.
    // 고스트는 과거 날짜에만 나타나므로(lib/todos.ts listTodos) 오늘의 드래그는 그대로다.
    const cur = todosRef.current.filter((t) => t.carried !== 1);
    const node = cur.find((t) => t.id === id);
    if (!node) return;

    // 트리 판단(플랫 행·서브트리·깊이 clamp·드롭 해석)은 전부 순수 모듈 lib/todoTree.ts.
    // 여기 남는 건 실측(rect)·오버레이 transform·DB 커밋뿐이다.
    // flattenSubset 인 이유: 이월로 부모만 다른 날에 있는 자식이 이 날 목록에 남을 수 있는데
    // (moveTodos 는 parent_id 를 안 건드린다), flattenTree 는 그런 행을 통째로 잃는다.
    const rows = flattenSubset(cur);
    const srcDepth = rows.find((r) => r.id === id)?.depth ?? 0;
    // 드래그 서브트리 — 자기 자신/자손 안으로는 못 들어간다 (후보에서 제외 = 순환 방지)
    const subtree = subtreeIds(cur, id);

    // 행 요소 맵 (실측은 원본을 접은 뒤 beginVisuals 에서 — 접힘 reflow 반영 좌표가 필요)
    const els = new Map<number, HTMLElement>();
    for (const el of Array.from(
      listEl.querySelectorAll<HTMLElement>(".todo-row[data-todo-id]"),
    )) {
      els.set(Number(el.dataset.todoId), el);
    }
    const srcRowEl = els.get(id);
    const srcUnitEl = listEl.querySelector<HTMLElement>(
      `.todo-unit[data-unit-id="${id}"]`,
    );
    if (!srcRowEl || !srcUnitEl) return;
    const srcRect = srcRowEl.getBoundingClientRect(); // 접기 전 — 오버레이 크기/grab 오프셋용
    const gapH = srcRect.height; // 서브트리는 오버레이 칩(+N)으로 접히므로 gap 은 한 행 높이
    // 삽입 후보 = 서브트리 제외 행들 (세로 순서 유지)
    const others = rows.filter((r) => !subtree.has(r.id) && els.has(r.id));
    const rects = new Map<number, DOMRect>();
    let listRect = listEl.getBoundingClientRect();

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
      listEl.classList.add("reordering"); // 드래그 중에만 밀림 트랜지션 활성
      // 원본(서브트리째) 접기 → 아래 행들이 자연 reflow 로 올라온 좌표를 실측
      srcUnitEl.style.display = "none";
      for (const r of others) {
        rects.set(r.id, els.get(r.id)!.getBoundingClientRect());
      }
      listRect = listEl.getBoundingClientRect();
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
      // 삽입 index: 커서 Y 가 중심을 지난 행 수 (others 는 세로 정렬 상태, 접힘 후 좌표)
      let idx = 0;
      for (const r of others) {
        const rc = rects.get(r.id)!;
        if (clientY > rc.top + rc.height / 2) idx++;
        else break;
      }
      const above = others[idx - 1] ?? null;
      const below = others[idx] ?? null;
      const desired = srcDepth + Math.round((clientX - startX) / INDENT);
      const depth = clampDropDepth(desired, above, below);
      slot = { idx, depth };
      // 밀림: 슬롯 아래 행 전부 gap 높이만큼 내려 자리를 비운다 (트랜지션은 .reordering CSS)
      for (let j = 0; j < others.length; j++) {
        const el = els.get(others[j].id)!;
        el.style.transform = j >= idx ? `translateY(${gapH}px)` : "";
      }
      // 삽입선: 열린 gap 의 세로 중앙 + 깊이만큼 들여쓴 왼쪽 (listing 좌표계)
      const y = below
        ? rects.get(below.id)!.top + gapH / 2
        : above
          ? rects.get(above.id)!.bottom + 4
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
      // 트랜지션을 먼저 끄고(즉시 스냅) 밀림 transform 제거 + 원본 복원 → reload 결과가 정본
      listEl.classList.remove("reordering");
      for (const r of others) {
        const el = els.get(r.id);
        if (el) el.style.transform = "";
      }
      srcUnitEl.style.display = "";
      overlay?.remove();
      line?.remove();
      if (!moved || !slot) return;

      const drop = resolveDrop(cur, others, id, slot); // null = 제자리·불가 슬롯
      if (!drop) return;
      const newParent = drop.newParentId;
      const oldParent = node.parent_id ?? null;
      void (async () => {
        try {
          if (newParent !== oldParent) await reparentTodo(id, newParent);
          await reorderTodos(drop.orderedSiblingIds);
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
    const seq = ++daySeq.current;
    try {
      const today = todayStr();
      const rows = await listTodos(selected);
      if (seq !== daySeq.current) return; // 그 사이 다른 날짜를 골랐다 — 밀려난 응답은 버린다
      setTodos(rows);
      const [ttFrom, ttTo] = ttRange(ttView, selected);
      const bl = await listBlocks(ttFrom, ttTo);
      const ov = selected === today ? await listOverdueOpen(today) : [];
      const [start, end] = dayRangeMs(selected);
      const lr = await conceptsLearnedOn(start, end);
      if (seq !== daySeq.current) return;
      setBlocks(bl);
      setOverdue(ov);
      setLearned(lr);
      setError(null);
    } catch (e) {
      if (seq !== daySeq.current) return;
      setError(errMsg(e));
    }
  }, [selected, ttView]);

  // 표시 중인 달 그리드 범위의 날짜별 개수 + 휴가 (캘린더 점·월 요약)
  const reloadCounts = useCallback(async () => {
    const seq = ++countsSeq.current;
    try {
      const dates = monthGridDates(cursor.year, cursor.month);
      const [from, to] = [dates[0], dates[dates.length - 1]];
      const rows = await listMonthCounts(from, to);
      const vac = await listVacations(from, to);
      if (seq !== countsSeq.current) return; // 달을 연달아 넘겼다 — 옛 응답은 버린다
      const map: Record<string, DayTodoCount> = {};
      for (const r of rows) map[r.due_date] = r;
      setCounts(map);
      setVacations(vac);
    } catch (e) {
      if (seq !== countsSeq.current) return;
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

  // 선택한 날의 휴가. 하루에 하나뿐이라(vacations.date PK) 상태는 '있다/없다' 둘뿐 —
  // kind=null 이 곧 해제다. 갱신은 reloadCounts 하나로 끝난다(캘린더가 유일한 표시처).
  const selectedVac = vacations[selected] ?? null;
  async function changeVacation(kind: VacationKind | null) {
    try {
      if (kind) await setVacation(selected, kind);
      else await removeVacation(selected);
      await reloadCounts();
    } catch (e) {
      setError(errMsg(e));
    }
  }

  /** keepFocus=false 는 blur 로 저장하는 경로 — 사용자가 방금 클릭한 곳에서 포커스를 뺏지 않는다 */
  async function add(keepFocus = true) {
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
      if (keepFocus) quickRef.current?.focus();
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

  // 밀린 스트립 항목 토글 — 메인 토글과 같은 전파(하향 서브트리 + 상향 재계산). 예전엔 단일 행만
  // 바꿔서 마지막 자식을 완료해도 부모가 스트립에 남았다. 완료하면 스트립에서 빠지므로 전체 갱신.
  async function toggleOverdue(t: Todo) {
    const next: 0 | 1 = t.done === 1 ? 0 : 1;
    try {
      await setSubtreeDone(t.id, next);
      await recomputeChainFrom(t.parent_id);
      refreshAll();
    } catch (e) {
      setError(errMsg(e));
    }
  }

  function startEdit(t: Todo) {
    editDone.current = false;
    setEditingId(t.id);
    setEditText(t.content);
  }

  /** 편집 취소(Esc) — 저장하지 않고 닫는다 */
  function cancelEdit() {
    editDone.current = true;
    setEditingId(null);
  }

  async function saveEdit(t: Todo) {
    if (editDone.current) return; // Enter/Esc 로 이미 끝난 세션 (unmount blur 중복 방지)
    editDone.current = true;
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

  // 삭제는 서브트리째라 비가역이다 — 자손이 있을 때만 확인을 받는다(DESIGN §8: 확인 모달 남발 금지).
  // 홑 항목(대부분)은 예전처럼 원클릭. 자손 수는 트리 모듈이 세고, 밀린 스트립 행은 그 스트립에서 센다.
  function askRemove(t: Todo, isOverdue: boolean) {
    const n = descendantCount(isOverdue ? overdue : todos, t.id);
    if (n === 0) {
      void remove(t);
      return;
    }
    setConfirmDelete({ todo: t, count: n });
  }

  async function remove(t: Todo) {
    setConfirmDelete(null);
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
      // 완료 재계산을 하지 않는다 — 옮기는 건 끝내는 게 아니다(lib/todos.moveTodos 주석)
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
  async function addChild(parentId: number, keepFocus = true) {
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
      if (keepFocus) childInputRef.current?.focus();
    }
  }

  const today = todayStr();
  const isToday = selected === today;
  // 부모가 이 날에 없는 항목(이월로 부모만 다른 날에 있는 자식)도 루트로 그린다 —
  // childrenIn(todos, null) 로만 고르면 그런 행이 어느 날에도 안 보인다
  const topLevel = visibleRoots(todos);
  const childrenOf = (pid: number) => childrenIn(todos, pid);
  // 하단 진행률은 **이 날짜가 맡은 일**만 센다 — 고스트는 다른 날로 넘긴 기록이라 제외한다.
  // (캘린더 점·월 요약도 due_date 기준이라 listMonthCounts 와 같은 기준이 된다)
  const ownTop = topLevel.filter((t) => t.carried !== 1);
  const doneTop = ownTop.filter((t) => t.done === 1).length;
  // 밀린 목록도 본문처럼 계층으로 — 부모가 빠질 수 있는 부분 집합이라 flattenSubset
  // 렌더 본문에서 계산하면 빠른 추가 입력의 키 입력마다 재계산된다 (overdue 는 안 바뀌는데도)
  const overdueRows = useMemo(() => flattenSubset(overdue), [overdue]);
  const overdueById = new Map(overdue.map((o) => [o.id, o]));

  // 파라미터를 todo 로 둔다 — t 로 줄이면 i18n 의 t() 를 가려서(shadowing) 번역 호출이 깨진다
  function renderRow(todo: Todo, opts?: { overdue?: boolean }) {
    const isOverdue = opts?.overdue ?? false;
    // 이월 고스트 — 이 날짜에 있었지만 다른 날로 넘어간 행. **체크는 된다**(그게 요점: 어제
    // 화면에서 끝내면 그 할 일이 완료되고 도착 날짜에도 체크된 채로 남는다). 편집·드래그·
    // 삭제·하위추가는 막는다 — 그 할 일이 지금 사는 곳은 도착 날짜라 거기서 다루게 한다.
    const isCarried = !isOverdue && todo.carried === 1;
    // 지워진 이월 고스트 — 이 날짜에 있었다는 기록만 남은 줄. 완료할 할 일이 더는 없으므로
    // 체크도 막는다(살아있는 고스트와 다른 점은 이것뿐).
    const isGone = isCarried && todo.deleted_at != null;
    const readOnly = isOverdue || isCarried;
    const onToggle = () => {
      if (isGone) return;
      if (isOverdue) toggleOverdue(todo);
      else toggle(todo);
    };
    const kids = isOverdue ? [] : childrenOf(todo.id);
    const kidsDone = kids.filter((k) => k.done === 1).length;
    return (
      <div
        className={`todo-row ${todo.done === 1 ? "done" : ""} ${isCarried ? "carried" : ""} ${isGone ? "gone" : ""}`}
        data-todo-id={todo.id}
      >
        {!readOnly && (
          <span
            className="todo-grip"
            title={t("todos.row.grip")}
            aria-hidden="true"
            onMouseDown={(e) => startDrag(e, todo.id)}
          >
            <Icon name="grip" size={14} />
          </span>
        )}
        <Checkbox
          checked={todo.done === 1}
          onChange={onToggle}
          label={todo.content}
        />
        {editingId === todo.id && !readOnly ? (
          <input
            className="input todo-edit"
            autoFocus
            value={editText}
            onChange={(e) => setEditText(e.target.value)}
            onKeyDown={(e) => {
              if (e.nativeEvent.isComposing) return;
              if (e.key === "Enter") void saveEdit(todo);
              if (e.key === "Escape") cancelEdit();
            }}
            // 바깥을 클릭해도 편집 내용을 버리지 않는다 — 취소는 Esc
            onBlur={() => void saveEdit(todo)}
          />
        ) : (
          <span
            className="todo-text"
            onClick={() => (readOnly ? onToggle() : startEdit(todo))}
          >
            {todo.content}
          </span>
        )}

        {kids.length > 0 && (
          <span className="todo-progress" title={t("todos.row.progress")}>
            {kidsDone}/{kids.length}
          </span>
        )}

        {isOverdue ? (
          // 밀린 스트립: 원래 날짜 + 가져오기/버리기 — 발견성 위해 항상 표시(hover 오버레이 아님)
          <span className="todo-overdue-actions">
            <span className="todo-row-date">{formatDayShort(todo.due_date)}</span>
            <button
              className="btn btn-sm"
              onClick={() => void moveToday([todo.id])}
            >
              {t("todos.overdue.moveOne")}
            </button>
            <Tooltip label={t("common.delete")}>
              <button
                aria-label={t("common.delete")}
                className="icon-btn sm danger"
                onClick={() => askRemove(todo, true)}
              >
                <Icon name="trash" size={13} />
              </button>
            </Tooltip>
          </span>
        ) : isGone ? (
          // 지워진 고스트: 갈 곳이 없으니 '→ 날짜' 대신 지워졌다는 사실만. 어제 목록은
          // 어제의 기록이라 오늘 지운 일이 이 줄을 걷어가지 않는다(migrations/0009).
          <span
            className="todo-row-date todo-carried-to"
            title={t("todos.row.deletedGhost")}
          >
            {t("todos.row.deleted")}
          </span>
        ) : isCarried ? (
          // 이월 고스트: 편집/삭제는 그 할 일이 지금 사는 도착 날짜에서 한다. 단 **하위 추가는
          // 된다** — 체크처럼 '그 날의 기록'을 채우는 행위라서다: 그 날 실제로 한 일이 뒤늦게
          // 기억나면 여기서 추가·체크해 원래 날짜에 남긴다(새 자식의 due_date 는 보는 날짜).
          <>
            <span className="row-actions" onClick={(e) => e.stopPropagation()}>
              <Tooltip label={t("todos.row.addChild")}>
                <button
                  aria-label={t("todos.row.addChild")}
                  className="icon-btn sm"
                  onClick={() => {
                    childDone.current = false;
                    setAddingChildFor(todo.id);
                    setChildInput("");
                  }}
                >
                  <Icon name="plus" size={13} />
                </button>
              </Tooltip>
            </span>
            <span
              className="todo-row-date todo-carried-to"
              title={t("todos.row.carriedTo", {
                date: formatDayLong(todo.due_date),
              })}
            >
              <Icon name="chevron-right" size={12} />
              {formatDayShort(todo.due_date)}
            </span>
          </>
        ) : (
          // 메인 목록: 하위추가/편집/삭제는 hover 오버레이(레이아웃을 밀지 않음)
          <span className="row-actions" onClick={(e) => e.stopPropagation()}>
            <Tooltip label={t("todos.row.addChild")}>
              <button
                aria-label={t("todos.row.addChild")}
                className="icon-btn sm"
                onClick={() => {
                  childDone.current = false;
                  setAddingChildFor(todo.id);
                  setChildInput("");
                }}
              >
                <Icon name="plus" size={13} />
              </button>
            </Tooltip>
            <Tooltip label={t("todos.row.schedule")}>
              <button
                aria-label={t("todos.row.schedule")}
                className="icon-btn sm"
                onClick={() => void scheduleTodo(todo)}
              >
                <Icon name="clock" size={13} />
              </button>
            </Tooltip>
            <Tooltip label={t("todos.row.rename")}>
              <button
                aria-label={t("todos.row.rename")}
                className="icon-btn sm"
                onClick={() => startEdit(todo)}
              >
                <Icon name="pencil" size={13} />
              </button>
            </Tooltip>
            <Tooltip label={t("common.delete")}>
              <button
                aria-label={t("common.delete")}
                className="icon-btn sm danger"
                onClick={() => askRemove(todo, false)}
              >
                <Icon name="trash" size={13} />
              </button>
            </Tooltip>
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
        className="todo-unit"
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
              placeholder={t("todos.child.placeholder")}
              value={childInput}
              onChange={(e) => setChildInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.nativeEvent.isComposing) return;
                if (e.key === "Enter") void addChild(node.id);
                if (e.key === "Escape") {
                  childDone.current = true;
                  setAddingChildFor(null);
                  setChildInput("");
                }
              }}
              // 바깥을 클릭하면 적던 하위 항목을 버리지 않고 저장하며 닫는다 — 취소는 Esc
              onBlur={() => {
                if (childDone.current) return;
                childDone.current = true;
                setAddingChildFor(null);
                if (childInput.trim()) void addChild(node.id, false);
                else setChildInput("");
              }}
            />
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="body todo-body" {...pane.bodyProps}>
      <aside className="list todo-cal-pane">
        <MiniCalendar
          year={cursor.year}
          month={cursor.month}
          selected={selected}
          today={today}
          counts={counts}
          vacations={vacations}
          generating={generatingDates}
          onSelect={goDate}
          onCursor={(y, m) => setCursor({ year: y, month: m })}
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

      <div {...pane.resizerProps} />

      <section className="detail">
        <div className="detail-head todo-head">
          <h1 className="detail-title">{formatDayLong(selected)}</h1>
          <span className="spacer" />
          {/* 휴가 — 안 잡힌 날엔 조용한 버튼 하나(한 번 누르면 연차), 잡힌 날엔 노랑 칩이 되어
              종류 변경·해제를 연다. 매일 보는 헤더라 평소엔 가볍게 두고 켜졌을 때만 무게를 준다.
              쓰는 빈도(1년에 몇 번)와 보는 빈도(매일)가 다를 땐 후자에 맞춘다(DESIGN §1). */}
          {selectedVac ? (
            <div className="todo-vac on">
              <Select<VacationKind | "">
                value={selectedVac}
                options={[
                  ...VACATION_KINDS.map((k) => ({
                    value: k as VacationKind | "",
                    label: vacationLabel(k),
                  })),
                  { value: "", label: t("todos.vac.clear") },
                ]}
                onChange={(v) => void changeVacation(v || null)}
                align="right"
              />
            </div>
          ) : (
            <button
              className="btn btn-sm todo-vac"
              title={t("todos.vac.setHint")}
              onClick={() => void changeVacation(DEFAULT_KIND)}
            >
              {t("todos.vac.set")}
            </button>
          )}
          {/* 캘린더 앱 표준: [오늘] ‹ › — 오늘이면 '오늘' 버튼 비활성(이미 오늘임을 표시) */}
          <div className="todo-nav">
            <button
              className="btn btn-sm"
              onClick={() => goDate(today)}
              disabled={isToday}
            >
              {t("todos.today")}
            </button>
            <Tooltip label={t("todos.nav.prevDay")}>
              <button
                aria-label={t("todos.nav.prevDay")}
                className="icon-btn ghost"
                onClick={() => goDate(shiftDay(selected, -1))}
              >
                <Icon name="chevron-left" size={16} />
              </button>
            </Tooltip>
            <Tooltip label={t("todos.nav.nextDay")}>
              <button
                aria-label={t("todos.nav.nextDay")}
                className="icon-btn ghost"
                onClick={() => goDate(shiftDay(selected, 1))}
              >
                <Icon name="chevron-right" size={16} />
              </button>
            </Tooltip>
          </div>
        </div>

        {error && <div className="error-note">{error}</div>}

        {/* 밀린 할 일은 '오늘 계획의 재료'라 날짜 바로 아래. 입력창과 그 입력이 들어갈 목록
            사이에 끼우면 타이핑한 것이 무관한 블록 아래에 나타나 매핑이 깨진다(DESIGN §7) */}
        {isToday && overdue.length > 0 && (
          <div className={`todo-overdue ${overdueOpen ? "open" : ""}`}>
            <div
              className="todo-overdue-head"
              onClick={() => setOverdueOpen((v) => !v)}
            >
              <span className="todo-overdue-caret">
                <Icon name="chevron-right" size={14} />
              </span>
              <b>{t("todos.overdue.title")}</b>
              <span className="todo-overdue-cnt">{overdue.length}</span>
              <span className="spacer" />
              <button
                className="btn btn-sm"
                onClick={(e) => {
                  e.stopPropagation();
                  void moveToday(overdue.map((o) => o.id));
                }}
              >
                {t("todos.overdue.moveAll")}
              </button>
            </div>
            <div className="todo-overdue-body">
              <div className="todo-overdue-inner">
                {overdueRows.slice(0, OVERDUE_LIMIT).map((r) => (
                  <div
                    key={r.id}
                    style={
                      r.depth ? { marginLeft: r.depth * INDENT } : undefined
                    }
                  >
                    {renderRow(overdueById.get(r.id)!, { overdue: true })}
                  </div>
                ))}
                {overdue.length > OVERDUE_LIMIT && (
                  <div className="hint todo-overdue-more">
                    {t("todos.overdue.more", { n: overdue.length - OVERDUE_LIMIT })}
                  </div>
                )}
              </div>
            </div>
          </div>
        )}

        <div className="todo-quick">
          <span className="todo-quick-ico">
            <Icon name="plus" size={15} />
          </span>
          <input
            ref={quickRef}
            className="input todo-quick-input"
            placeholder={t("todos.quick.placeholder")}
            value={quick}
            onChange={(e) => setQuick(e.target.value)}
            onKeyDown={(e) => {
              if (e.nativeEvent.isComposing) return;
              if (e.key === "Enter") void add();
              if (e.key === "Escape") setQuick("");
            }}
            // 바깥을 클릭해도 적던 할 일을 잃지 않는다 — 비우려면 Esc
            onBlur={() => void add(false)}
          />
        </div>

        <div className="todo-listing" ref={listRef}>
          {todos.length === 0 ? (
            <div className="hint todo-day-empty">{t("todos.empty.day")}</div>
          ) : (
            topLevel.map((p) => renderUnit(p, 0))
          )}
        </div>

        {ownTop.length > 0 && (
          <div className="detail-meta">
            {t("todos.meta.done", { total: ownTop.length, done: doneTop })}
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

        {/* 주간 리포트 — 타임테이블이 '주' 뷰일 때만. 일 단위로 보는 중에 주간 블록이 끼면
            하루 흐름을 방해한다. 뷰 토글이 곧 '주간을 보겠다'는 의사표시라 접힘 없이 바로 펼친다.
            key 는 월요일이라 같은 주 안에서 날짜를 옮겨도 패널 상태가 유지된다 */}
        {ttView === "week" && (
          <WeeklyReportPanel
            key={mondayOf(selected)}
            weekStart={mondayOf(selected)}
            config={config}
            active={active}
            onOpenSettings={onOpenSettings}
          />
        )}

        {learned.length > 0 && (
          <div className="todo-learned">
            <div className="todo-learned-label">
              {t("todos.learned.label", { n: learned.length })}
            </div>
            <div className="todo-learned-chips">
              {learned.map((c) => (
                <button
                  key={c.id}
                  className="chip btn-like"
                  title={t("todos.learned.open")}
                  onClick={() => openConceptInApp(c.id)}
                >
                  {c.title}
                </button>
              ))}
            </div>
          </div>
        )}
      </section>

      <Modal
        open={confirmDelete != null}
        title={t("todos.delete.title")}
        narrow
        onClose={() => setConfirmDelete(null)}
        footer={
          <>
            <span className="spacer" />
            <button className="btn btn-sm" onClick={() => setConfirmDelete(null)}>
              {t("common.cancel")}
            </button>
            <button
              className="btn btn-sm btn-danger-ghost"
              onClick={() => confirmDelete && void remove(confirmDelete.todo)}
            >
              {t("common.delete")}
            </button>
          </>
        }
      >
        {confirmDelete && (
          <p style={{ margin: 0 }}>
            {withBoldName(
              t("todos.delete.confirm", { n: confirmDelete.count }),
              confirmDelete.todo.content,
            )}
            <br />
            {t("todos.delete.irreversible")}
          </p>
        )}
      </Modal>
    </div>
  );
}
