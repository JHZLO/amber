// 페이지 내 검색 바(⌘F) — 필기노트·개념·투두가 공유한다.
// 각 화면이 자기 검색을 따로 만들면 단축키·표시·이동 규칙이 화면마다 갈린다.
//
// 두 갈래로 돈다:
//   읽기   — 그려진 텍스트를 Range 로 찾아 칠하고 그 자리로 스크롤한다.
//   편집   — `sourceRef`(textarea)가 오면 **원문에서** 찾는다.
//            textarea 의 내용은 텍스트 노드가 아니라 Range 로 잡히지 않고(원문 쪽은 0건이 된다),
//            잡히는 건 오른쪽 미리보기뿐인데 그쪽으로 스크롤해 봐야 스크롤 동기화가 원문 칸을
//            도로 끌어당긴다.
//            칠하는 자리도 원문 칸 위가 아니다 — 같은 글을 같은 상자에 겹쳐 그린 **거울판**에
//            칠하고 원문 칸은 배경만 비운다. 네이티브 선택은 못 쓴다: WKWebView 는 포커스 없는
//            textarea 의 선택을 아예 그리지 않고(`::selection` 을 줘도 안 된다), 그렇다고 원문 칸에
//            포커스를 내려놓으면 그다음 Enter 가 줄바꿈이 되어 찾다가 노트가 고쳐진다.

import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Icon } from "../icons";
import { t } from "../lib/i18n";
import { FIND_LIMIT, clearPaint, findInText, findRanges, paint } from "../lib/pageFind";

const KEY_ALL = "page-find";
const KEY_CUR = "page-find-cur";
/** 검색 디바운스 — 매 글자마다 전체 DOM 을 훑으면 긴 노트에서 타이핑이 밀린다 */
const FIND_DEBOUNCE_MS = 120;
/** 거울판이 원문 칸에서 그대로 베껴 와야 하는 것 — 줄이 같은 자리에서 접히게 하는 값들 */
const MIRROR_PROPS = [
  "font-family",
  "font-size",
  "font-weight",
  "font-style",
  "line-height",
  "letter-spacing",
  "word-spacing",
  "text-indent",
  "tab-size",
  "white-space",
  "overflow-wrap",
  "word-break",
  "padding-top",
  "padding-right",
  "padding-bottom",
  "padding-left",
  "border-top-width",
  "border-right-width",
  "border-bottom-width",
  "border-left-width",
];

