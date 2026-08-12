// 다이어그램 인라인 렌더 캔버스 — mermaid-studio.html 의 프리뷰와 동일한 동작.
// svg-pan-zoom 기반: 휠 줌 · 드래그 팬 · 더블클릭 줌인 · fit/center, 어떤 배율에서도 벡터 선명.
// 렌더마다 인스턴스를 재생성하고 fit (스튜디오와 동일). 문법 오류 시 마지막 정상 렌더 유지 + 에러 바.

import { useEffect, useRef, useState } from "react";
import svgPanZoom from "svg-pan-zoom";
import { renderMermaid } from "./Mermaid";
import { Icon } from "../icons";
import { t } from "../lib/i18n";
import { errText } from "../lib/errors";

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

  const deselectNode = () => {
    selectedElRef.current?.classList.remove("node-selected");
    selectedElRef.current = null;
    setSel(null);
    setCopied(false); // 다른 노드를 골랐는데 '복사됨'이 남아 있으면 거짓말이 된다
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
        hostRef.current.innerHTML = svg;
        const svgEl = hostRef.current.querySelector("svg");
        if (!svgEl) return;
        normalizeSvg(svgEl);
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
  }, [chart]);

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
