// 좌 원문 / 우 라이브 프리뷰 2분할(마크다운 편집)에서 두 칸의 스크롤을 함께 움직인다.
// 한쪽을 내리면 다른 쪽도 같은 자리를 보여준다 - 손으로 굴리는 칸이 항상 주인이 되고,
// 그 반작용으로 되돌아오는 스크롤은 무시한다(잠금). 양방향이라 어느 쪽을 굴려도 된다.
//
// **대응점 기반**이다. 전체 높이 비율로만 맞추면 원문과 렌더 높이가 크게 다른 블록에서 크게 어긋난다:
// ```svg 40줄이 그림 한 장(300px)으로, 표 20줄이 촘촘한 표 하나로 줄어드니 그 아래는 전부 밀린다.
// 그래서 파서가 이미 심어 둔 소스 좌표(data-md-start, lib/mdBlocks)를 대응점으로 쓴다:
// 블록마다 "원문 몇 픽셀 = 렌더 몇 픽셀"을 알면 그 사이는 선형 보간으로 채울 수 있다.
// textarea 는 내부 좌표를 내주지 않으므로 같은 글꼴/폭/줄바꿈 규칙의 복제본에 Range 를 대어 잰다.
// 대응점이 없으면(아직 렌더 전 등) 예전의 전체 비율로 조용히 떨어진다.

import { useEffect, useRef, type RefObject } from "react";

/** 두 칸에서 같은 자리를 가리키는 점. a, b 는 각 칸의 **내용 좌표**(스크롤 0 기준 픽셀) */
export interface Anchor {
  a: number;
  b: number;
}

/** src 의 스크롤 위치를 dst 의 스크롤 범위로 옮긴 값. 어느 쪽이든 못 굴리면 null */
export function mappedScrollTop(
  srcTop: number,
  srcMax: number,
  dstMax: number,
): number | null {
  if (srcMax <= 0 || dstMax <= 0) return null;
  const ratio = Math.min(1, Math.max(0, srcTop / srcMax));
  return ratio * dstMax;
}

/** 뒤로 가는 점과 범위 밖의 점을 걷어낸다 - 보간이 성립하려면 a, b 둘 다 단조여야 한다 */
export function cleanAnchors(anchors: Anchor[], aEnd: number, bEnd: number): Anchor[] {
  const out: Anchor[] = [];
  let lastA = 0;
  let lastB = 0;
  for (const p of anchors) {
    if (!(p.a > lastA && p.b > lastB && p.a < aEnd && p.b < bEnd)) continue;
    out.push(p);
    lastA = p.a;
    lastB = p.b;
  }
  return out;
}

/** a 칸의 내용 좌표 y 를 b 칸의 좌표로 옮긴다. 대응점 사이는 선형 보간, 없으면 전체 비율.
 *  양 끝은 (0,0) 과 (aEnd,bEnd) 를 점으로 세워 문서 처음과 끝이 서로 맞게 한다. */
export function mapWithAnchors(
  y: number,
  anchors: Anchor[],
  aEnd: number,
  bEnd: number,
): number {
  if (aEnd <= 0 || bEnd <= 0) return 0;
  const pts: Anchor[] = [{ a: 0, b: 0 }, ...cleanAnchors(anchors, aEnd, bEnd), { a: aEnd, b: bEnd }];
  let lo = 0;
  let hi = pts.length - 1;
  while (hi - lo > 1) {
    const mid = (lo + hi) >> 1;
    if (pts[mid].a <= y) lo = mid;
    else hi = mid;
  }
  const p = pts[lo];
  const q = pts[hi];
  const span = q.a - p.a;
  if (span <= 0) return p.b;
  const t = Math.min(1, Math.max(0, (y - p.a) / span));
  return p.b + t * (q.b - p.b);
}

/** textarea 안 문자 오프셋들의 픽셀 y. (레이아웃이 있어야 의미가 있어 vitest(node)로는 못 잰다 — 브라우저에서 확인한다)
 * textarea 는 내부 좌표를 안 주므로 같은 글꼴/폭/줄바꿈
 *  규칙의 복제본을 한 번 만들어 Range 로 잰다 - 줄바꿈된 긴 줄도 실제 위치가 그대로 나온다. */
export function measureOffsets(ta: HTMLTextAreaElement, text: string, offsets: number[]): number[] {
  if (!offsets.length) return [];
  const cs = getComputedStyle(ta);
  const mirror = document.createElement("div");
  for (const k of [
    "fontFamily", "fontSize", "fontWeight", "fontStyle", "lineHeight", "letterSpacing",
    "wordSpacing", "textIndent", "textTransform", "whiteSpace", "wordBreak", "overflowWrap",
    "tabSize", "paddingTop", "paddingRight", "paddingBottom", "paddingLeft",
    "borderTopWidth", "borderRightWidth", "borderBottomWidth", "borderLeftWidth", "boxSizing",
  ] as const) {
    mirror.style[k] = cs[k];
  }
  // textarea 는 줄바꿈이 pre-wrap 이다(스타일이 뭐라 하든) - 복제본도 같아야 폭이 맞는다
  mirror.style.whiteSpace = "pre-wrap";
  mirror.style.width = `${ta.clientWidth}px`;
  mirror.style.position = "absolute";
  mirror.style.top = "-99999px";
  mirror.style.left = "0";
  mirror.style.height = "auto";
  mirror.style.visibility = "hidden";
  mirror.style.pointerEvents = "none";
  // 마지막 줄이 빈 줄이면 사라져 높이가 짧아진다 - 한 글자를 덧대 막는다
  mirror.textContent = `${text}​`;
  document.body.appendChild(mirror);
  const node = mirror.firstChild;
  const base = mirror.getBoundingClientRect().top;
  const range = document.createRange();
  const out = offsets.map((o) => {
    if (!node) return 0;
    const i = Math.max(0, Math.min(text.length, o));
    try {
      range.setStart(node, i);
      range.setEnd(node, Math.min(i + 1, text.length + 1));
      const r = range.getBoundingClientRect();
      return r.height ? r.top - base : 0;
    } catch {
      return 0;
    }
  });
  mirror.remove();
  return out;
}

