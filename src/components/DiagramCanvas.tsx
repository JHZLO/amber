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
import { neighborsOf, parseEdgeEndpoints } from "../lib/diagramGraph";

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

// 포커스된 엔티티 테두리에 쓰는 이리데슨트 스펙트럼.
// 캔버스는 mermaid 밝은 테마 고정이라(DESIGN.md §2 의 mermaid 캔버스 예외) 밝은 배경 기준.
// 원색·형광·노랑 계열은 뺐다 — 유막(oil slick)처럼 명도가 고른 한 벌이라야 테두리가
// 조각조각 튀지 않고 한 줄기로 읽힌다.
const IRIS_STOPS = [
  "#6B5FA7", // indigo
  "#4C7FB0", // steel blue
  "#3D8F9E", // teal
  "#8A7BB8", // periwinkle
  "#9C6F97", // plum
];
/** 그라디언트 한 주기 길이(user unit) — 캔버스를 가로지르는 색 띠의 폭 */
const IRIS_SPAN = 620;

/** 빛 방향 (feDistantLight). y 축이 아래로 향하는 필터 좌표계 기준 — 왼쪽 위에서 비춘다. */
const LIGHT_AZIMUTH = 235;
const LIGHT_ELEVATION = 58;

/** 테두리에 쓸 그라디언트와 베벨 필터를 SVG 안에 심고, url() 을 CSS 변수로 노출한다.
 *  id 는 렌더마다 달라지므로 CSS 가 하드코딩할 수 없어 변수로 건넨다. */
export function injectIrisDefs(svgEl: SVGElement): void {
  const NS = "http://www.w3.org/2000/svg";
  const base = svgEl.id || "dgm";
  let defs = svgEl.querySelector("defs");
  if (!defs) {
    defs = document.createElementNS(NS, "defs");
    svgEl.insertBefore(defs, svgEl.firstChild);
  }

  // ── 색: 캔버스 전체를 가로지르는 하나의 띠(userSpaceOnUse).
  // 테이블마다 따로 무지개를 돌리면 제각각 놀아서 산만하다. 같은 띠를 공유하면
  // 테두리를 도는 빛이 위치에 따라 색을 바꿔 물고 간다.
  const gid = `${base}-iris`;
  const grad = document.createElementNS(NS, "linearGradient");
  grad.setAttribute("id", gid);
  grad.setAttribute("gradientUnits", "userSpaceOnUse");
  grad.setAttribute("spreadMethod", "repeat");
  grad.setAttribute("x1", "0");
  grad.setAttribute("y1", "0");
  grad.setAttribute("x2", String(IRIS_SPAN));
  grad.setAttribute("y2", String(IRIS_SPAN * 0.35));
  // 첫 색을 끝에 한 번 더 찍어야 주기가 매끄럽게 이어진다
  const stops = [...IRIS_STOPS, IRIS_STOPS[0]];
  stops.forEach((color, i) => {
    const stop = document.createElementNS(NS, "stop");
    stop.setAttribute("offset", `${(i / (stops.length - 1)) * 100}%`);
    stop.setAttribute("stop-color", color);
    grad.appendChild(stop);
  });
  defs.appendChild(grad);

  // ── 입체: 알파를 살짝 흐려 높이맵으로 삼고 한 방향에서 조명 → 빛을 마주한 모서리만
  // 하얗게 서는 베벨. diffuse 그늘까지 얹어 봤더니 색을 먹어 회색으로 죽길래
  // specular(하이라이트) 한 겹만 더한다 — 색은 그대로 두고 입체만 얻는다.
  const fid = `${base}-bevel`;
  defs.insertAdjacentHTML(
    "beforeend",
    `<filter id="${fid}" x="-25%" y="-25%" width="150%" height="150%"
             color-interpolation-filters="sRGB">
       <feGaussianBlur in="SourceAlpha" stdDeviation="0.6" result="h"/>
       <feSpecularLighting in="h" surfaceScale="5" specularConstant="1.15"
                           specularExponent="11" lighting-color="#ffffff" result="sp">
         <feDistantLight azimuth="${LIGHT_AZIMUTH}" elevation="${LIGHT_ELEVATION}"/>
       </feSpecularLighting>
       <feComposite in="sp" in2="SourceAlpha" operator="in" result="spCut"/>
       <feComposite in="SourceGraphic" in2="spCut" operator="arithmetic"
                    k1="0" k2="1" k3="1" k4="0"/>
     </filter>`,
  );

  svgEl.style.setProperty("--dgm-iris", `url(#${gid})`);
  svgEl.style.setProperty("--dgm-bevel", `url(#${fid})`);
}

/** 시계방향(화면 기준: 위→오른쪽→아래→왼쪽)으로 한 바퀴 도는 닫힌 사각 경로 */
function clockwiseRectPath(
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
): string {
  const rr = Math.min(r, w / 2, h / 2);
  return [
    `M${x + rr} ${y}`,
    `H${x + w - rr}`,
    `A${rr} ${rr} 0 0 1 ${x + w} ${y + rr}`,
    `V${y + h - rr}`,
    `A${rr} ${rr} 0 0 1 ${x + w - rr} ${y + h}`,
    `H${x + rr}`,
    `A${rr} ${rr} 0 0 1 ${x} ${y + h - rr}`,
    `V${y + rr}`,
    `A${rr} ${rr} 0 0 1 ${x + rr} ${y}`,
    "Z",
  ].join(" ");
}

