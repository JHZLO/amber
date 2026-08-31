// 다이어그램 인라인 렌더 캔버스 — mermaid-studio.html 의 프리뷰와 동일한 동작.
// svg-pan-zoom 기반: 휠 줌 · 드래그 팬 · 더블클릭 줌인 · fit/center, 어떤 배율에서도 벡터 선명.
// 렌더마다 인스턴스를 재생성하고 fit (스튜디오와 동일). 문법 오류 시 마지막 정상 렌더 유지 + 에러 바.

import { useEffect, useRef, useState } from "react";
import svgPanZoom from "svg-pan-zoom";
import { renderMermaid } from "./Mermaid";
import { Icon } from "../icons";
import { Tooltip } from "../ui";
import { t } from "../lib/i18n";
import { errText } from "../lib/errors";
import {
  DIAGRAM_LAYOUTS,
  setDiagramLayout,
  useDiagramLayout,
} from "../lib/diagramLayout";
import {
  neighborsOf,
  nullabilityFlag,
  parseEdgeEndpoints,
  splitOptionalType,
} from "../lib/diagramGraph";

let seq = 0;

// 스튜디오와 동일: 클릭으로 선택 가능한 노드 셀렉터 (다이어그램 타입별)
const NODE_SELECTORS = [
  ".node", // flowchart, state, mindmap
  ".actor", // sequence
  ".classGroup", // class
  ".cluster", // subgraph
  ".er.entityBox", // er entity box
  ".entityLabel", // er entity label group
  ".task", // gantt task
  ".pieCircle", // pie
  ".quadrant", // quadrant
  'g[class*="mindmap-node"]',
].join(", ");

const escapeRegex = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** 노드의 표시 텍스트 (foreignObject 라벨 우선, 그다음 svg text) */
function getNodeText(node: Element): string {
  const fo = node.querySelector("foreignObject");
  if (fo?.textContent?.trim()) return fo.textContent.trim();
  const collected = Array.from(node.querySelectorAll("text, tspan"))
    .map((t) => t.textContent ?? "")
    .join(" ")
    .trim();
  return collected || node.getAttribute("id") || "";
}

/** mermaid 내부 prefix/suffix 를 벗긴 노드 id */
function extractNodeId(node: Element): string {
  const rawId = node.getAttribute("id") ?? "";
  if (!rawId) return "";
  return rawId
    .replace(/-\d+$/, "")
    .replace(
      /^(flowchart|graph|state|actor|classId|class|er|mindmap|note|gantt|pie|quadrant|sankey)-/i,
      "",
    );
}

/** 배율(%) — 0 크기 컨테이너에서 pan-zoom 이 NaN 을 뱉는 일이 있어 100 으로 눌러 둔다 */
const safePct = (level: number) =>
  Number.isFinite(level) && level > 0 ? Math.round(level * 100) : 100;

/** 렌더된 SVG 에서 읽어낸 연결 관계 (포커스 모드용) */
interface DiagramGraph {
  /** 노드 id(다이어그램 접두사 뗀 것) → 노드 g 엘리먼트 */
  nodes: Map<string, Element>;
  edges: {
    el: Element;
    /** 짝이 맞을 때만 — 라벨은 id 가 없어 순서로만 대응된다 */
    label: Element | null;
    source: string;
    target: string;
  }[];
}

/** mermaid 가 붙인 id 로 연결 관계를 복원한다. 못 읽으면 빈 그래프(포커스 비활성). */
function buildGraph(svgEl: SVGElement): DiagramGraph {
  const prefix = svgEl.id ? `${svgEl.id}-` : "";
  const nodes = new Map<string, Element>();
  for (const el of svgEl.querySelectorAll("g.node")) {
    const raw = el.getAttribute("id") ?? "";
    if (!raw.startsWith(prefix)) continue;
    nodes.set(raw.slice(prefix.length), el);
  }
  const paths = [...svgEl.querySelectorAll("g.edgePaths > path")];
  const labels = [...svgEl.querySelectorAll("g.edgeLabels > g.edgeLabel")];
  // 라벨엔 id 가 없다 — 개수가 같을 때만 순서로 짝지운다(어긋나면 라벨은 건드리지 않음)
  const pairable = labels.length === paths.length;
  const ids = [...nodes.keys()];
  const edges: DiagramGraph["edges"] = [];
  paths.forEach((el, i) => {
    const ends = parseEdgeEndpoints(el.getAttribute("data-id") ?? "", ids);
    if (ends)
      edges.push({ el, label: pairable ? labels[i] : null, ...ends });
  });
  return { nodes, edges };
}

