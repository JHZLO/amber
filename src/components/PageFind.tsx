// 페이지 내 검색 바(⌘F) — 필기노트·개념·투두가 공유한다.
// 각 화면이 자기 검색을 따로 만들면 단축키·표시·이동 규칙이 화면마다 갈린다.
// 여기서는 "보이는 텍스트를 찾아 칠하고 그 자리로 스크롤" 하나만 한다.

import { useCallback, useEffect, useRef, useState } from "react";
import { Icon } from "../icons";
import { t } from "../lib/i18n";
import { clearPaint, findRanges, paint } from "../lib/pageFind";

const KEY_ALL = "page-find";
const KEY_CUR = "page-find-cur";

export function PageFind({
  containerRef,
  active,
}: {
  /** 검색 대상 — 이 요소 안의 텍스트만 찾는다 */
  containerRef: React.RefObject<HTMLElement | null>;
  /** 이 섹션이 화면에 있는가 (안 보이는 화면이 단축키를 가로채지 않게) */
  active: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [idx, setIdx] = useState(0);
  const [count, setCount] = useState(0);
  const hitsRef = useRef<Range[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);

  const close = useCallback(() => {
    setOpen(false);
    setQuery("");
    setCount(0);
    setIdx(0);
    hitsRef.current = [];
    clearPaint(KEY_ALL, KEY_CUR);
  }, []);

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

  const run = useCallback(
    (q: string) => {
      const root = containerRef.current;
      if (!root) return;
      const hits = findRanges(root, q);
      hitsRef.current = hits;
      setCount(hits.length);
      setIdx(0);
      paint(KEY_ALL, hits);
      if (hits.length) focusHit(hits, 0);
      else clearPaint(KEY_CUR);
    },
    [containerRef, focusHit],
  );

  const step = useCallback(
    (delta: number) => {
      const hits = hitsRef.current;
      if (!hits.length) return;
      const next = (((idx + delta) % hits.length) + hits.length) % hits.length;
      setIdx(next);
      focusHit(hits, next);
    },
    [idx, focusHit],
  );

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
  useEffect(() => () => clearPaint(KEY_ALL, KEY_CUR), []);

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
          run(e.target.value);
        }}
        onKeyDown={(e) => {
          if (e.nativeEvent.isComposing) return;
          if (e.key === "Enter") step(e.shiftKey ? -1 : 1);
          if (e.key === "Escape") close();
        }}
      />
      <span className="page-find-count">
        {count ? `${idx + 1}/${count}` : query.trim() ? "0" : ""}
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
