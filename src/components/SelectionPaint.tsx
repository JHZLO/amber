// 읽기 모드 본문의 드래그 선택을 **글자 높이로** 칠한다.
//
// 왜 직접 칠하나: WebKit 은 둘째 줄부터 선택 칸의 위를 윗줄 글자 바닥까지 늘려 줄 사이 틈을 메운다.
// 행간이 넓은 본문(1.7)에서는 글자 위로 빈 띠가 붙어 글자보다 위까지 잡힌 것처럼 보였다.
// CSS 로는 그 높이를 못 바꾼다 — 그래서 네이티브 칠은 투명하게 두고(styles.css) 글자 상자
// (getClientRects)만큼 깐다. 글자 위에 덮지만 혼합 모드라 글자 색은 그대로 남는다.
//
// SelectionSweep 과 달리 고정 레이어가 아니다. 본문과 같이 스크롤되는 자리에 깔아야
// 스크롤할 때 칠이 글자를 한 박자 늦게 따라오지 않는다.

import { useEffect, useRef, useState } from "react";
import { inCollapsedBody, onToggleChange } from "../lib/mdDetails";
import type { Rect } from "../lib/noteAnchor";

/** 넘친 글자를 잘라 보이는 블록(styles.css 의 overflow) — 칠도 그 상자 밖으로 새지 않게 자른다 */
const CLIPS = "pre, table, .md-svg, .mermaid-rendered, .md-toggle-inner";

function clipRect(r: DOMRect, box: DOMRect | null): DOMRect | null {
  if (!box) return r;
  const left = Math.max(r.left, box.left);
  const right = Math.min(r.right, box.right);
  const top = Math.max(r.top, box.top);
  const bottom = Math.min(r.bottom, box.bottom);
  return right > left && bottom > top ? new DOMRect(left, top, right - left, bottom - top) : null;
}

export function SelectionPaint({
  containerRef,
}: {
  /** 칠할 본문 — 이 안의 선택만 칠한다(styles.css 가 이 안의 네이티브 칠만 지운다) */
  containerRef: React.RefObject<HTMLElement | null>;
}) {
  const layerRef = useRef<HTMLDivElement>(null);
  const [boxes, setBoxes] = useState<Rect[]>([]);

  useEffect(() => {
    let raf = 0;
    const measure = () => {
      raf = 0;
      const c = containerRef.current;
      const layer = layerRef.current;
      const sel = window.getSelection();
      if (!c || !layer || !sel || sel.rangeCount === 0 || sel.isCollapsed) return setBoxes([]);
      const r = sel.getRangeAt(0);
      if (!r.intersectsNode(c)) return setBoxes([]);
      // 본문 안에서 시작하고 끝나면 공통 조상만 훑는다(긴 노트에서 짧게 고를 때 전체를 돌지 않게)
      const common = r.commonAncestorContainer;
      const root = !c.contains(common)
        ? c
        : common.nodeType === Node.TEXT_NODE
          ? common.parentNode!
          : common;
      const base = layer.getBoundingClientRect();
      const clips = new Map<Element, DOMRect | null>();
      const out: Rect[] = [];
      const part = document.createRange();
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      for (let n = walker.nextNode(); n; n = walker.nextNode()) {
        if (!r.intersectsNode(n)) continue;
        part.selectNodeContents(n);
        if (n === r.startContainer) part.setStart(n, r.startOffset);
        if (n === r.endContainer) part.setEnd(n, r.endOffset);
        // 접힌 토글 안의 글자는 잘려 있다 — 그 상자는 아래 블록 위에 놓인다
        if (inCollapsedBody(part)) continue;
        const parent = n.parentElement!;
        if (!clips.has(parent)) clips.set(parent, parent.closest(CLIPS)?.getBoundingClientRect() ?? null);
        for (const b of part.getClientRects()) {
          const v = clipRect(b, clips.get(parent)!);
          if (v) out.push({ top: v.top - base.top, left: v.left - base.left, width: v.width, height: v.height });
        }
      }
      setBoxes(out);
    };
    const soon = () => {
      if (!raf) raf = requestAnimationFrame(measure);
    };
    document.addEventListener("selectionchange", soon);
    // 본문 스크롤은 칠이 같이 움직이지만, 코드 블록을 가로로 굴리면 글자만 움직인다
    window.addEventListener("scroll", soon, true);
    window.addEventListener("resize", soon);
    const offToggle = onToggleChange(containerRef.current, soon);
    return () => {
      offToggle?.();
      if (raf) cancelAnimationFrame(raf);
      document.removeEventListener("selectionchange", soon);
      window.removeEventListener("scroll", soon, true);
      window.removeEventListener("resize", soon);
    };
  }, [containerRef]);

  return (
    <div ref={layerRef} className="sel-paint" aria-hidden="true">
      {boxes.map((b, i) => (
        <span key={i} style={{ top: b.top, left: b.left, width: b.width, height: b.height }} />
      ))}
    </div>
  );
}