/** 소스 코드에서 노드 id 가 등장하는 첫 줄 번호 (1-base, 없으면 -1) */
function findLineForNodeId(nodeId: string, code: string): number {
  if (!nodeId || !code) return -1;
  const re = new RegExp(
    `(^|[\\s\\[\\(\\{<>"|*&!\\->,;])${escapeRegex(nodeId)}([\\s\\[\\(\\{<>"|*&!\\->,;:]|$)`,
  );
  const lines = code.split("\n");
  for (let i = 0; i < lines.length; i++) {
    if (re.test(lines[i])) return i + 1;
  }
  return -1;
}

/** ER 엔티티 한 줄 (렌더된 라벨에서 그대로 읽는다) */
interface NodeColumn {
  /** `?` 는 떼어낸 순수 타입 — 널 여부는 flag 로 통일해 보여준다 */
  type: string;
  name: string;
  keys: string;
  /** `[NULL]`/`[NOTNULL]`. 설명 본문은 싣지 않는다(길이가 제각각이라 줄이 접힌다) */
  flag: string;
}

interface NodeSel {
  text: string;
  id: string;
  line: number;
  columns: NodeColumn[];
}

/** 라벨 클래스 → 모아 담을 칸. comment 는 널 표기만 뽑아 쓰므로 원문 그대로 받아 둔다. */
type RawField = "type" | "name" | "keys" | "comment";
const COLUMN_FIELD: Record<string, RawField> = {
  "attribute-type": "type",
  "attribute-name": "name",
  "attribute-keys": "keys",
  "attribute-comment": "comment",
};

/** ER 엔티티의 컬럼 목록. mermaid 원문을 다시 파싱하지 않고 **렌더 결과**를 읽는다 —
 *  엔티티 본문은 이미 `g.label.attribute-{type|name|keys|comment}` 로 쪼개져 있어서
 *  타입·이름·키·코멘트가 화면에 보이는 그대로 나온다(따옴표·별칭 파싱 걱정 없음).
 *
 *  한 줄이 항상 4칸은 아니다 — 코멘트가 하나도 없는 엔티티는 mermaid 가 코멘트 칸 자체를
 *  안 그린다. 그래서 개수를 가정하지 않고 **이미 채운 칸이 다시 나오면 다음 줄**로 넘긴다. */
function readColumns(nodeEl: Element): NodeColumn[] {
  const raw: Record<RawField, string>[] = [];
  let cur: Partial<Record<RawField, string>> = {};
  const flush = () => {
    if (Object.keys(cur).length === 0) return;
    raw.push({
      type: cur.type ?? "",
      name: cur.name ?? "",
      keys: cur.keys ?? "",
      comment: cur.comment ?? "",
    });
    cur = {};
  };
  for (const el of nodeEl.querySelectorAll("g.label")) {
    const cls = el.getAttribute("class") ?? "";
    // 엔티티 이름(g.label.name)은 attribute- 접두어가 없어 자연히 걸러진다
    const field = Object.entries(COLUMN_FIELD).find(([k]) => cls.includes(k))?.[1];
    if (!field) continue;
    if (field in cur) flush();
    cur[field] = el.textContent?.trim() ?? "";
  }
  flush();

  // `?` 규약을 쓰는 표인지 먼저 본다 — 그래야 나머지 컬럼을 NOT NULL 로 읽어도 된다
  const usesOptional = raw.some((r) => splitOptionalType(r.type).optional);
  return raw.map((r) => ({
    type: splitOptionalType(r.type).type,
    name: r.name,
    keys: r.keys,
    flag: nullabilityFlag(r.type, r.comment, usesOptional),
  }));
}