/** 렌더된 칸의 블록마다 (소스 오프셋 -> 그 칸의 픽셀 y) 를 모으고, 원문 쪽 픽셀 y 와 짝지어 준다 */
export function buildAnchors(ta: HTMLTextAreaElement, dst: HTMLElement, text: string): Anchor[] {
  const els = dst.querySelectorAll<HTMLElement>("[data-md-start]");
  if (!els.length) return [];
  const dstTop = dst.getBoundingClientRect().top - dst.scrollTop;
  const offsets: number[] = [];
  const dstYs: number[] = [];
  for (const el of els) {
    const n = Number(el.getAttribute("data-md-start"));
    if (!Number.isFinite(n)) continue;
    offsets.push(n);
    dstYs.push(el.getBoundingClientRect().top - dstTop);
  }
  const srcYs = measureOffsets(ta, text, offsets);
  return offsets.map((_, i) => ({ a: srcYs[i], b: dstYs[i] })).sort((x, y) => x.a - y.a);
}

const RELEASE_MS = 120; // 반작용 스크롤이 잦아들 시간 - 이 뒤엔 다시 어느 쪽이든 주인이 될 수 있다
const REBUILD_MS = 180; // 입력이 멎고 렌더가 끝난 뒤에 지도를 다시 그린다

export function useScrollSync(
  aRef: RefObject<HTMLTextAreaElement | null>,
  bRef: RefObject<HTMLElement | null>,
  enabled: boolean,
  /** 원문. 주면 대응점으로 맞추고, 없으면 전체 비율로만 맞춘다 */
  text?: string,
) {
  // 원문은 ref 로만 읽는다 — 의존성에 넣으면 한 글자 칠 때마다 리스너를 다시 달고 지도를 다시 그린다
  const textRef = useRef(text);
  textRef.current = text;
  const remapRef = useRef<() => void>(() => {});

  useEffect(() => {
    const a = aRef.current;
    const b = bRef.current;
    if (!enabled || !a || !b) return;

    let anchors: Anchor[] = [];
    let owner: HTMLElement | null = null;
    let release: ReturnType<typeof setTimeout> | null = null;
    let rebuild: ReturnType<typeof setTimeout> | null = null;
    let raf = 0;

    const remap = () => {
      const t = textRef.current;
      anchors = t === undefined ? [] : buildAnchors(a, b, t);
    };
    const scheduleRemap = () => {
      if (rebuild) clearTimeout(rebuild);
      rebuild = setTimeout(remap, REBUILD_MS);
    };
    remapRef.current = scheduleRemap;
    // 첫 지도도 미뤄 그린다 — 마운트 직후엔 그림·다이어그램이 아직 높이를 갖지 않았다
    scheduleRemap();

    const follow = (src: HTMLElement, dst: HTMLElement) => {
      raf = 0;
      const srcMax = src.scrollHeight - src.clientHeight;
      const dstMax = dst.scrollHeight - dst.clientHeight;
      if (srcMax <= 0 || dstMax <= 0) return;
      if (!anchors.length) {
        const top = mappedScrollTop(src.scrollTop, srcMax, dstMax);
        if (top !== null) dst.scrollTop = top;
        return;
      }
      // 대응점은 a(원문) 기준이라, 프리뷰가 주인이면 짝을 뒤집어 같은 함수를 쓴다
      const fromA = src === a;
      const pts = fromA ? anchors : anchors.map((p) => ({ a: p.b, b: p.a }));
      const y = mapWithAnchors(src.scrollTop, pts, src.scrollHeight, dst.scrollHeight);
      dst.scrollTop = Math.min(dstMax, Math.max(0, y));
    };

    const onScroll = (src: HTMLElement, dst: HTMLElement) => () => {
      // 주인이 아닌 칸의 이벤트 = 방금 우리가 옮긴 반작용이다. 여기서 되받으면 서로 밀며 떤다.
      if (owner && owner !== src) return;
      owner = src;
      if (release) clearTimeout(release);
      release = setTimeout(() => {
        owner = null;
      }, RELEASE_MS);
      if (!raf) raf = requestAnimationFrame(() => follow(src, dst));
    };

    const onA = onScroll(a, b);
    const onB = onScroll(b, a);
    a.addEventListener("scroll", onA, { passive: true });
    b.addEventListener("scroll", onB, { passive: true });
    // 프리뷰 높이는 늦게 정해진다(그림·다이어그램이 나중에 뜬다) - 바뀌면 지도를 다시 그린다
    const ro = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(scheduleRemap);
    ro?.observe(a);
    ro?.observe(b);

    return () => {
      a.removeEventListener("scroll", onA);
      b.removeEventListener("scroll", onB);
      ro?.disconnect();
      if (release) clearTimeout(release);
      if (rebuild) clearTimeout(rebuild);
      if (raf) cancelAnimationFrame(raf);
      remapRef.current = () => {};
    };
  }, [aRef, bRef, enabled]);

  // 글이 바뀌면 지도만 다시 그린다(리스너는 그대로) — 입력이 멎은 뒤 한 번만 돈다
  useEffect(() => {
    remapRef.current();
  }, [text]);
}
