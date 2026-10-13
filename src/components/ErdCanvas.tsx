// ERD 캔버스 — 그림은 resin 플레이그라운드의 것을 그대로 쓴다(resin/viewer 의 Canvas: 라이브 유리, 이동/확대,
// 초점). 이 컴포넌트는 그 둘레의 틀만 만든다: 떠 있는 조작(보기, 배경, 확대), 오른쪽 표 패널, 컬럼 정보창.
// 패널과 정보창의 내용도 resin 의 것(tableDetails, columnDetails)이라 플레이그라운드와 같은 말을 한다.
//
// 조작 규약(resin 과 같다): 휠 = 포인터 중심 확대, 드래그 = 이동, 더블클릭 = 맞춤. 표 이름 = 패널, 컬럼 = 정보창,
// 표 몸통 = 초점만, 빈 곳 = 정보창 닫고 초점은 패널의 표로. 패널은 X 와 Esc 로만 닫힌다.
// ⌘F 는 그려진 표와 컬럼의 이름에서 찾는다(트리에 포커스가 있으면 트리 찾기가 먼저 받는다).

import { useEffect, useRef, useState } from "react";
import type { Model } from "@jhzlo/resin";
import {
  CANVAS_STAGES,
  Canvas,
  columnDetails,
  tableDetails,
  type CanvasHit,
  type CanvasStage,
  type MountedDrawing,
} from "@jhzlo/resin/viewer";
import "@jhzlo/resin/viewer.css";
import { drawErd } from "../lib/erdRender";
import { THEME_EVENT, resolvedTheme } from "../lib/theme";
import { t } from "../lib/i18n";
import { errText } from "../lib/errors";
import { Icon } from "../icons";
import { Spinner, Tooltip } from "../ui";

type Columns = "all" | "keys" | "none";
interface ViewPrefs {
  columns: Columns;
  audit: "collapse" | "expand";
  edges: "angular" | "curved";
  /** 세로(위에서 아래, 선이 카드 위아래에 붙는다) 또는 가로(resin 기본, 선이 컬럼 행에 붙는다) */
  direction: "down" | "right";
  grid: boolean;
  stage: CanvasStage;
}

const VIEW_KEY = "amber.erd.view";
const PANEL_KEY = "amber.erd.panel";
const PANEL_MIN = 360;
const DEFAULT_VIEW: ViewPrefs = { columns: "all", audit: "collapse", edges: "angular", direction: "down", grid: true, stage: "aurora" };
/** 다시 그리지 않고 되살릴 그림 — 스키마를 오갈 때. 개수와 DOM 노드 수 둘 다로 묶는다(플레이그라운드와 같은 값) */
const KEEP_DRAWINGS = 4;
const KEEP_NODES = 100_000;

function loadView(): ViewPrefs {
  try {
    const v = JSON.parse(localStorage.getItem(VIEW_KEY) ?? "{}") as Partial<ViewPrefs>;
    return {
      columns: v.columns === "keys" || v.columns === "none" ? v.columns : "all",
      audit: v.audit === "expand" ? "expand" : "collapse",
      edges: v.edges === "curved" ? "curved" : "angular",
      direction: v.direction === "right" ? "right" : "down",
      grid: v.grid !== false,
      stage: v.stage && CANVAS_STAGES.includes(v.stage) ? v.stage : DEFAULT_VIEW.stage,
    };
  } catch {
    return DEFAULT_VIEW;
  }
}

function loadPanelWidth(): number {
  try {
    const n = Number(localStorage.getItem(PANEL_KEY));
    return n >= PANEL_MIN ? n : 560;
  } catch {
    return 560;
  }
}

