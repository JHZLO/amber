// 페이지 내 검색 바(⌘F) — 필기노트·개념·투두가 공유한다.
// 각 화면이 자기 검색을 따로 만들면 단축키·표시·이동 규칙이 화면마다 갈린다.
//
// 두 갈래로 돈다:
//   읽기   — 그려진 텍스트를 Range 로 찾아 칠하고 그 자리로 스크롤한다.
//   편집   — `sourceRef`(textarea)가 오면 **원문에서** 찾고 그 자리를 선택한다.
//            textarea 의 내용은 텍스트 노드가 아니라 Range 로 잡히지 않고(원문 쪽은 0건이 된다),
//            잡히는 건 오른쪽 미리보기뿐인데 그쪽으로 스크롤해 봐야 스크롤 동기화가 원문 칸을
//            도로 끌어당긴다. 편집 중에 찾는 건 고치려는 것이니 커서가 원문에 서야 맞는다.

import { useCallback, useEffect, useRef, useState } from "react";
import { Icon } from "../icons";
import { t } from "../lib/i18n";
import { FIND_LIMIT, clearPaint, findInText, findRanges, paint } from "../lib/pageFind";

const KEY_ALL = "page-find";
const KEY_CUR = "page-find-cur";
/** 검색 디바운스 — 매 글자마다 전체 DOM 을 훑으면 긴 노트에서 타이핑이 밀린다 */
const FIND_DEBOUNCE_MS = 120;

export function PageFind({
  containerRef,
  sourceRef,
  active,
}: {
  /** 검색 대상 — 이 요소 안의 텍스트만 찾는다 */
  containerRef: React.RefObject<HTMLElement | null>;
  /** 편집 중인 원문 칸. 주면 여기서 찾고 여기에 커서를 놓는다(없으면 읽기 갈래) */
  sourceRef?: React.RefObject<HTMLTextAreaElement | null>;
  /** 이 섹션이 화면에 있는가 (안 보이는 화면이 단축키를 가로채지 않게) */
  active: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [idx, setIdx] = useState(0);
  const [count, setCount] = useState(0);
  const hitsRef = useRef<Range[]>([]);
  /** 편집 갈래의 결과 — 원문 문자열 안의 [시작, 끝) */
  const srcHitsRef = useRef<[number, number][]>([]);
  const inputRef = useRef<HTMLInputElement>(null);
  const timerRef = useRef<number | null>(null);

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
    srcHitsRef.current = [];
    clearPaint(KEY_ALL, KEY_CUR);
    // 편집 중이었다면 찾던 자리에 커서를 두고 나간다 — 찾은 이유가 거기를 고치려는 것이다
    sourceRef?.current?.focus();
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

  /** 그 구간이 든 줄을 원문 칸 한가운데로 (포커스는 건드리지 않는다) */
  const scrollSrcTo = useCallback(
    (at: number) => {
      const el = sourceRef?.current;
      if (!el) return;
      const line = el.value.slice(0, at).split("\n").length - 1;
      const lineH = parseFloat(getComputedStyle(el).lineHeight) || 20;
      el.scrollTop = Math.max(0, line * lineH - el.clientHeight / 2);
    },
    [sourceRef],
  );

  /** 편집 갈래 — 원문 칸의 그 구간을 **포커스한 채** 선택한다.
   *
   *  포커스를 입력칸으로 돌려놓지 않는 이유: WKWebView 는 포커스 없는 textarea 의 선택을
   *  아예 그리지 않는다(`::selection` 을 줘도 안 된다). 돌려놓으면 몇 번째로 갔는지가 보이지 않는다.
   *  그래서 macOS 기본 앱과 같은 규약을 쓴다 — Enter 로 원문에 내려서고, 그다음은 ⌘G 로 돈다. */
  const focusSrcHit = useCallback(
    (hits: [number, number][], i: number) => {
      const el = sourceRef?.current;
      const hit = hits[i];
      if (!el || !hit) return;
      el.focus();
      el.setSelectionRange(hit[0], hit[1]);
      scrollSrcTo(hit[0]);
    },
    [sourceRef, scrollSrcTo],
  );

  const run = useCallback(
    (q: string) => {
      const src = sourceRef?.current;
      if (src) {
        // 편집 갈래 — 미리보기는 칠하지 않는다. 원문과 렌더는 글자가 달라(마크다운 기호) 개수가
        // 어긋나는데, 7/14 라고 해 놓고 다른 데를 칠하면 어느 쪽을 세고 있는지 알 수 없어진다
        clearPaint(KEY_ALL, KEY_CUR);
        const hits = findInText(src.value, q);
        srcHitsRef.current = hits;
        hitsRef.current = [];
        setCount(hits.length);
        setIdx(0);
        // 타이핑 중에는 포커스를 가져오지 않는다 — 두 번째 글자를 못 친다.
        // 자리만 보여 주고, 내려서는 건 Enter 가 한다
        if (hits.length) scrollSrcTo(hits[0][0]);
        return;
      }
      const root = containerRef.current;
      if (!root) return;
      const hits = findRanges(root, q);
      hitsRef.current = hits;
      srcHitsRef.current = [];
      setCount(hits.length);
      setIdx(0);
      paint(KEY_ALL, hits);
      if (hits.length) focusHit(hits, 0);
      else clearPaint(KEY_CUR);
    },
    [containerRef, sourceRef, focusHit, scrollSrcTo],
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
      const src = srcHitsRef.current;
      const dom = hitsRef.current;
      const n = src.length || dom.length;
      if (!n) return;
      const next = (((idx + delta) % n) + n) % n;
      setIdx(next);
      if (src.length) focusSrcHit(src, next);
      else focusHit(dom, next);
    },
    [idx, focusHit, focusSrcHit],
  );

  // 읽기↔편집을 오가면 찾을 대상이 바뀐다 — 열려 있으면 그 자리에서 다시 훑는다
  const runRef = useRef(run);
  runRef.current = run;
  useEffect(() => {
    if (open && query.trim()) runRef.current(query);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sourceRef]);

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
  return (
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
  );
}
