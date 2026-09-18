// 파일 트리 안에서 찾기(⌘F) — 필기노트와 다이어그램 트리가 공유한다.
// 본문 검색(PageFind)과 나뉘어 있는 이유는 대상이 다르기 때문이다: 본문 검색은 그려진 글자를
// 칠하지만, 트리에서 찾는 것은 대개 **접힌 폴더 안에 있는 노트**라 DOM 에 없다. 그래서 여기서는
// 자료구조를 걸러(lib/treeFind) 맞은 것만 남기고 조상을 자동으로 펼친다.
//
// 단축키가 겹치지 않는 이유: ⌘F 를 트리 패널의 keydown 에서 받고 stopPropagation 한다.
// 포커스가 트리 안에 있을 때만 그 핸들러까지 올라오므로, 본문에 있을 때는 PageFind 가 그대로 받는다.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Icon } from "../icons";
import { t } from "../lib/i18n";
import { filterTree, splitHit, type FindableNode, type TreeMatch } from "../lib/treeFind";

/** 행을 찾아 스크롤하려고 트리 행에 붙이는 속성 이름 */
export const TREE_ROW_ATTR = "data-tree-path";

export function useTreeFind<T extends FindableNode>(
  tree: T[],
  /** Enter 로 현재 항목을 열 때 */
  onOpen: (path: string, isDir: boolean) => void,
) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [idx, setIdx] = useState(0);
  const paneRef = useRef<HTMLElement | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const result = useMemo(
    () =>
      open
        ? filterTree(tree, query)
        : { nodes: tree, matches: [] as TreeMatch[], expand: new Set<string>() },
    [open, tree, query],
  );
  const current: TreeMatch | null = result.matches[idx] ?? null;

  // 검색어가 바뀌면 처음 것으로. 결과가 줄어 idx 가 범위를 벗어나는 것도 여기서 막는다
  useEffect(() => {
    setIdx((i) => (i < result.matches.length ? i : 0));
  }, [result.matches.length]);

  /** 현재 항목을 화면 안으로 — 행은 필터 결과가 그려진 뒤에 생기므로 한 프레임 미룬다 */
  useEffect(() => {
    if (!current) return;
    const id = requestAnimationFrame(() => {
      const el = paneRef.current?.querySelector(`[${TREE_ROW_ATTR}="${CSS.escape(current.path)}"]`);
      el?.scrollIntoView({ block: "nearest" });
    });
    return () => cancelAnimationFrame(id);
  }, [current]);

  const close = useCallback(() => {
    setOpen(false);
    setQuery("");
    setIdx(0);
  }, []);

  const start = useCallback(() => {
    setOpen(true);
    requestAnimationFrame(() => inputRef.current?.select());
  }, []);

  const step = useCallback(
    (delta: number) => {
      const n = result.matches.length;
      if (!n) return;
      setIdx((i) => (((i + delta) % n) + n) % n);
    },
    [result.matches.length],
  );

  /** 트리 패널에 그대로 펼치는 속성 — ⌘F 를 여기서 받아 본문 검색으로 새지 않게 한다 */
  const paneProps = {
    ref: (el: HTMLElement | null) => {
      paneRef.current = el;
    },
    // 행을 클릭하면 이 패널이 포커스를 가져간다 → 그 뒤의 ⌘F 가 트리로 온다
    tabIndex: -1,
    onKeyDown: (e: React.KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && !e.altKey && e.key.toLowerCase() === "f") {
        if (document.querySelector(".overlay, .mmd-zoom-overlay")) return; // 모달이 우선
        e.preventDefault();
        e.stopPropagation(); // PageFind 의 window 리스너까지 가지 않게
        start();
      }
    },
  };

  return {
    open,
    query,
    idx,
    current,
    /** 화면에 그릴 트리 — 찾는 중이면 걸러진 것 */
    nodes: result.nodes,
    matches: result.matches,
    /** 찾는 동안 임시로 펼쳐 둘 폴더 */
    autoExpand: result.expand,
    paneProps,
    inputRef,
    start,
    close,
    step,
    onOpen,
    setQuery,
    setIdx,
  };
}

/** 찾기 줄이 쓰는 부분만 — 트리 노드 타입과 무관하다 */
export type TreeFind = Omit<ReturnType<typeof useTreeFind<FindableNode>>, "nodes" | "autoExpand" | "paneProps">;

/** 트리 머리 아래 붙는 찾기 줄 */
export function TreeFindBar({ find }: { find: TreeFind }) {
  if (!find.open) return null;
  const count = find.matches.length;
  return (
    <div className="page-find tree-find">
      <Icon name="search" size={14} />
      <input
        ref={find.inputRef}
        className="page-find-input"
        autoFocus
        placeholder={t("common.find.treePh")}
        value={find.query}
        onChange={(e) => {
          find.setQuery(e.target.value);
          find.setIdx(0);
        }}
        onKeyDown={(e) => {
          if (e.nativeEvent.isComposing) return;
          if (e.key === "Escape") {
            e.stopPropagation();
            find.close();
          }
          if (e.key === "ArrowDown") {
            e.preventDefault();
            find.step(1);
          }
          if (e.key === "ArrowUp") {
            e.preventDefault();
            find.step(-1);
          }
          if (e.key === "Enter") {
            e.preventDefault();
            // Enter 는 지금 것을 연다. 다음으로 넘기는 건 ↑↓ — 열면서 동시에 넘어가면
            // 어느 것을 열었는지 놓친다 (본문 검색의 Enter=다음 과 다른 이유)
            if (find.current) find.onOpen(find.current.path, find.current.isDir);
          }
        }}
      />
      <span className="page-find-count">{count ? `${find.idx + 1}/${count}` : find.query.trim() ? "0" : ""}</span>
      <button
        className="page-find-nav"
        aria-label={t("common.find.prev")}
        onClick={() => find.step(-1)}
        disabled={!count}
      >
        <Icon name="chevron-up" size={14} />
      </button>
      <button
        className="page-find-nav"
        aria-label={t("common.find.next")}
        onClick={() => find.step(1)}
        disabled={!count}
      >
        <Icon name="chevron-down" size={14} />
      </button>
      <button className="page-find-nav" aria-label={t("common.close")} onClick={find.close}>
        <Icon name="x" size={14} />
      </button>
    </div>
  );
}

/** 트리 라벨 — 찾는 중이면 맞은 글자만 칠한다 */
export function TreeLabel({ name, query }: { name: string; query: string }) {
  if (!query.trim()) return <>{name}</>;
  return (
    <>
      {splitHit(name, query).map((p, i) =>
        p.hit ? (
          <mark key={i} className="tree-hit">
            {p.text}
          </mark>
        ) : (
          <span key={i}>{p.text}</span>
        ),
      )}
    </>
  );
}