export function ErdCanvas({
  model,
  drawingId,
  active,
  reveal,
}: {
  /** 그릴 모델. null 이면 비운다 */
  model: Model | null;
  /** 지금 그리는 스키마와 그 구조를 말하는 값 — 바뀌면 화면에 맞춰 새로 보이고 패널을 닫는다 */
  drawingId: string;
  /** 탭과 보기가 보이는가 — 아니면 유리 그리기를 멈춘다 */
  active: boolean;
  /** 트리에서 고른 표 — 그려진 뒤 그 표의 패널을 연다(seq 가 바뀔 때마다) */
  reveal: { table: string; seq: number } | null;
}) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const viewportRef = useRef<HTMLDivElement>(null);
  const panelRef = useRef<HTMLElement>(null);
  const panelBodyRef = useRef<HTMLDivElement>(null);
  const popRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<Canvas | null>(null);

  const [view, setView] = useState<ViewPrefs>(loadView);
  const [dark, setDark] = useState(() => resolvedTheme() === "dark");
  const [zoom, setZoom] = useState(100);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [inspected, setInspected] = useState<string | null>(null);
  const [popped, setPopped] = useState<{ table: string; column: string } | null>(null);
  const [panelWidth, setPanelWidth] = useState(loadPanelWidth);
  // 패널 옆에 남은 캔버스 폭 — 좁으면 배경 고르기가 글자를 접고, 더 좁으면 비켜선다(플레이그라운드와 같은 문턱)
  const [room, setRoom] = useState<"" | "snug" | "tight">("");
  const [findOpen, setFindOpen] = useState(false);
  const [findQuery, setFindQuery] = useState("");
  const [findIdx, setFindIdx] = useState(0);
  const [findCount, setFindCount] = useState(0);
  // 찾기 바가 좌상단 보기 조작과 부딪히면 그 아래 줄로 내려간다(패널이 열려 폭이 줄 때)
  const [findLow, setFindLow] = useState(false);
  const findInputRef = useRef<HTMLInputElement>(null);
  const findBarRef = useRef<HTMLDivElement>(null);
  const viewFloatRef = useRef<HTMLDivElement>(null);
  const findHits = useRef<Element[]>([]);
  /** 찾느라 초점 흐림을 걷었는가 — 찾기를 비우거나 닫을 때 한 번만 되돌린다 */
  const findCleared = useRef(false);

  const mounted = useRef(new Map<string, MountedDrawing>());
  const shownKey = useRef("");
  const shownDrawing = useRef<MountedDrawing | null>(null);
  const shownId = useRef("");
  const shownDirection = useRef("");
  const renderSeq = useRef(0);
  /** 그림이 바뀐 뒤 할 일 — 숨은 컬럼을 고르면 컬럼을 펼쳐 다시 그린 다음 정보창을 연다 */
  const afterDraw = useRef<(() => void) | null>(null);
  const popFrame = useRef(0);

  // 콜백이 늘 지금 상태를 보게 한다 — Canvas 는 한 번 만들어 계속 쓰므로 생성 때의 클로저에 갇히면 안 된다
  const live = useRef({ model, inspected, popped, view, find: findOpen ? findQuery : "" });
  live.current = { model, inspected, popped, view, find: findOpen ? findQuery : "" };

  const covered = () => panelRef.current?.offsetWidth ?? 0;

  function placePop() {
    const pop = popRef.current;
    const wrap = wrapRef.current;
    const c = canvasRef.current;
    const p = live.current.popped;
    if (!pop || !wrap || !c || !p) return;
    const row = c.row(p.table, p.column);
    if (!row) {
      pop.style.visibility = "hidden";
      return;
    }
    const area = wrap.getBoundingClientRect();
    const r = row.getBoundingClientRect();
    const w = pop.offsetWidth;
    const h = pop.offsetHeight;
    const cover = covered();
    const left = Math.max(8, Math.min(r.left - area.left, area.width - cover - w - 8));
    const below = r.bottom - area.top + 6;
    const above = r.top - area.top - h - 6;
    const top = below + h <= area.height - 8 || above < 8 ? below : above;
    const seen = r.bottom > area.top && r.top < area.bottom && r.right > area.left && r.left < area.right - cover;
    pop.style.visibility = seen ? "" : "hidden";
    pop.style.translate = `${Math.round(left)}px ${Math.round(Math.max(8, Math.min(top, area.height - h - 8)))}px`;
  }

  /** 아무것도 고르지 않았을 때의 초점 — 패널에 표가 있으면 그 표, 없으면 없음 */
  function settleFocus() {
    const c = canvasRef.current;
    const name = live.current.inspected;
    if (name) c?.focusTable(name);
    else c?.focus?.clear();
  }

  function openTable(name: string, onlyIfCovered = false) {
    const c = canvasRef.current;
    if (!c) return;
    setPopped(null);
    setInspected(name);
    c.focusTable(name);
    c.refresh();
    // 패널이 열린 다음(폭이 정해진 다음)에 그 표가 패널에 가렸는지 본다
    requestAnimationFrame(() => c.reveal(name, covered(), onlyIfCovered));
  }

  function pickColumn(table: string, column: string) {
    const c = canvasRef.current;
    if (!c) return;
    const cur = live.current.popped;
    if (cur && cur.table === table && cur.column === column) {
      setPopped(null);
      settleFocus();
      return;
    }
    // 키만/이름만 보기에서 숨은 컬럼을 패널에서 골랐다 — 컬럼을 펼쳐 그린 뒤에 연다
    if (!c.row(table, column)) {
      if (live.current.view.columns === "all") return; // 다 그렸는데도 없다 — 펼쳐도 나오지 않는다
      afterDraw.current = () => pickColumn(table, column);
      setView((v) => ({ ...v, columns: "all" }));
      return;
    }
    c.focus?.row(table, column);
    c.refresh();
    setPopped({ table, column });
    requestAnimationFrame(() => c.revealElement(c.row(table, column), covered()));
  }

  /** 찾기 — 그려진 표 이름(머리글)과 컬럼 이름에서. 칠하는 건 resin 이 행마다 깔아 둔 판(.rz-hit)이라
   *  그림은 그대로다. `move` 가 거짓이면 다시 칠하기만 한다(그림을 새로 올렸을 때) */
  function runFind(q: string, move = true) {
    const root = viewportRef.current;
    if (!root) return;
    for (const el of Array.from(root.querySelectorAll(".erd-find-hit"))) el.classList.remove("erd-find-hit", "erd-find-cur");
    const term = q.trim().toLowerCase();
    const hits: Element[] = [];
    if (term) {
      for (const card of Array.from(root.querySelectorAll<SVGGElement>(".rz-t[data-t]"))) {
        const head = card.querySelector(".rz-head");
        if (head && card.dataset.t!.toLowerCase().includes(term)) hits.push(head);
        for (const row of Array.from(card.querySelectorAll<SVGGElement>(".rz-c[data-c]")))
          if (row.dataset.c!.toLowerCase().includes(term)) hits.push(row);
      }
    }
    for (const el of hits) el.classList.add("erd-find-hit");
    findHits.current = hits;
    setFindCount(hits.length);
    setFindIdx(0);
    if (hits.length) markFind(0, move);
    if (term) liftFocus();
    else if (findCleared.current) {
      findCleared.current = false;
      restoreFocus();
    }
  }

  /** 찾는 동안은 초점 흐림을 걷는다 — 흐려진 표에 걸린 결과도 보여야 한다. 비우거나 닫으면 되돌린다 */
  function liftFocus() {
    const c = canvasRef.current;
    c?.focus?.clear();
    c?.refresh(true);
    findCleared.current = true;
  }

  /** 패널과 정보창이 가리키는 것으로 초점을 되돌린다 */
  function restoreFocus() {
    const c = canvasRef.current;
    const pop = live.current.popped;
    if (pop && c?.row(pop.table, pop.column)) c.focus?.row(pop.table, pop.column);
    else settleFocus();
    c?.refresh(true);
  }

  /** 키보드(입력, Enter)로 옮길 때는 움직이지 않고 바로 간다(DESIGN: 키보드 동작에 애니메이션 없음) */
  function markFind(i: number, move = true, animate = false) {
    const hits = findHits.current;
    for (const el of hits) el.classList.remove("erd-find-cur");
    hits[i]?.classList.add("erd-find-cur");
    if (move) canvasRef.current?.revealElement(hits[i] ?? null, covered(), animate);
  }

  function stepFind(delta: number, animate = false) {
    const n = findHits.current.length;
    if (!n) return;
    const next = (((findIdx + delta) % n) + n) % n;
    setFindIdx(next);
    // 찾는 사이 표를 눌러 다시 흐려졌을 수 있다
    liftFocus();
    markFind(next, true, animate);
  }

  function closeFind() {
    setFindOpen(false);
    setFindQuery("");
    runFind("", false);
  }

  const tap = useRef<(hit: CanvasHit) => void>(() => {});
  tap.current = (hit) => {
    if (hit.service && hit.rowTable) return openTable(hit.rowTable, true);
    if (hit.table && hit.column) return pickColumn(hit.table, hit.column);
    if (hit.table && hit.header) return openTable(hit.table, true);
    setPopped(null);
    if (hit.table) canvasRef.current?.focus?.table(hit.table);
    else settleFocus();
  };

  // 캔버스는 한 번 만든다 — WebGL 문맥을 렌더마다 새로 잡지 않는다
  useEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const c = new Canvas(el, {
      onView: () => {
        setZoom(Math.round(c.view.scale * 100));
        if (!popFrame.current)
          popFrame.current = requestAnimationFrame(() => {
            popFrame.current = 0;
            placePop();
          });
      },
      onTap: (hit) => tap.current(hit),
    });
    canvasRef.current = c;
    return () => {
      c.destroy();
      canvasRef.current = null;
      shownKey.current = "";
      shownDrawing.current = null;
      mounted.current.clear();
    };
  }, []);

  useEffect(() => {
    canvasRef.current?.pause(!active);
  }, [active]);

  useEffect(() => {
    const h = () => setDark(resolvedTheme() === "dark");
    window.addEventListener(THEME_EVENT, h);
    return () => window.removeEventListener(THEME_EVENT, h);
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem(VIEW_KEY, JSON.stringify(view));
    } catch {
      /* 이번 실행 동안만 기억한다 */
    }
  }, [view]);

  // 다른 스키마로 가면 패널과 정보창을 닫는다 — 그 표가 거기엔 없다. 예약해 둔 일도 버린다
  useEffect(() => {
    setInspected(null);
    setPopped(null);
    afterDraw.current = null;
  }, [drawingId]);

  // 그리기: 모델, 보기, 테마가 바뀌면 배치를 받아(워커) 캔버스에 올린다
  useEffect(() => {
    const c = canvasRef.current;
    if (!c) return;
    c.setLook(view.stage, dark);
    c.setGrid(view.grid);
    const seq = ++renderSeq.current;
    if (!model) {
      c.show({ svg: "", width: 0, height: 0, background: null, boxes: [] });
      shownKey.current = "";
      shownDrawing.current = null;
      setBusy(false);
      return;
    }
    const ink = c.drawOptions();
    const key = JSON.stringify([drawingId, view.columns, view.audit, view.edges, view.direction, ink.look, ink.stage]);
    // 다른 스키마이거나 방향이 바뀌면 그림이 통째로 달라진다 — 보던 자리가 뜻을 잃으니 화면에 맞춘다
    const fit = shownId.current !== drawingId || shownDirection.current !== view.direction;
    if (key === shownKey.current) {
      // 앞선 그리기를 이 그림으로 되돌렸다 — 그 그리기는 seq 가 어긋나 스스로 끄지 않으니 여기서 끈다
      setBusy(false);
      setError(null);
      if (fit) c.view.fit();
      shownId.current = drawingId;
      return;
    }
    const restored = mounted.current.get(key);
    setError(null);
    setBusy(!restored);
    void (async () => {
      try {
        const drawing =
          restored ??
          (await drawErd(key, model, {
            ...ink,
            columns: view.columns,
            audit: view.audit,
            edges: view.edges,
            direction: view.direction,
            idPrefix: "erd-",
          }));
        if (seq !== renderSeq.current) return;
        // 지금 보이는 그림은 남겨 둔다 — 스키마를 오가거나 보기를 되돌릴 때 다시 그리지 않게
        const prev = shownDrawing.current;
        if (prev && shownKey.current && prev.nodes <= KEEP_NODES) {
          mounted.current.delete(shownKey.current);
          mounted.current.set(shownKey.current, prev);
          let nodes = [...mounted.current.values()].reduce((n, d) => n + d.nodes, 0);
          while (mounted.current.size > KEEP_DRAWINGS || nodes > KEEP_NODES) {
            const first = mounted.current.keys().next().value!;
            nodes -= mounted.current.get(first)!.nodes;
            mounted.current.delete(first);
          }
        }
        mounted.current.delete(key);
        shownDrawing.current = c.show(drawing, fit);
        shownKey.current = key;
        shownId.current = drawingId;
        shownDirection.current = view.direction;
        // 열려 있던 것을 새 그림에서 다시 잡는다
        const { inspected: open, popped: pop } = live.current;
        if (pop && c.row(pop.table, pop.column)) c.focus?.row(pop.table, pop.column);
        else {
          if (pop) setPopped(null);
          if (open) c.focusTable(open);
        }
        // 찾기를 새 그림에 다시 칠한다(찾는 중이면 위의 초점을 다시 걷는다). 되살린 그림에 남은 지난 칠도 여기서 걷힌다
        runFind(live.current.find, false);
        c.refresh(true);
        const next = afterDraw.current;
        afterDraw.current = null;
        next?.();
        requestAnimationFrame(placePop);
      } catch (e) {
        if (seq === renderSeq.current) setError(errText(e));
      } finally {
        if (seq === renderSeq.current) setBusy(false);
      }
    })();
  }, [model, drawingId, view, dark]);

  // 트리에서 표를 골랐다 — 그림이 이 스키마의 것이면 바로, 아니면 그린 다음에. 카드가 없어도 연다
  // (접힌 감사 테이블처럼 그림에 없는 표도 패널은 모델에서 그린다). seq 가 같은 표를 다시 고른 것도 알린다
  useEffect(() => {
    if (!reveal) return;
    const open = () => openTable(reveal.table);
    if (shownId.current === drawingId) open();
    else afterDraw.current = open;
  }, [reveal?.seq]);

  // 패널 — resin 의 표 설명을 그대로 넣는다
  useEffect(() => {
    const body = panelBodyRef.current;
    if (!body) return;
    if (!model || !inspected) {
      body.replaceChildren();
      return;
    }
    const el = tableDetails(model, inspected, (name) => openTable(name), (tb, col) => pickColumn(tb, col), () => {});
    if (!el) {
      setInspected(null);
      return;
    }
    body.replaceChildren(el);
  }, [model, inspected]);

  // 정보창 — resin 의 컬럼 설명
  useEffect(() => {
    const pop = popRef.current;
    if (!pop) return;
    if (!model || !popped) {
      pop.replaceChildren();
      return;
    }
    const el = columnDetails(model, popped.table, popped.column, (name) => openTable(name));
    if (!el) {
      setPopped(null);
      return;
    }
    pop.replaceChildren(el);
    placePop();
  }, [model, popped]);

  // 패널 폭이 바뀌면 정보창 자리와 남은 폭도 다시
  useEffect(() => {
    requestAnimationFrame(placePop);
    const wrap = wrapRef.current;
    if (!wrap) return;
    const measure = () => {
      const free = wrap.clientWidth - covered();
      setRoom(free < 350 ? "tight" : free < 560 ? "snug" : "");
      const tl = viewFloatRef.current;
      const bar = findBarRef.current;
      setFindLow(!!tl && !!bar && tl.offsetLeft + tl.offsetWidth + 8 > free - 10 - bar.offsetWidth);
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(wrap);
    return () => ro.disconnect();
  }, [panelWidth, inspected, findOpen]);

  // Esc — 안쪽부터: 정보창, 그다음 패널
  useEffect(() => {
    if (!active) return;
    const h = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || document.querySelector(".overlay")) return;
      const { popped: pop, inspected: open } = live.current;
      if (pop) {
        setPopped(null);
        settleFocus();
      } else if (open) {
        setInspected(null);
        canvasRef.current?.focus?.clear();
      } else return;
      canvasRef.current?.refresh(true);
      e.preventDefault();
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [active]);

  // ⌘F — 탭이 보일 때만. 트리에 포커스가 있으면 트리 찾기가 먼저 받고 막는다(TreeFind). 탭을 떠나면 닫는다
  useEffect(() => {
    if (!active) {
      closeFind();
      return;
    }
    const h = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.altKey || e.key.toLowerCase() !== "f") return;
      if (document.querySelector(".overlay, .mmd-zoom-overlay")) return;
      e.preventDefault();
      setFindOpen(true);
      requestAnimationFrame(() => findInputRef.current?.select());
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [active]);

  // 패널 왼쪽 가장자리를 끌어 폭을 바꾼다
  function startResize(e: React.PointerEvent<HTMLDivElement>) {
    const wrap = wrapRef.current;
    if (!wrap) return;
    e.preventDefault();
    const handle = e.currentTarget;
    handle.setPointerCapture(e.pointerId);
    const max = () => Math.max(PANEL_MIN, wrap.clientWidth - 120);
    let width = panelWidth;
    const move = (ev: PointerEvent) => {
      width = Math.round(Math.min(max(), Math.max(PANEL_MIN, wrap.getBoundingClientRect().right - ev.clientX)));
      setPanelWidth(width);
    };
    const up = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      handle.removeEventListener("pointercancel", up);
      try {
        localStorage.setItem(PANEL_KEY, String(width));
      } catch {
        /* 이번 실행 동안만 */
      }
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
    handle.addEventListener("pointercancel", up);
  }

  const set = (patch: Partial<ViewPrefs>) => setView((v) => ({ ...v, ...patch }));
  const zoomBy = (f: number) => canvasRef.current?.view.zoomBy(f, undefined, undefined, true);

  const COLUMN_LEVELS: { id: Columns; label: string; hint: string }[] = [
    { id: "all", label: t("erd.canvas.columns.all"), hint: t("erd.canvas.columns.allHint") },
    { id: "keys", label: t("erd.canvas.columns.keys"), hint: t("erd.canvas.columns.keysHint") },
    { id: "none", label: t("erd.canvas.columns.none"), hint: t("erd.canvas.columns.noneHint") },
  ];

  return (
    <div className="erd-stage" ref={wrapRef} data-room={room || undefined}>
      <div className="erd-viewport" ref={viewportRef} />

      {/* 좌상단 — 무엇을 그리나. 고른 쪽만 채운다(§3) */}
      <div className="erd-float erd-float-tl" ref={viewFloatRef} role="group" aria-label={t("erd.canvas.view")}>
        {COLUMN_LEVELS.map((c) => (
          <Tooltip key={c.id} label={c.hint}>
            <button
              className={`erd-float-btn ${view.columns === c.id ? "on" : ""}`}
              aria-pressed={view.columns === c.id}
              onClick={() => set({ columns: c.id })}
            >
              {c.label}
            </button>
          </Tooltip>
        ))}
        <span className="erd-float-sep" aria-hidden="true" />
        <Tooltip label={t("erd.canvas.auditHint")}>
          <button
            className={`erd-float-btn ${view.audit === "expand" ? "on" : ""}`}
            aria-pressed={view.audit === "expand"}
            onClick={() => set({ audit: view.audit === "expand" ? "collapse" : "expand" })}
          >
            {t("erd.canvas.audit")}
          </button>
        </Tooltip>
        <span className="erd-float-sep" aria-hidden="true" />
        {(["down", "right"] as const).map((d) => (
          <Tooltip key={d} label={t(d === "down" ? "erd.canvas.direction.downHint" : "erd.canvas.direction.rightHint")}>
            <button
              className={`erd-float-btn ${view.direction === d ? "on" : ""}`}
              aria-pressed={view.direction === d}
              onClick={() => set({ direction: d })}
            >
              {t(d === "down" ? "erd.canvas.direction.down" : "erd.canvas.direction.right")}
            </button>
          </Tooltip>
        ))}
        <span className="erd-float-sep" aria-hidden="true" />
        {(["angular", "curved"] as const).map((e) => (
          <button
            key={e}
            className={`erd-float-btn ${view.edges === e ? "on" : ""}`}
            aria-pressed={view.edges === e}
            onClick={() => set({ edges: e })}
          >
            {t(e === "angular" ? "erd.canvas.edges.angular" : "erd.canvas.edges.curved")}
          </button>
        ))}
      </div>

      {/* 우하단 — 화면 조작. 패널이 열리면 패널 왼쪽으로 비켜선다 */}
      <div
        className="erd-float erd-float-br"
        style={inspected ? { right: `calc(min(${panelWidth}px, 100% - 120px) + 10px)` } : undefined}
      >
        <Tooltip label={t("erd.canvas.fit")}>
          <button className="erd-float-btn" aria-label={t("erd.canvas.fit")} onClick={() => canvasRef.current?.view.fit(true)}>
            <Icon name="fit-screen" size={16} />
          </button>
        </Tooltip>
        <Tooltip label={t("erd.canvas.zoomOut")}>
          <button className="erd-float-btn" aria-label={t("erd.canvas.zoomOut")} onClick={() => zoomBy(0.8)}>
            <Icon name="zoom-out" size={16} />
          </button>
        </Tooltip>
        <Tooltip label={t("erd.canvas.actualSize")}>
          <button className="erd-float-btn erd-float-pct" onClick={() => canvasRef.current?.view.actualSize()}>
            {zoom}%
          </button>
        </Tooltip>
        <Tooltip label={t("erd.canvas.zoomIn")}>
          <button className="erd-float-btn" aria-label={t("erd.canvas.zoomIn")} onClick={() => zoomBy(1.25)}>
            <Icon name="zoom-in" size={16} />
          </button>
        </Tooltip>
        <span className="erd-float-sep" aria-hidden="true" />
        <Tooltip label={t("erd.canvas.grid")}>
          <button
            className={`erd-float-btn ${view.grid ? "on" : ""}`}
            aria-label={t("erd.canvas.grid")}
            aria-pressed={view.grid}
            onClick={() => set({ grid: !view.grid })}
          >
            <Icon name="grid" size={15} />
          </button>
        </Tooltip>
      </div>

      {/* 좌하단 — 배경. 테마의 밝고 어두움은 앱을 따른다 */}
      <div className="erd-float erd-float-bl" role="group" aria-label={t("erd.canvas.background")}>
        {CANVAS_STAGES.map((s) => (
          <button
            key={s}
            className={`erd-float-btn ${view.stage === s ? "on" : ""}`}
            aria-pressed={view.stage === s}
            aria-label={t(`erd.canvas.stage.${s}`)}
            onClick={() => set({ stage: s })}
          >
            <span className={`erd-swatch erd-swatch-${s}`} aria-hidden="true" />
            <span className="erd-float-label">{t(`erd.canvas.stage.${s}`)}</span>
          </button>
        ))}
      </div>

      {/* 우상단 — 찾기(⌘F). 본문 검색 바와 같은 구성이고, 패널이 열리면 패널 왼쪽으로 비켜선다 */}
      {findOpen && (
        <div
          className="page-find erd-find"
          ref={findBarRef}
          data-low={findLow || undefined}
          style={
            inspected
              ? {
                  right: `calc(min(${panelWidth}px, 100% - 120px) + 10px)`,
                  maxWidth: `calc(100% - min(${panelWidth}px, 100% - 120px) - 20px)`,
                }
              : undefined
          }
        >
          <Icon name="search" size={14} />
          <input
            ref={findInputRef}
            className="page-find-input"
            autoFocus
            placeholder={t("erd.canvas.findPh")}
            value={findQuery}
            onChange={(e) => {
              setFindQuery(e.target.value);
              runFind(e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.nativeEvent.isComposing) return;
              if (e.key === "Enter") stepFind(e.shiftKey ? -1 : 1);
              if (e.key === "Escape") {
                e.stopPropagation(); // 정보창과 패널을 닫는 Esc 까지 가지 않게
                closeFind();
              }
            }}
          />
          <span className="page-find-count">{findCount ? `${findIdx + 1}/${findCount}` : findQuery.trim() ? "0" : ""}</span>
          <button className="page-find-nav" aria-label={t("common.find.prev")} onClick={() => stepFind(-1, true)} disabled={!findCount}>
            <Icon name="chevron-left" size={14} />
          </button>
          <button className="page-find-nav" aria-label={t("common.find.next")} onClick={() => stepFind(1, true)} disabled={!findCount}>
            <Icon name="chevron-right" size={14} />
          </button>
          <button className="page-find-nav" aria-label={t("common.close")} onClick={closeFind}>
            <Icon name="x" size={14} />
          </button>
        </div>
      )}

      {busy && (
        <div className="erd-status">
          <Spinner />
          {t("erd.canvas.laying")}
        </div>
      )}
      {error && <div className="erd-status error-note">{error}</div>}

      <div className="erd-pop rz-details" ref={popRef} role="dialog" hidden={!popped} />

      {inspected && (
        <aside
          className="erd-panel"
          ref={panelRef}
          style={{ ["--erd-panel-w" as string]: `${panelWidth}px` }}
          aria-label={inspected}
        >
          <div
            className="erd-panel-resize"
            role="separator"
            aria-orientation="vertical"
            aria-label={t("common.resizePane")}
            onPointerDown={startResize}
          />
          <div className="erd-panel-head">
            <span className="erd-panel-label">{t("erd.canvas.table")}</span>
            <span className="erd-panel-title">{inspected}</span>
            <span className="spacer" />
            <Tooltip label={t("common.close")}>
              <button
                className="erd-float-btn"
                aria-label={t("common.close")}
                onClick={() => {
                  setInspected(null);
                  setPopped(null);
                  canvasRef.current?.focus?.clear();
                  canvasRef.current?.refresh();
                }}
              >
                <Icon name="x" size={15} />
              </button>
            </Tooltip>
          </div>
          <div className="erd-panel-body rz-details" ref={panelBodyRef} />
        </aside>
      )}
    </div>
  );
}
