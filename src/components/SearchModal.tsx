// 보관함 전체 빠른 검색(⌘K) — 필기노트·다이어그램 파일(이름+본문)과 개념을 한 자리에서.
// 파일 검색은 lib/vaultTree 의 searchFiles(substring), 개념은 기존 listConcepts 의 LIKE 를 그대로 탄다.
// 결과를 여는 것(섹션 전환·선택)은 App 의 몫 — 여기서는 고른 항목만 올려보낸다.

import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Modal } from "../ui";
import { Icon } from "../icons";
import { searchFiles, parentOf } from "../lib/vaultTree";
import { listNoteTree, readNoteFile } from "../lib/notes";
import { listDiagramTree, readDiagramFile } from "../lib/diagrams";
import { listConcepts } from "../lib/db";
import { t } from "../lib/i18n";

export type SearchHit =
  | { kind: "concept"; id: number; title: string; sub: string; where: string }
  | {
      kind: "note" | "diagram";
      path: string;
      title: string;
      sub: string;
      where: string;
    };

const KIND_LABEL: Record<SearchHit["kind"], string> = {
  concept: t("app.quicksearch.kind.concept"),
  note: t("app.quicksearch.kind.note"),
  diagram: t("app.quicksearch.kind.diagram"),
};

// 종류별 상한 — 한 종류가 결과 목록을 독점하지 않게
const PER_KIND = 8;

async function runSearch(q: string): Promise<SearchHit[]> {
  // 루트가 사라졌거나 DB 가 아직 준비 전이면 그 종류만 조용히 비운다(검색 전체를 죽이지 않음)
  const [concepts, notes, diagrams] = await Promise.all([
    // limit 없이 부르면 'a' 한 글자에 아카이브 전체가 IPC 를 건너온다 — 8줄만 그리면서
    listConcepts({
      status: "all",
      search: q,
      sort: "recent_updated",
      limit: PER_KIND,
    }).catch(() => []),
    searchFiles(
      { listTree: listNoteTree, readFile: readNoteFile },
      q,
      PER_KIND,
    ).catch(() => []),
    searchFiles(
      { listTree: listDiagramTree, readFile: readDiagramFile },
      q,
      PER_KIND,
    ).catch(() => []),
  ]);

  return [
    ...concepts.map<SearchHit>((c) => ({
      kind: "concept",
      id: c.id,
      title: c.title,
      sub: c.summary,
      where: c.tags.slice(0, 3).map((t) => `#${t}`).join(" "),
    })),
    ...notes.map<SearchHit>((m) => ({
      kind: "note",
      path: m.path,
      title: m.name,
      sub: m.snippet ?? "",
      where: parentOf(m.path),
    })),
    ...diagrams.map<SearchHit>((m) => ({
      kind: "diagram",
      path: m.path,
      title: m.name,
      sub: m.snippet ?? "",
      where: parentOf(m.path),
    })),
  ];
}

export function SearchModal({
  open,
  onClose,
  onOpenHit,
}: {
  open: boolean;
  onClose: () => void;
  onOpenHit: (hit: SearchHit) => void;
}) {
  const [input, setInput] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [idx, setIdx] = useState(0);
  const [searching, setSearching] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);

  // 닫으면 초기화 — 다음 ⌘K 는 항상 빈 상태로 시작한다
  useEffect(() => {
    if (open) return;
    setInput("");
    setHits([]);
    setIdx(0);
  }, [open]);

  // 200ms 디바운스 (개념 탭 검색과 같은 간격). alive 플래그로 늦게 온 응답이 덮어쓰지 않게
  useEffect(() => {
    if (!open) return;
    const q = input.trim();
    if (!q) {
      setHits([]);
      setIdx(0);
      setSearching(false);
      return;
    }
    let alive = true;
    setSearching(true);
    const t = setTimeout(() => {
      runSearch(q)
        .then((r) => {
          if (!alive) return;
          setHits(r);
          setIdx(0);
        })
        .finally(() => {
          if (alive) setSearching(false);
        });
    }, 200);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [input, open]);

  // 키보드 이동 시 선택 행을 보이는 범위로 (키보드 액션이라 애니메이션 없음)
  useEffect(() => {
    listRef.current
      ?.querySelector(".row.selected")
      ?.scrollIntoView({ block: "nearest" });
  }, [idx, hits]);

  function onKeyDown(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!hits.length) return;
      const d = e.key === "ArrowDown" ? 1 : -1;
      setIdx((i) => (i + d + hits.length) % hits.length);
      return;
    }
    if (e.key !== "Enter") return;
    if (e.nativeEvent.isComposing) return; // 한글 조합 확정 Enter 는 흘려보낸다
    const hit = hits[idx];
    if (hit) onOpenHit(hit);
  }

  return (
    <Modal
      open={open}
      title={t("app.quicksearch.title")}
      onClose={onClose}
      footer={<span className="hint">{t("app.quicksearch.hint")}</span>}
    >
      <div className="search-wrap" style={{ maxWidth: "none" }}>
        <Icon name="search" size={15} className="search-icon" />
        <input
          className="search"
          autoFocus
          placeholder={t("app.quicksearch.placeholder")}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={onKeyDown}
        />
      </div>

      {/* 결과 높이를 고정해 타이핑 중 모달이 커졌다 작아지지 않게(DESIGN §7). 좌우는 모달 폭까지 흘림 */}
      <div
        ref={listRef}
        style={{ height: 320, overflowY: "auto", margin: "14px -20px -20px" }}
      >
        {hits.map((h, i) => (
          <div
            key={`${h.kind}:${h.kind === "concept" ? h.id : h.path}`}
            className={`row ${i === idx ? "selected" : ""}`}
            onClick={() => onOpenHit(h)}
          >
            <div className="row-top">
              <span className="row-title">{h.title}</span>
            </div>
            {h.sub && <div className="row-summary">{h.sub}</div>}
            <div className="row-meta">
              <span className="chip">{KIND_LABEL[h.kind]}</span>
              {h.where && <span>{h.where}</span>}
            </div>
          </div>
        ))}
        {!hits.length && (
          <div className="empty">
            {!input.trim()
              ? t("app.quicksearch.emptyPrompt")
              : searching
                ? t("app.quicksearch.searching")
                : t("app.empty.noResults")}
          </div>
        )}
      </div>
    </Modal>
  );
}
