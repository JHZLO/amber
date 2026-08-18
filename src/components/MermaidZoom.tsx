// mermaid 다이어그램 확대 뷰어: 가운데 카드 모달 + 휠 줌(커서 기준) + 드래그 팬.
// 최대 120% 로 제한하므로 CSS transform(translate+scale)로 처리한다 — 레이아웃 재계산이 없어
// 매끄럽고 깜빡임이 없다(저배율이라 transform 업스케일 흐림·텍스처 한계 문제도 없음).

import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Icon } from "../icons";
import { Tooltip } from "../ui";
import { t } from "../lib/i18n";

const MIN = 0.2;
const MAX = 1.2; // 최대 120% 까지만 확대
const clamp = (v: number, a: number, b: number) => Math.min(Math.max(v, a), b);

export function MermaidZoom({
  svg,
  open,
  onClose,
}: {
  svg: string;
  open: boolean;
  onClose: () => void;
}) {
  const canvasRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(1);
  const [tx, setTx] = useState(0);
  const [ty, setTy] = useState(0);

  // 최신값 ref (네이티브/rAF 핸들러의 stale 방지)
  const s = useRef(1);
  s.current = scale;
  const x = useRef(0);
  x.current = tx;
  const y = useRef(0);
  y.current = ty;
  const drag = useRef<{ x: number; y: number } | null>(null);
  const nat = useRef<{ w: number; h: number } | null>(null);

  const reset = useCallback(() => {
    const canvas = canvasRef.current;
    setTx(0);
    setTy(0);
    if (canvas && nat.current) {
      setScale(
        clamp(
          Math.min(
            canvas.clientWidth / nat.current.w,
            canvas.clientHeight / nat.current.h,
          ) * 0.9,
          MIN,
          MAX,
        ),
      );
    } else setScale(1);
  }, []);

  // 열 때: SVG 자연 치수로 크기 고정 + 캔버스에 맞는 초기 배율(fit)
  useEffect(() => {
    if (!open) return;
    setTx(0);
    setTy(0);
    nat.current = null;
    const id = requestAnimationFrame(() => {
      const canvas = canvasRef.current;
      const el = contentRef.current?.querySelector<SVGSVGElement>("svg");
      if (!canvas || !el) return;
      const vb = el.getAttribute("viewBox");
      const p = vb ? vb.split(/[\s,]+/).map(Number) : [];
      let w = p.length === 4 ? p[2] : NaN;
      let h = p.length === 4 ? p[3] : NaN;
      if (!(w > 0 && h > 0)) {
        const r = el.getBoundingClientRect();
        w = r.width;
        h = r.height;
      }
      if (w > 0 && h > 0) {
        // SVG 를 자연 치수로 고정(배율은 transform 이 담당)
        el.style.width = `${w}px`;
        el.style.height = `${h}px`;
        el.style.maxWidth = "none";
        nat.current = { w, h };
        setScale(
          clamp(
            Math.min(canvas.clientWidth / w, canvas.clientHeight / h) * 0.9,
            MIN,
            MAX,
          ),
        );
      }
    });
    return () => cancelAnimationFrame(id);
  }, [open, svg]);

  // ESC 닫기
  useEffect(() => {
    if (!open) return;
    const h = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", h, true);
    return () => window.removeEventListener("keydown", h, true);
  }, [open, onClose]);

  // 휠 줌: rAF 로 한 프레임의 이벤트를 합쳐 한 번만 반영(빠른 스크롤 깜빡임 방지)
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!open || !canvas) return;
    let raf = 0;
    let pending: { mx: number; my: number; factor: number } | null = null;
    const flush = () => {
      raf = 0;
      if (!pending) return;
      const { mx, my, factor } = pending;
      pending = null;
      const ns = clamp(s.current * factor, MIN, MAX);
      const k = ns / s.current;
      setTx(mx - (mx - x.current) * k);
      setTy(my - (my - y.current) * k);
      setScale(ns);
    };
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const rect = canvas.getBoundingClientRect();
      const mx = e.clientX - (rect.left + rect.width / 2);
      const my = e.clientY - (rect.top + rect.height / 2);
      const step = e.deltaY < 0 ? 1.1 : 1 / 1.1;
      pending = {
        mx,
        my,
        factor: (pending?.factor ?? 1) * step,
      };
      if (!raf) raf = requestAnimationFrame(flush);
    };
    canvas.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      canvas.removeEventListener("wheel", onWheel);
      if (raf) cancelAnimationFrame(raf);
    };
  }, [open]);

  // 드래그 팬 (window 리스너로 캔버스 밖까지 이어짐)
  useEffect(() => {
    if (!open) return;
    const mm = (e: MouseEvent) => {
      if (!drag.current) return;
      const dx = e.clientX - drag.current.x;
      const dy = e.clientY - drag.current.y;
      drag.current = { x: e.clientX, y: e.clientY };
      setTx((t) => t + dx);
      setTy((t) => t + dy);
    };
    const mu = () => {
      drag.current = null;
    };
    window.addEventListener("mousemove", mm);
    window.addEventListener("mouseup", mu);
    return () => {
      window.removeEventListener("mousemove", mm);
      window.removeEventListener("mouseup", mu);
    };
  }, [open]);

  if (!open) return null;

  // 버튼 줌: 캔버스 중앙 기준
  const zoomBy = (k: number) => {
    const ns = clamp(s.current * k, MIN, MAX);
    const ratio = ns / s.current;
    setScale(ns);
    setTx((t) => t * ratio);
    setTy((t) => t * ratio);
  };

  return createPortal(
    <div
      className="mmd-zoom-overlay"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="mmd-zoom-modal">
        <div className="mmd-zoom-toolbar">
          <span className="mmd-zoom-pct">{Math.round(scale * 100)}%</span>
          <span className="mmd-zoom-sp" />
          <Tooltip label={t("diagrams.zoom.out")}>
            <button
              aria-label={t("diagrams.zoom.out")}
              className="icon-btn"
              onClick={() => zoomBy(1 / 1.2)}
            >
              <Icon name="minus" size={16} />
            </button>
          </Tooltip>
          <button
            className="btn btn-sm"
            onClick={reset}
            title={t("diagrams.zoom.fitTitle")}
          >
            {t("diagrams.zoom.fit")}
          </button>
          <Tooltip label={t("diagrams.zoom.in")}>
            <button
              aria-label={t("diagrams.zoom.in")}
              className="icon-btn"
              onClick={() => zoomBy(1.2)}
            >
              <Icon name="plus" size={16} />
            </button>
          </Tooltip>
          <Tooltip label={`${t("common.close")} (Esc)`}>
            <button
              aria-label={`${t("common.close")} (Esc)`}
              className="icon-btn"
              onClick={onClose}
            >
              <Icon name="x" size={17} />
            </button>
          </Tooltip>
        </div>
        <div
          ref={canvasRef}
          className="mmd-zoom-canvas"
          onMouseDown={(e) => {
            drag.current = { x: e.clientX, y: e.clientY };
          }}
        >
          <div
            ref={contentRef}
            className="mmd-zoom-content"
            style={{ transform: `translate(${tx}px, ${ty}px) scale(${scale})` }}
            dangerouslySetInnerHTML={{ __html: svg }}
          />
        </div>
      </div>
    </div>,
    document.body,
  );
}
