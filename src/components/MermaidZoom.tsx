// mermaid 다이어그램 확대 뷰어: 가운데 카드 모달 + 휠 줌(커서 기준) + 드래그 팬.
// 최대 120% 로 제한하므로 CSS transform(translate+scale)로 처리한다 — 레이아웃 재계산이 없어
// 매끄럽고 깜빡임이 없다(저배율이라 transform 업스케일 흐림·텍스처 한계 문제도 없음).

import { useCallback, useEffect, useRef, useState } from "react";
import { Dialog as DialogPrimitive, VisuallyHidden } from "radix-ui";
import { Icon } from "../icons";
import { Tooltip } from "../ui";
import { t } from "../lib/i18n";
import { Button } from "@/components/ui/button";
import { portalContainer, useReturnFocus } from "@/components/ui/dialog";

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
  // 휠 리스너를 붙일 캔버스. 판은 Radix(Presence)가 열린 **다음 렌더**에 붙이므로, [open] 만 보는
  // effect 는 캔버스가 생기기 전에 돌고 끝난다 — 붙는 순간을 상태로 받아 그때 다시 돈다
  const [canvasEl, setCanvasEl] = useState<HTMLDivElement | null>(null);
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

  // 휠 줌: rAF 로 한 프레임의 이벤트를 합쳐 한 번만 반영(빠른 스크롤 깜빡임 방지)
  useEffect(() => {
    const canvas = canvasEl;
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
  }, [open, canvasEl]);

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

  const returnFocus = useReturnFocus(open);

  // 버튼 줌: 캔버스 중앙 기준
  const zoomBy = (k: number) => {
    const ns = clamp(s.current * k, MIN, MAX);
    const ratio = ns / s.current;
    setScale(ns);
    setTx((t) => t * ratio);
    setTy((t) => t * ratio);
  };

  // 겹친 창 중 맨 위 것만 Esc 를 받는 건 Radix 레이어가 한다 — 모달 위에서 열어도 뷰어만 닫힌다.
  // 배경을 눌러도 닫지 않는다 — 닫기는 X 와 Esc 뿐(ui.tsx Modal 과 같은 규약).
  // 확대한 그림을 끌어 옮기다 커서가 판 밖에서 떨어지면 뷰어가 닫혀 버렸다.
  return (
    <DialogPrimitive.Root open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogPrimitive.Portal container={portalContainer()}>
        <div className="mmd-zoom-overlay">
          <DialogPrimitive.Content
            className="mmd-zoom-modal"
            aria-describedby={undefined}
            onOpenAutoFocus={(e) => {
              // 첫 버튼(축소)에 포커스 링이 서지 않게 판 자체에 둔다 — 키보드는 Tab 으로 들어간다
              e.preventDefault();
              (e.currentTarget as HTMLElement | null)?.focus();
            }}
            onCloseAutoFocus={returnFocus}
            onInteractOutside={(e) => e.preventDefault()}
          >
            <VisuallyHidden.Root asChild>
              <DialogPrimitive.Title>{t("diagrams.zoom.title")}</DialogPrimitive.Title>
            </VisuallyHidden.Root>
            <div className="mmd-zoom-toolbar">
              <span className="mmd-zoom-pct">{Math.round(scale * 100)}%</span>
              <span className="mmd-zoom-sp" />
              <Tooltip label={t("diagrams.zoom.out")}>
                <Button
                  aria-label={t("diagrams.zoom.out")}
                  size="icon"
                  onClick={() => zoomBy(1 / 1.2)}
                >
                  <Icon name="minus" size={16} />
                </Button>
              </Tooltip>
              <Button
                size="sm"
                onClick={reset}
                title={t("diagrams.zoom.fitTitle")}
              >
                {t("diagrams.zoom.fit")}
              </Button>
              <Tooltip label={t("diagrams.zoom.in")}>
                <Button
                  aria-label={t("diagrams.zoom.in")}
                  size="icon"
                  onClick={() => zoomBy(1.2)}
                >
                  <Icon name="plus" size={16} />
                </Button>
              </Tooltip>
              <Tooltip label={`${t("common.close")} (Esc)`}>
                <Button
                  aria-label={`${t("common.close")} (Esc)`}
                  size="icon"
                  onClick={onClose}
                >
                  <Icon name="x" size={17} />
                </Button>
              </Tooltip>
            </div>
            <div
              ref={(el) => {
                canvasRef.current = el;
                setCanvasEl(el);
              }}
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
          </DialogPrimitive.Content>
        </div>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