/** 스튜디오와 동일한 SVG 정규화: viewBox 보장 + 크기를 컨테이너에 맡김 */
function normalizeSvg(svgEl: SVGSVGElement) {
  if (!svgEl.getAttribute("viewBox")) {
    // mindmap/quadrant 등은 viewBox 없이 width/height 만 있는 경우가 있음
    let w = parseFloat(svgEl.getAttribute("width") ?? "") || 0;
    let h = parseFloat(svgEl.getAttribute("height") ?? "") || 0;
    if (!w || !h) {
      try {
        const bbox = svgEl.getBBox();
        w = bbox.width + Math.max(0, -bbox.x) * 2 + 40;
        h = bbox.height + Math.max(0, -bbox.y) * 2 + 40;
      } catch {
        /* 비표시 상태 등 getBBox 실패 무시 */
      }
    }
    if (!w || !h) {
      w = 1200;
      h = 800;
    }
    svgEl.setAttribute("viewBox", `0 0 ${w} ${h}`);
  }
  svgEl.removeAttribute("width");
  svgEl.removeAttribute("height");
  svgEl.setAttribute("preserveAspectRatio", "xMidYMid meet");
  svgEl.style.maxWidth = "none";
  svgEl.style.maxHeight = "none";
  svgEl.style.width = "100%";
  svgEl.style.height = "100%";
  svgEl.style.display = "block";
}