export function PageFind({
  containerRef,
  sourceRef,
  sourceText,
  active,
}: {
  /** 검색 대상 — 이 요소 안의 텍스트만 찾는다 */
  containerRef: React.RefObject<HTMLElement | null>;
  /** 편집 중인 원문 칸. 주면 여기서 찾는다(없으면 읽기 갈래) */
  sourceRef?: React.RefObject<HTMLTextAreaElement | null>;
  /** 그 칸의 현재 글. 거울판에 그대로 다시 그리고, 글이 바뀌면 자리를 다시 잡는다 */
  sourceText?: string;
  /** 이 섹션이 화면에 있는가 (안 보이는 화면이 단축키를 가로채지 않게) */
  active: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [idx, setIdx] = useState(0);
  const [count, setCount] = useState(0);
  const hitsRef = useRef<Range[]>([]);
  /** 편집 갈래의 결과 — 원문 문자열 안의 [시작, 끝). 거울판을 다시 그려야 해서 state 다 */
  const [srcHits, setSrcHits] = useState<[number, number][]>([]);
  const inputRef = useRef<HTMLInputElement>(null);
  const timerRef = useRef<number | null>(null);
  const mirrorRef = useRef<HTMLDivElement | null>(null);
  /** 나갈 때 커서를 둘 자리 — 마지막으로 보여 준 구간 */
  const lastHitRef = useRef<[number, number] | null>(null);

  const cancelPending = useCallback(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const close = useCallback(() => {
    cancelPending();
    setOpen(false);
    setQuery("");
    setCount(0);
    setIdx(0);
    hitsRef.current = [];
    setSrcHits([]);
    clearPaint(KEY_ALL, KEY_CUR);
    // 편집 중이었다면 찾던 자리에 커서를 두고 나간다 — 찾은 이유가 거기를 고치려는 것이다.
    // 찾기 줄에 손이 있었을 때만 — 화면을 떠나서 닫히는 경우까지 원문 칸을 붙잡으면 안 된다
    const src = sourceRef?.current;
    const at = lastHitRef.current;
    if (src && at && document.activeElement?.closest(".page-find")) {
      // focus() 는 그 순간 커서 자리(맨 앞)로 화면을 되돌린다 — 보고 있던 자리를 그대로 둔다
      const keep = src.scrollTop;
      src.focus();
      src.setSelectionRange(at[0], at[1]);
      src.scrollTop = keep;
    }
    lastHitRef.current = null;
  }, [cancelPending, sourceRef]);

  /** 현재 결과를 화면 안으로. Range 는 스크롤이 안 되므로 그 글자를 품은 요소를 쓴다. */
  const focusHit = useCallback((hits: Range[], i: number) => {
    const r = hits[i];
    if (!r) return;
    paint(KEY_CUR, [r]);
    const el =
      r.startContainer.nodeType === Node.TEXT_NODE
        ? r.startContainer.parentElement
        : (r.startContainer as HTMLElement);
    el?.scrollIntoView({ block: "center", behavior: "smooth" });
  }, []);

  /** 지금 구간을 원문 칸 안으로 — 거울판에 그려진 <mark> 의 자리를 그대로 쓴다.
   *  줄 번호 × 줄 높이로 셈하면 접혀 넘어간 줄(soft wrap)만큼 어긋난다. */
  const scrollSrcToCur = useCallback(() => {
    const ta = sourceRef?.current;
    const mark = mirrorRef.current?.querySelector<HTMLElement>("mark.cur");
    if (!ta || !mark) return;
    const top = mark.offsetTop;
    const edge = 24; // 가장자리에 딱 붙지 않게
    // 이미 보이면 건드리지 않는다 — 한 칸 옮길 때마다 화면이 튀면 어디로 갔는지 놓친다
    const seen = top >= ta.scrollTop + edge && top + mark.offsetHeight <= ta.scrollTop + ta.clientHeight - edge;
    if (seen) return;
    ta.scrollTop = Math.max(0, top - ta.clientHeight / 2);
  }, [sourceRef]);

  const run = useCallback(
    (q: string) => {
      const src = sourceRef?.current;
      if (src) {
        // 편집 갈래 — 미리보기는 칠하지 않는다. 원문과 렌더는 글자가 달라(마크다운 기호) 개수가
        // 어긋나는데, 7/14 라고 해 놓고 다른 데를 칠하면 어느 쪽을 세고 있는지 알 수 없어진다
        clearPaint(KEY_ALL, KEY_CUR);
        const hits = findInText(src.value, q);
        setSrcHits(hits);
        hitsRef.current = [];
        setCount(hits.length);
        setIdx(0);
        lastHitRef.current = hits[0] ?? null;
        // 자리로 굴리는 건 거울판이 다시 그려진 뒤다(아래 효과) — 지금은 <mark> 가 아직 없다
        return;
      }
      const root = containerRef.current;
      if (!root) return;
      const hits = findRanges(root, q);
      hitsRef.current = hits;
      setSrcHits([]);
      setCount(hits.length);
      setIdx(0);
      paint(KEY_ALL, hits);
      if (hits.length) focusHit(hits, 0);
      else clearPaint(KEY_CUR);
    },
    [containerRef, sourceRef, focusHit],
  );

  /** 타이핑 중에는 미룬다 — 마지막 입력 뒤 한 번만 훑는다 */
  const runSoon = useCallback(
    (q: string) => {
      cancelPending();
      timerRef.current = window.setTimeout(() => {
        timerRef.current = null;
        run(q);
      }, FIND_DEBOUNCE_MS);
    },
    [cancelPending, run],
  );

  const step = useCallback(
    (delta: number) => {
      const dom = hitsRef.current;
      const n = srcHits.length || dom.length;
      if (!n) return;
      const next = (((idx + delta) % n) + n) % n;
      setIdx(next);
      // 편집 갈래는 포커스를 옮기지 않는다 — Enter 를 연달아 눌러 다음으로 넘어가야 한다.
      // 자리 이동은 거울판이 다시 그려진 뒤 효과가 맡는다
      if (srcHits.length) lastHitRef.current = srcHits[next] ?? null;
      else focusHit(dom, next);
    },
    [idx, srcHits, focusHit],
  );

  // 읽기↔편집을 오가면 찾을 대상이 바뀐다 — 열려 있으면 그 자리에서 다시 훑는다
  const runRef = useRef(run);
  runRef.current = run;
  useEffect(() => {
    if (open && query.trim()) runRef.current(query);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sourceRef]);

  // 원문을 고치면 뒤쪽 자리가 전부 밀린다 — 찾기가 열려 있으면 다시 훑는다(칠한 자리가 어긋난 채로
  // 남으면 엉뚱한 글자를 가리킨다). 타이핑 중이므로 디바운스를 탄다
  useEffect(() => {
    if (open && query.trim() && sourceRef?.current) runSoon(query);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sourceText]);

  // 거울판을 원문 칸과 **같은 상자**로 맞춘다 — 글꼴·여백·줄바꿈 규칙이 한 가지라도 다르면 줄이
  // 다르게 접혀 칠한 자리가 밀린다. CSS 에 값을 베껴 두지 않고 계산된 스타일에서 가져온다
  // (원문 칸 글꼴을 고쳐도 따라온다). 스크롤도 여기서 맞춘다 — 다시 그린 직후에도 붙어 있어야 한다
  useEffect(() => {
    const ta = sourceRef?.current;
    const m = mirrorRef.current;
    if (!ta || !m) return;
    const cs = getComputedStyle(ta);
    for (const prop of MIRROR_PROPS) m.style.setProperty(prop, cs.getPropertyValue(prop));
    // 스크롤바가 자리를 먹는 설정("항상 표시")이면 원문 칸만 글 폭이 좁아져 줄이 더 일찍 접힌다 —
    // 그만큼 거울판 오른쪽 여백을 늘려 같은 자리에서 접히게 한다
    const frame = parseFloat(cs.borderLeftWidth) + parseFloat(cs.borderRightWidth);
    const bar = Math.max(0, ta.offsetWidth - frame - ta.clientWidth);
    m.style.paddingRight = `${parseFloat(cs.paddingRight) + bar}px`;
    m.scrollTop = ta.scrollTop;
    m.scrollLeft = ta.scrollLeft;
  });

  // 원문 칸을 굴리면 거울판도 같이 굴린다
  useEffect(() => {
    const ta = sourceRef?.current;
    if (!ta) return;
    const sync = () => {
      const m = mirrorRef.current;
      if (!m) return;
      m.scrollTop = ta.scrollTop;
      m.scrollLeft = ta.scrollLeft;
    };
    ta.addEventListener("scroll", sync);
    return () => ta.removeEventListener("scroll", sync);
  }, [sourceRef, open]);

  // 결과가 바뀌거나 다음으로 넘어가면 그 자리를 화면 안으로. 거울판이 그려진 뒤라야 <mark> 가 있다
  useEffect(() => {
    if (open && srcHits.length) scrollSrcToCur();
  }, [open, srcHits, idx, scrollSrcToCur]);

  // ⌘F 로 연다. 섹션이 보일 때만 — 안 보이는 화면이 단축키를 먹으면 어디가 열렸는지 알 수 없다.
  const activeRef = useRef(active);
  activeRef.current = active;
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (!activeRef.current) return;
      if ((e.metaKey || e.ctrlKey) && !e.altKey && e.key.toLowerCase() === "f") {
        // 모달이 떠 있으면 그쪽이 우선이다
        if (document.querySelector(".overlay, .mmd-zoom-overlay")) return;
        e.preventDefault();
        setOpen(true);
        requestAnimationFrame(() => inputRef.current?.select());
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  // 섹션을 떠나거나 언마운트되면 칠한 것을 걷는다 — 하이라이트는 전역이라 남으면 다른 화면에 묻는다
  useEffect(() => {
    if (!active) close();
  }, [active, close]);
  useEffect(
    () => () => {
      cancelPending();
      clearPaint(KEY_ALL, KEY_CUR);
    },
    [cancelPending],
  );

  if (!open) return null;
  // 편집 갈래의 형광펜 — 원문 칸을 감싼 칸에 겹쳐 그린다(자리·크기는 위 효과가 맞춘다)
  const mirrorHost = sourceText !== undefined ? sourceRef?.current?.parentElement : null;
  return (
    <>
      {mirrorHost &&
        createPortal(
          <div className="find-src-mirror" aria-hidden ref={mirrorRef}>
            {srcSegments(sourceText ?? "", srcHits, idx)}
          </div>,
          mirrorHost,
        )}
      <div className="page-find">
        <Icon name="search" size={14} />
        <input
          ref={inputRef}
          className="page-find-input"
          autoFocus
          placeholder={t("common.find.ph")}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            runSoon(e.target.value);
          }}
          onKeyDown={(e) => {
            if (e.nativeEvent.isComposing) return;
            if (e.key === "Enter") step(e.shiftKey ? -1 : 1);
            if (e.key === "Escape") close();
          }}
        />
        <span className="page-find-count">
          {/* 상한에 닿으면 "500+" — 그 뒤는 찾지 않았다는 뜻이다 */}
          {count
            ? `${idx + 1}/${count}${count >= FIND_LIMIT ? "+" : ""}`
            : query.trim()
              ? "0"
              : ""}
        </span>
        <button
          className="page-find-nav"
          aria-label={t("common.find.prev")}
          onClick={() => step(-1)}
          disabled={!count}
        >
          <Icon name="chevron-left" size={14} />
        </button>
        <button
          className="page-find-nav"
          aria-label={t("common.find.next")}
          onClick={() => step(1)}
          disabled={!count}
        >
          <Icon name="chevron-right" size={14} />
        </button>
        <button className="page-find-nav" aria-label={t("common.close")} onClick={close}>
          <Icon name="x" size={14} />
        </button>
      </div>
    </>
  );
}

/** 거울판에 그릴 조각 — 찾은 구간만 <mark>, 나머지는 글자 그대로.
 *  끝에 줄바꿈을 하나 더한다: 마지막 줄이 빈 줄일 때 높이가 원문 칸과 같아야 자리가 맞는다. */
function srcSegments(text: string, hits: [number, number][], cur: number) {
  const out: React.ReactNode[] = [];
  let at = 0;
  for (const [i, [start, end]] of hits.entries()) {
    if (start > at) out.push(text.slice(at, start));
    out.push(
      <mark key={i} className={i === cur ? "cur" : undefined}>
        {text.slice(start, end)}
      </mark>,
    );
    at = end;
  }
  out.push(`${text.slice(at)}\n`);
  return out;
}