/** 테두리 위를 도는 "빛 반사" 오버레이를 깔아둔다(보이기는 CSS 가 결정).
 *
 *  테두리 자체는 끊지 않는다 — 색은 한 바퀴 다 채워져 있고, 그 위를 미끄러지는 건
 *  하이라이트뿐이다. 그래서 dash 는 바탕이 아니라 이 오버레이에만 쓴다.
 *  넓고 옅은 것 + 좁고 밝은 것 두 겹을 겹쳐 번지는 반사처럼 보이게 한다.
 *
 *  mermaid 의 윤곽선을 복제하지 않는 이유: ER 의 g.outer-path 는 변마다 끊긴
 *  **subpath 8개**(중복 포함, Z 도 없음)라 dash 가 한 바퀴 도는 게 아니라 조각 사이를
 *  건너뛴다. bbox 로 시계방향 닫힌 사각형을 직접 그려야 진행 방향까지 우리가 정한다. */
export function addGlintOverlays(svgEl: SVGElement): void {
  const NS = "http://www.w3.org/2000/svg";
  for (const node of svgEl.querySelectorAll("g.node")) {
    // ER 엔티티는 g.outer-path 안 둘째 path 가 윤곽선(첫째는 면)
    const border = node.querySelector<SVGPathElement>(
      "g.outer-path > path:last-of-type",
    );
    if (!border) continue;
    border.classList.add("dgm-border");
    let box: DOMRect | null = null;
    try {
      box = border.getBBox();
    } catch {
      /* 지오메트리를 못 재면 하이라이트 없이 정적 테두리만 */
    }
    if (!box || box.width <= 0 || box.height <= 0) continue;

    const d = clockwiseRectPath(box.x, box.y, box.width, box.height, 2);
    let len = 0;
    for (const cls of ["dgm-glint", "dgm-glint-core"]) {
      const p = document.createElementNS(NS, "path") as SVGPathElement;
      p.setAttribute("class", cls);
      p.setAttribute("d", d);
      // 붙인 뒤에 재야 한다 — 떨어져 있는 엘리먼트는 엔진에 따라 길이를 못 준다
      border.parentElement?.appendChild(p);
      len = len || p.getTotalLength() || 2 * (box.width + box.height);
      p.style.setProperty("--dgm-len", String(len));
    }
  }
}

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

interface NodeSel {
  text: string;
  id: string;
  line: number;
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
    applyFocus(null);
  };

  function selectNode(node: Element) {
    if (selectedElRef.current === node) {
      deselectNode(); // 같은 노드 재클릭 = 토글 해제
      return;
    }
    setCopied(false);
    selectedElRef.current?.classList.remove("node-selected");
    selectedElRef.current = node;
    node.classList.add("node-selected");
    applyFocus(graphIdOf(node));
    const text = getNodeText(node);
    const id = extractNodeId(node);
    let line = id ? findLineForNodeId(id, chartRef.current) : -1;
    if (line < 1 && text && text !== id)
      line = findLineForNodeId(text, chartRef.current);
    setSel({ text: text || t("diagrams.node.unnamed"), id, line });
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
        injectIrisDefs(svgEl);
        addGlintOverlays(svgEl); // 움직임은 CSS 라 prefers-reduced-motion 이 그대로 먹는다
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
            onZoom: (level: number) => setZoomPct(Math.round(level * 100)),
          });
          setZoomPct(Math.round(pzRef.current.getZoom() * 100));
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
      <div className="dgm-toolbar">
        <span className="dgm-zoom-pct">{zoomPct}%</span>
        <span className="dgm-toolbar-hint">{t("diagrams.canvas.hint")}</span>
        <span className="spacer" />
        {/* 레이아웃 엔진 — 두 배치를 오가며 볼 수 있게. 선택은 앱 전역에 남는다.
            네이티브 title 은 WKWebView 에서 안 뜨므로 공용 Tooltip 으로 감싼다 */}
        <Tooltip label={t("diagrams.layout.hint")}>
          <div className="segmented dgm-layout-seg">
            {DIAGRAM_LAYOUTS.map((l) => (
              <button
                key={l}
                className={`tab ${layout === l ? "active" : ""}`}
                onClick={() => setDiagramLayout(l)}
              >
                {t(`diagrams.layout.${l}`)}
              </button>
            ))}
          </div>
        </Tooltip>
        <button
          className="icon-btn ghost sm"
          title={`${t("diagrams.zoom.in")} (+)`}
          onClick={zoomIn}
        >
          <Icon name="plus" size={15} />
        </button>
        <button
          className="icon-btn ghost sm"
          title={`${t("diagrams.zoom.out")} (-)`}
          onClick={zoomOut}
        >
          <Icon name="minus" size={15} />
        </button>
        <button className="btn btn-sm" title="100% (1)" onClick={reset100}>
          100%
        </button>
        <button
          className="btn btn-sm"
          title={`${t("diagrams.zoom.fitTitle")} (0)`}
          onClick={fit}
        >
          {t("diagrams.zoom.fit")}
        </button>
        <button
          className="icon-btn ghost sm"
          title={
            fullscreen
              ? t("diagrams.canvas.fullscreenClose")
              : t("diagrams.canvas.fullscreen")
          }
          onClick={() => setFullscreen((f) => !f)}
        >
          <Icon name={fullscreen ? "x" : "expand"} size={15} />
        </button>
      </div>
      <div
        ref={hostRef}
        className="dgm-canvas"
        onMouseDown={(e) => {
          downPosRef.current = { x: e.clientX, y: e.clientY };
        }}
      />
      {!hasSvg && !error && (
        <div className="dgm-canvas-empty">{t("diagrams.canvas.rendering")}</div>
      )}
      {/* 노드 선택 정보 카드 (스튜디오의 node-info-card 대응) */}
      {sel && (
        <div className="dgm-node-info">
          <div className="dgm-node-info-body">
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
          <button
            className="icon-btn ghost sm"
            title={t("diagrams.node.deselect")}
            onClick={deselectNode}
          >
            <Icon name="x" size={14} />
          </button>
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
