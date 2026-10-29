// 노트 읽기 본문 위에 Range 를 **글자 높이로** 칠하는 층 — 드래그 선택과 질문 형광펜이 쓴다.
//
// 왜 직접 칠하나: WebKit 은 둘째 줄부터 칠(선택, ::highlight 둘 다)의 위를 윗줄 글자 바닥까지 늘려 줄 사이
// 틈을 메운다. 행간이 넓은 본문(1.7)에서는 글자 위로 빈 띠가 붙어 글자보다 위까지 칠한 것처럼 보였다.
// CSS 로는 그 높이를 못 바꾼다 — 그래서 글자 상자(getClientRects)만큼 깐다. 글자 위에 덮지만
// 혼합 모드라 글자 색은 그대로 남는다(styles.css .glyph-paint).
//
// SelectionSweep 과 달리 고정 레이어가 아니다. 본문과 같이 스크롤되는 .note-read-wrap 안에 깔아야
// 스크롤할 때 칠이 한 박자 늦게 따라오지 않는다. 대신 본문 배치가 바뀌면(토글 펼침, 그림 로드, 폭) 다시 잰다.

import { useEffect, useMemo, useRef, useState } from "react";
import { inCollapsedBody, onToggleChange } from "../lib/mdDetails";
import type { Rect } from "../lib/noteAnchor";

/** 칠의 종류 — 색은 styles.css 가 정한다(질문은 note-q 와 같은 규약: 전부 연하게, 지금 것만 진하게) */
export type GlyphTone = "sel" | "q" | "q-cur";
export type GlyphMark = { range: Range; tone: GlyphTone };

/** 넘친 글자를 잘라 보이는 블록(styles.css 의 overflow, resin 표 설명의 가로 스크롤 칸) — 칠도 그 상자 밖으로 새지 않게 자른다 */
const CLIPS = "pre, table, .md-svg, .mermaid-rendered, .md-toggle-inner, .rz-details .t-wrap";

function clipRect(r: DOMRect, box: DOMRect | null): DOMRect | null {
  if (!box) return r;
  const left = Math.max(r.left, box.left);
  const right = Math.min(r.right, box.right);
  const top = Math.max(r.top, box.top);
  const bottom = Math.min(r.bottom, box.bottom);
  return right > left && bottom > top ? new DOMRect(left, top, right - left, bottom - top) : null;
}

/** el 을 감싼 CLIPS 상자들의 교집합 — resin 표는 table 보다 그걸 감싼 스크롤 칸이 좁다. 없으면 null */
function clipBox(el: Element): DOMRect | null {
  let box: DOMRect | null = null;
  for (let b = el.closest(CLIPS); b; b = b.parentElement?.closest(CLIPS) ?? null) {
    const r = b.getBoundingClientRect();
    // 겹치지 않으면 다 가려졌다 — 빈 상자라 아무것도 칠하지 않는다
    box = box ? (clipRect(r, box) ?? new DOMRect(r.left, r.top, 0, 0)) : r;
  }
  return box;
}

/** r 중 본문 c 안의 글자 상자들(화면 좌표). clips = 이번 측정 동안 블록별 자르는 상자 캐시 */
function glyphRects(c: HTMLElement, r: Range, clips: Map<Element, DOMRect | null>): DOMRect[] {
  // 본문이 다시 그려져 떨어져 나간 노드의 구간이면 여기서 빠진다
  if (!r.intersectsNode(c)) return [];
  // 본문 안에서 시작하고 끝나면 공통 조상만 훑는다(긴 노트에서 짧게 고를 때 전체를 돌지 않게)
  const common = r.commonAncestorContainer;
  const root = !c.contains(common)
    ? c
    : common.nodeType === Node.TEXT_NODE
      ? common.parentNode!
      : common;
  const out: DOMRect[] = [];
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
    if (!clips.has(parent)) clips.set(parent, clipBox(parent));
    for (const b of part.getClientRects()) {
      const v = clipRect(b, clips.get(parent)!);
      if (v) out.push(v);
    }
  }
  return out;
}