export function DiagramCanvas({
  chart,
  onJumpToLine,
}: {
  chart: string;
  /** 노드 정보 카드의 "라인 N" 클릭 시 (에디터 점프). 없으면 라인 버튼은 표시만 생략 */
  onJumpToLine?: (line: number) => void;
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  const pzRef = useRef<ReturnType<typeof svgPanZoom> | null>(null);
  const hoverRef = useRef(false);
  const [zoomPct, setZoomPct] = useState(100);
  const [error, setError] = useState<string | null>(null);
  const [hasSvg, setHasSvg] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const fullscreenRef = useRef(false);
  fullscreenRef.current = fullscreen;
  const layout = useDiagramLayout(); // 바뀌면 아래 렌더 effect 가 다시 돈다

  // 노드 선택 (스튜디오의 selectNode/deselectNode 대응)
  const selectedElRef = useRef<Element | null>(null);
  const [sel, setSel] = useState<NodeSel | null>(null);
  const chartRef = useRef(chart);
  chartRef.current = chart;
  const downPosRef = useRef<{ x: number; y: number } | null>(null);

  // 컬럼 목록 펼침 — 노드를 바꿔 고르면 접힌 상태로 돌아간다
  const [showCols, setShowCols] = useState(false);
  // 검색 — 큰 ERD 에서 테이블·컬럼 이름을 눈으로 훑는 건 사실상 불가능하다.
  // 렌더된 SVG 의 text 노드를 그대로 뒤진다(원본 mermaid 소스가 아니라): 화면에 보이는
  // 문자열과 검색 대상이 정확히 같아야 "분명 있는데 안 잡힌다"가 생기지 않는다.
  const [searchOpen, setSearchOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [hitIdx, setHitIdx] = useState(0);
  const [hitCount, setHitCount] = useState(0);
  const hitsRef = useRef<SVGGraphicsElement[]>([]);
  const searchRef = useRef<HTMLInputElement>(null);
  // 이름 복사 피드백 — 리포트 복사 버튼과 같은 문법(1.5s 뒤 원래대로)
  const [copied, setCopied] = useState(false);
  const copyTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (copyTimer.current) clearTimeout(copyTimer.current);
    },
    [],
  );
  async function copyName(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      if (copyTimer.current) clearTimeout(copyTimer.current);
      copyTimer.current = setTimeout(() => setCopied(false), 1500);
    } catch {
      /* 클립보드 실패는 조용히 무시 */
    }
  }

  // 포커스 모드: 고른 노드 + 직접 연결된 노드/엣지만 남기고 나머지를 죽인다.
  const graphRef = useRef<DiagramGraph | null>(null);

  /** 클릭된 엘리먼트에서 그래프가 아는 노드 id 로 거슬러 올라간다
   *  (클릭 대상이 g.node 가 아니라 안쪽 rect/label 일 수 있다) */
  function graphIdOf(el: Element): string | null {
    const g = graphRef.current;
    if (!g) return null;
    for (const [id, nodeEl] of g.nodes)
      if (nodeEl === el || nodeEl.contains(el)) return id;
    return null;
  }

  function applyFocus(nodeId: string | null) {
    const g = graphRef.current;
    const svgEl = hostRef.current?.querySelector("svg");
    if (!g || !svgEl) return;
    // 연결 정보를 못 읽었으면 아무것도 흐리게 하지 않는다 — 전부 죽어버리는 것보단 낫다
    if (!nodeId || g.edges.length === 0) {
      svgEl.classList.remove("dgm-focused");
      for (const el of svgEl.querySelectorAll(".dgm-rel"))
        el.classList.remove("dgm-rel");
      return;
    }
    const keep = neighborsOf(nodeId, g.edges);
    keep.add(nodeId);
    for (const [id, el] of g.nodes) el.classList.toggle("dgm-rel", keep.has(id));
    for (const e of g.edges) {
      const on = e.source === nodeId || e.target === nodeId;
      e.el.classList.toggle("dgm-rel", on);
      e.label?.classList.toggle("dgm-rel", on);
    }
    svgEl.classList.add("dgm-focused");
  }

  const deselectNode = () => {
    selectedElRef.current?.classList.remove("node-selected");
    selectedElRef.current = null;
    setSel(null);
    setCopied(false); // 다른 노드를 골랐는데 '복사됨'이 남아 있으면 거짓말이 된다
    setShowCols(false);
    applyFocus(null);
  };

  function selectNode(node: Element) {
    if (selectedElRef.current === node) {
      deselectNode(); // 같은 노드 재클릭 = 토글 해제
      return;
    }
    setCopied(false);
    setShowCols(false); // 다른 테이블의 컬럼이 펼쳐진 채로 남지 않게
    selectedElRef.current?.classList.remove("node-selected");
    selectedElRef.current = node;
    node.classList.add("node-selected");
    applyFocus(graphIdOf(node));
    const text = getNodeText(node);
    const id = extractNodeId(node);
    let line = id ? findLineForNodeId(id, chartRef.current) : -1;
    if (line < 1 && text && text !== id)
      line = findLineForNodeId(text, chartRef.current);
    setSel({
      text: text || t("diagrams.node.unnamed"),
      id,
      line,
      // 컬럼은 g.node 기준으로 읽는다 — 클릭 지점이 안쪽 rect 일 수 있다
      columns: readColumns(node.closest("g.node") ?? node),
    });
  }

  useEffect(() => {
    let alive = true;
    const id = `dgmc-${(seq += 1)}`;
    renderMermaid(id, chart)
      .then(({ svg }) => {
        if (!alive || !hostRef.current) return;
        // 스튜디오와 동일: 렌더마다 파괴 후 재생성 + fit (선택도 초기화)
        try {
          pzRef.current?.destroy();
        } catch {
          /* noop */
        }
        pzRef.current = null;
        selectedElRef.current = null;
        setSel(null);
        graphRef.current = null;
        hostRef.current.innerHTML = svg;
        const svgEl = hostRef.current.querySelector("svg");
        if (!svgEl) return;
        normalizeSvg(svgEl);
        graphRef.current = buildGraph(svgEl); // 포커스 모드용 연결 관계
        // 클릭 = 노드 선택 토글 (드래그 팬 후에는 무시 — 4px 이동 가드)
        svgEl.addEventListener("click", (e) => {
          const down = downPosRef.current;
          if (
            down &&
            Math.abs(e.clientX - down.x) + Math.abs(e.clientY - down.y) > 4
          )
            return;
          const target = e.target as Element | null;
          const node = target?.closest?.(NODE_SELECTORS);
          if (node) {
            e.stopPropagation();
            selectNode(node);
          } else {
            deselectNode();
          }
        });
        try {
          pzRef.current = svgPanZoom(svgEl, {
            zoomEnabled: true,
            panEnabled: true,
            controlIconsEnabled: false,
            fit: true,
            center: true,
            contain: false,
            minZoom: 0.1,
            maxZoom: 20,
            zoomScaleSensitivity: 0.4,
            dblClickZoomEnabled: true,
            mouseWheelZoomEnabled: true,
            preventMouseEventsDefault: true,
            onZoom: (level: number) => setZoomPct(safePct(level)),
          });
          setZoomPct(safePct(pzRef.current.getZoom()));
        } catch {
          /* 0 크기 컨테이너 등 초기화 실패 — svg 는 그대로 보임 */
        }
        setHasSvg(true);
        setError(null);
      })
      .catch((e: unknown) => {
        // 마지막 정상 렌더 유지, 에러 메시지만 표시 (스튜디오의 에러 패널 대응)
        if (alive) setError(errText(e));
      });
    return () => {
      alive = false;
    };
  }, [chart, layout]);

  // 언마운트 시 pan-zoom 정리
  useEffect(
    () => () => {
      try {
        pzRef.current?.destroy();
      } catch {
        /* noop */
      }
    },
    [],
  );

  // 전체화면 토글 시 컨테이너 크기가 바뀌므로 pan-zoom 을 다시 맞춘다(레이아웃 반영 후 2프레임 뒤).
  useEffect(() => {
    if (!pzRef.current) return;
    const raf = requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        pzRef.current?.resize();
        pzRef.current?.fit();
        pzRef.current?.center();
      }),
    );
    return () => cancelAnimationFrame(raf);
  }, [fullscreen]);

  // 스튜디오 단축키: + / - / 1(100%) / 0(맞춤) — 캔버스에 마우스가 있을 때만.
  // Esc 는 전체화면을 먼저 닫는다(호버 여부와 무관).
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable))
        return;
      if (e.key === "Escape" && fullscreenRef.current) {
        setFullscreen(false);
        e.preventDefault();
        return;
      }
      if (!hoverRef.current || !pzRef.current) return;
      // '/' 로 검색 열기 — 입력창에 포커스가 있으면 그냥 글자다
      if (e.key === "/") {
        setSearchOpen(true);
        requestAnimationFrame(() => searchRef.current?.focus());
        e.preventDefault();
        return;
      }
      if (e.key === "+" || e.key === "=") pzRef.current.zoomIn();
      else if (e.key === "-") pzRef.current.zoomOut();
      else if (e.key === "1") {
        pzRef.current.resetZoom();
        pzRef.current.center();
      } else if (e.key === "0") {
        pzRef.current.resize();
        pzRef.current.fit();
        pzRef.current.center();
      } else if (e.key === "Escape") {
        // 모달/줌뷰어가 떠 있으면 그쪽 우선
        if (document.querySelector(".overlay, .mmd-zoom-overlay")) return;
        if (!selectedElRef.current) return;
        deselectNode();
      } else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  /** 검색어에 걸리는 SVG text 노드를 모아 표시하고, 첫 결과로 이동한다.
   *  칠하는 건 text 하나가 아니라 그 text 를 품은 그룹(테이블 상자)까지다 — 컬럼 하나가
   *  걸렸을 때 어느 테이블 안인지 같이 보여야 검색이 쓸모 있다. */
  function runSearch(q: string) {
    const host = hostRef.current;
    if (!host) return;
    for (const el of Array.from(host.querySelectorAll(".dgm-hit, .dgm-hit-box, .dgm-hit-cur"))) {
      el.classList.remove("dgm-hit", "dgm-hit-box", "dgm-hit-cur");
    }
    const term = q.trim().toLowerCase();
    if (!term) {
      hitsRef.current = [];
      setHitCount(0);
      setHitIdx(0);
      return;
    }
    const hits: SVGGraphicsElement[] = [];
    for (const el of Array.from(host.querySelectorAll<SVGGraphicsElement>("svg text"))) {
      if (!(el.textContent ?? "").toLowerCase().includes(term)) continue;
      el.classList.add("dgm-hit");
      // 상자까지 표시 — 조상 g 안의 rect 에 **직접** 클래스를 건다.
      // CSS 자식 선택자로 잡으면 mermaid 가 rect 를 한 겹 더 감쌌을 때 조용히 안 그려진다.
      const rect = el.closest("g")?.querySelector("rect");
      rect?.classList.add("dgm-hit-box");
      hits.push(el);
    }
    hitsRef.current = hits;
    setHitCount(hits.length);
    setHitIdx(0);
    if (hits.length) focusHit(0);
  }

  /** 결과 하나를 화면 중앙으로. 화면 좌표로 계산해 현재 줌·팬 상태와 무관하게 맞는다. */
  function focusHit(i: number) {
    const hits = hitsRef.current;
    const pz = pzRef.current;
    const host = hostRef.current;
    if (!hits.length || !pz || !host) return;
    const el = hits[((i % hits.length) + hits.length) % hits.length];
    for (const h of hits) h.classList.remove("dgm-hit-cur");
    el.classList.add("dgm-hit-cur");
    const r = el.getBoundingClientRect();
    const h = host.getBoundingClientRect();
    pz.panBy({
      x: h.left + h.width / 2 - (r.left + r.width / 2),
      y: h.top + h.height / 2 - (r.top + r.height / 2),
    });
  }

  function stepHit(delta: number) {
    if (!hitsRef.current.length) return;
    const next =
      (((hitIdx + delta) % hitsRef.current.length) + hitsRef.current.length) %
      hitsRef.current.length;
    setHitIdx(next);
    focusHit(next);
  }

  function closeSearch() {
    setSearchOpen(false);
    setQuery("");
    runSearch("");
  }

  const zoomIn = () => pzRef.current?.zoomIn();
  const zoomOut = () => pzRef.current?.zoomOut();
  const reset100 = () => {
    pzRef.current?.resetZoom();
    pzRef.current?.center();
  };
  const fit = () => {
    pzRef.current?.resize();
    pzRef.current?.fit();
    pzRef.current?.center();
  };

  return (
    <div
      className={`dgm-canvas-wrap ${fullscreen ? "fullscreen" : ""}`}
      onMouseEnter={() => {
        hoverRef.current = true;
      }}
      onMouseLeave={() => {
        hoverRef.current = false;
      }}
    >
      <div
        ref={hostRef}
        className="dgm-canvas"
        onMouseDown={(e) => {
          downPosRef.current = { x: e.clientX, y: e.clientY };
        }}
      />

      {/* 검색 바 — 툴바 바로 아래. 결과 개수와 이동이 한 줄에 있어야 훑는 흐름이 안 끊긴다 */}
      {searchOpen && (
        <div className="dgm-search">
          <Icon name="search" size={14} />
          <input
            ref={searchRef}
            className="dgm-search-input"
            placeholder={t("diagrams.search.ph")}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              runSearch(e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.nativeEvent.isComposing) return;
              if (e.key === "Enter") stepHit(e.shiftKey ? -1 : 1);
              if (e.key === "Escape") closeSearch();
            }}
          />
          <span className="dgm-search-count">
            {hitCount ? `${hitIdx + 1}/${hitCount}` : query.trim() ? "0" : ""}
          </span>
          <button
            className="dgm-search-nav"
            aria-label={t("diagrams.search.prev")}
            onClick={() => stepHit(-1)}
            disabled={!hitCount}
          >
            <Icon name="chevron-left" size={14} />
          </button>
          <button
            className="dgm-search-nav"
            aria-label={t("diagrams.search.next")}
            onClick={() => stepHit(1)}
            disabled={!hitCount}
          >
            <Icon name="chevron-right" size={14} />
          </button>
          <button
            className="dgm-search-nav"
            aria-label={t("common.close")}
            onClick={closeSearch}
          >
            <Icon name="x" size={14} />
          </button>
        </div>
      )}

      {/* 우상단 — 화면 조작. 캔버스 위에 떠 있어 그림 폭을 잡아먹지 않는다.
          아이콘 뜻: 네 귀퉁이 브래킷 = 프레임에 담는다(맞춤), 대각 화살표 = 밖으로 펼친다(전체화면).
          네이티브 title 은 WKWebView 에서 안 뜨므로 공용 Tooltip 으로 감싼다. */}
      <div className="dgm-float dgm-float-tr">
        <Tooltip label={`${t("diagrams.search.open")} (/)`}>
          <button
            className={`dgm-float-btn ${searchOpen ? "on" : ""}`}
            aria-label={t("diagrams.search.open")}
            onClick={() => {
              if (searchOpen) closeSearch();
              else {
                setSearchOpen(true);
                requestAnimationFrame(() => searchRef.current?.focus());
              }
            }}
          >
            <Icon name="search" size={16} />
          </button>
        </Tooltip>
        <Tooltip label={`${t("diagrams.zoom.fitTitle")} (0)`}>
          <button
            className="dgm-float-btn"
            aria-label={t("diagrams.zoom.fitTitle")}
            onClick={fit}
          >
            <Icon name="fit-screen" size={16} />
          </button>
        </Tooltip>
        <Tooltip label={`${t("diagrams.zoom.out")} (-)`}>
          <button
            className="dgm-float-btn"
            aria-label={t("diagrams.zoom.out")}
            onClick={zoomOut}
          >
            <Icon name="zoom-out" size={16} />
          </button>
        </Tooltip>
        {/* 배율 자체가 100% 복귀 버튼 — 레퍼런스엔 없지만 읽고 되돌리는 걸 한 자리에서 */}
        <Tooltip label={`100% (1)`}>
          <button
            className="dgm-float-btn dgm-float-pct"
            aria-label={`100%`}
            onClick={reset100}
          >
            {zoomPct}%
          </button>
        </Tooltip>
        <Tooltip label={`${t("diagrams.zoom.in")} (+)`}>
          <button
            className="dgm-float-btn"
            aria-label={t("diagrams.zoom.in")}
            onClick={zoomIn}
          >
            <Icon name="zoom-in" size={16} />
          </button>
        </Tooltip>
        <Tooltip
          label={
            fullscreen
              ? t("diagrams.canvas.fullscreenClose")
              : t("diagrams.canvas.fullscreen")
          }
        >
          <button
            className="dgm-float-btn"
            aria-label={
              fullscreen
                ? t("diagrams.canvas.fullscreenClose")
                : t("diagrams.canvas.fullscreen")
            }
            onClick={() => setFullscreen((f) => !f)}
          >
            <Icon name={fullscreen ? "x" : "expand"} size={16} />
          </button>
        </Tooltip>
      </div>

      {/* 좌하단 — 레이아웃 엔진. 선택은 앱 전역에 남는다 */}
      <div className="dgm-float dgm-float-bl">
        {DIAGRAM_LAYOUTS.map((l) => (
          <Tooltip key={l} label={t(`diagrams.layout.${l}.hint`)}>
            <button
              className={`dgm-float-btn dgm-float-tab ${layout === l ? "active" : ""}`}
              onClick={() => setDiagramLayout(l)}
            >
              {t(`diagrams.layout.${l}`)}
            </button>
          </Tooltip>
        ))}
      </div>
      {!hasSvg && !error && (
        <div className="dgm-canvas-empty">{t("diagrams.canvas.rendering")}</div>
      )}
      {/* 노드 선택 정보 카드 (스튜디오의 node-info-card 대응) */}
      {sel && (
        <div className="dgm-node-info">
          <div className="dgm-node-info-body">
            {/* 윗줄 = 이름 + 컬럼 버튼. 컬럼 버튼을 아래 meta 줄에 두면 긴 id 에 밀려
                줄바꿈이 나므로 여기로 올린다 — 길이가 변하는 건 id 뿐이라야 한다. */}
            <div className="dgm-node-info-top">
              {/* 이름 = 클릭하면 클립보드로. 힌트('복사')는 자리를 잡아둔 채 흐리게 늘 떠 있고
                  hover 에서 또렷해진다 — 나타났다 사라지면 카드 폭이 흔들린다(§9.2) */}
              <button
                className={`dgm-node-info-name${copied ? " copied" : ""}`}
                onClick={() => void copyName(sel.text)}
                aria-label={`${t("diagrams.node.copyName")}: ${sel.text}`}
              >
                <span className="dgm-node-info-text">{sel.text}</span>
                <span className="dgm-node-info-hint" aria-hidden="true">
                  <Icon name={copied ? "check" : "copy"} size={12} />
                  {copied ? t("diagrams.node.copied") : t("diagrams.copy")}
                </span>
              </button>
              {sel.columns.length > 0 && (
                <button
                  className={`btn btn-sm dgm-cols-toggle${showCols ? " active" : ""}`}
                  aria-expanded={showCols}
                  onClick={() => setShowCols((v) => !v)}
                >
                  <Icon name="layers" size={13} />
                  {t("diagrams.node.columns", { n: sel.columns.length })}
                </button>
              )}
            </div>
            <div className="dgm-node-info-meta">
              {sel.id && sel.id !== sel.text && (
                <span className="dgm-node-info-id">{sel.id}</span>
              )}
              {sel.line > 0 && onJumpToLine && (
                <button
                  className="btn btn-sm"
                  onClick={() => onJumpToLine(sel.line)}
                >
                  {t("diagrams.node.line", { n: sel.line })}
                </button>
              )}
            </div>
          </div>
          <Tooltip label={t("diagrams.node.deselect")}>
            <button
              aria-label={t("diagrams.node.deselect")}
              className="icon-btn ghost sm"
              onClick={deselectNode}
            >
              <Icon name="x" size={14} />
            </button>
          </Tooltip>
        </div>
      )}

      {/* 컬럼 패널 — 우측에서 밀려 들어온다. 줌 플로팅 아래에서 시작하고, 아래쪽은
          정보 카드와 겹치지 않게 높이를 잘라 둔다. */}
      {showCols && sel && sel.columns.length > 0 && (
        <div className="dgm-cols" role="dialog" aria-label={sel.text}>
          <div className="dgm-cols-head">
            <div className="dgm-cols-title">
              <span className="dgm-cols-name">{sel.text}</span>
              <span className="dgm-cols-count">
                {t("diagrams.node.columns", { n: sel.columns.length })}
              </span>
            </div>
            <button
              className="icon-btn ghost sm"
              aria-label={t("common.close")}
              onClick={() => setShowCols(false)}
            >
              <Icon name="x" size={14} />
            </button>
          </div>
          {/* 긁어서 복사하라고 만든 목록이라 선택을 살려 둔다.
              칸 사이의 {" "} 는 장식이 아니다 — CSS 마진은 복사한 텍스트에 남지 않아
              이게 없으면 `id bigint PK` 가 `idbigintPK` 로 붙어 나온다(실측).
              코멘트가 길어 줄이 접히는 건 상관없다: soft wrap 은 개행을 넣지 않아
              **한 컬럼이 한 줄**로 복사된다. */}
          <div className="dgm-cols-body">
            {sel.columns.map((c, i) => (
              <div className="dgm-col-row" key={`${c.name}-${i}`}>
                {/* 칸마다 고정 폭 inline-block — 블록으로 쪼개면 복사할 때 줄이 갈라지고,
                    폭을 안 주면 칸이 들쭉날쭉해진다.
                    칸 사이 공백은 반드시 셀 **바깥**에 둔다 — 셀 안 끝에 두면 inline-block
                    줄 끝이라 브라우저가 지워버려 `PK[NOTNULL]` 로 붙는다(실측).
                    키가 없는 줄은 공백이 하나 더 들어가지만, 그래야 칸 위치가 어긋나지 않는다. */}
                <span className="dgm-col-name">{c.name}</span>{" "}
                <span className="dgm-col-type">{c.type}</span>{" "}
                <span className="dgm-col-keycell">
                  {c.keys && <span className="dgm-col-keys">{c.keys}</span>}
                </span>{" "}
                {c.flag && <span className="dgm-col-flag">{c.flag}</span>}
              </div>
            ))}
          </div>
        </div>
      )}
      {error && (
        <div className="dgm-error">
          <span className="dgm-error-msg">{error}</span>
          <button
            className="btn btn-sm"
            onClick={() => void navigator.clipboard.writeText(error)}
          >
            {t("diagrams.copy")}
          </button>
        </div>
      )}
    </div>
  );
}