export function GlyphPaint({
  containerRef,
  marks,
}: {
  /** 칠할 본문 — 이 안의 글자만 칠한다 */
  containerRef: React.RefObject<HTMLElement | null>;
  /** 칠할 구간들. 뒤에 온 것이 위에 칠해진다. 렌더마다 새 배열이면 매번 다시 재므로 memo 해서 넘긴다 */
  marks: GlyphMark[];
}) {
  const layerRef = useRef<HTMLDivElement>(null);
  const [boxes, setBoxes] = useState<(Rect & { tone: GlyphTone })[]>([]);

  useEffect(() => {
    if (!marks.length) {
      setBoxes([]);
      return;
    }
    let raf = 0;
    const measure = () => {
      raf = 0;
      const c = containerRef.current;
      const layer = layerRef.current;
      if (!c || !layer) return setBoxes([]);
      const base = layer.getBoundingClientRect();
      const clips = new Map<Element, DOMRect | null>();
      setBoxes(
        marks.flatMap(({ range, tone }) =>
          glyphRects(c, range, clips).map((b) => ({
            tone,
            top: b.top - base.top,
            left: b.left - base.left,
            width: b.width,
            height: b.height,
          })),
        ),
      );
    };
    const soon = () => {
      if (!raf) raf = requestAnimationFrame(measure);
    };
    // 본문 스크롤은 칠이 같이 움직인다 — 다시 잴 건 본문 **안**의 스크롤(코드 블록을 가로로 굴릴 때)뿐이다
    const onScroll = (e: Event) => {
      if (e.target instanceof Node && containerRef.current?.contains(e.target)) soon();
    };
    measure();
    // 토글 펼침(트랙이 자라는 동안 매 프레임), 그림 로드, 다이어그램 렌더, 폭 변경 — 본문 상자 크기로 다 잡힌다
    const ro = new ResizeObserver(soon);
    if (containerRef.current) ro.observe(containerRef.current);
    window.addEventListener("scroll", onScroll, true);
    const offToggle = onToggleChange(containerRef.current, soon);
    return () => {
      offToggle?.();
      ro.disconnect();
      if (raf) cancelAnimationFrame(raf);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, [marks, containerRef]);

  return (
    <div ref={layerRef} className="glyph-paint" aria-hidden="true">
      {boxes.map((b, i) => (
        <span
          key={i}
          className={b.tone}
          style={{ top: b.top, left: b.left, width: b.width, height: b.height }}
        />
      ))}
    </div>
  );
}

/** 본문 드래그 선택 — 네이티브 칠은 styles.css 가 지운다(.note-read-wrap > .markdown, .glyph-sel 의 ::selection) */
export function SelectionPaint({
  containerRef,
}: {
  containerRef: React.RefObject<HTMLElement | null>;
}) {
  const [range, setRange] = useState<Range | null>(null);
  useEffect(() => {
    let raf = 0;
    const read = () => {
      raf = 0;
      const c = containerRef.current;
      const sel = window.getSelection();
      const r = sel && sel.rangeCount > 0 && !sel.isCollapsed ? sel.getRangeAt(0) : null;
      // 본문 밖(질문 칸 입력 등)의 선택은 null 로 접는다 — 같은 값이면 다시 그리지 않는다
      setRange(c && r && r.intersectsNode(c) ? r.cloneRange() : null);
    };
    const soon = () => {
      if (!raf) raf = requestAnimationFrame(read);
    };
    document.addEventListener("selectionchange", soon);
    return () => {
      if (raf) cancelAnimationFrame(raf);
      document.removeEventListener("selectionchange", soon);
    };
  }, [containerRef]);
  const marks = useMemo<GlyphMark[]>(() => (range ? [{ range, tone: "sel" }] : []), [range]);
  return <GlyphPaint containerRef={containerRef} marks={marks} />;
}
