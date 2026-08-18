import { useEffect, useMemo, useRef, useState } from "react";
import { emitTo, listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import "./styles.css";
import type { AppConfig } from "./lib/config";
import { loadConfig } from "./lib/config";
import { useAnyReportGenerating } from "./lib/reportRun";
import type {
  ConceptFilter,
  ConceptSort,
  ConceptStatus,
  ConceptWithTags,
} from "./types";
import { allTags, listConcepts, statusCounts } from "./lib/db";
import {
  ConfidenceDots,
  Select,
  StatusBadge,
  TagChip,
  Tooltip,
  UnsavedModal,
} from "./ui";
import { AmberMark, Icon } from "./icons";
import { ConceptDetail } from "./components/ConceptDetail";
import { AddConceptModal } from "./components/AddConceptModal";
import { SettingsModal } from "./components/SettingsModal";
import { NotesView } from "./components/NotesView";
import { AiOnboarding } from "./components/AiOnboarding";
import { DiagramsView } from "./components/DiagramsView";
import { TodoView } from "./components/TodoView";
import { SearchModal, type SearchHit } from "./components/SearchModal";
import { THEME_EVENT, resolvedTheme, toggleTheme } from "./lib/theme";
import { OPEN_CONCEPT, OPEN_NOTE, openDiagramInApp, openNoteInApp } from "./lib/nav";
import { t } from "./lib/i18n";
import { usePaneResize } from "./lib/usePaneResize";

type StatusTab = ConceptStatus | "all";
type Section = "til" | "notes" | "diagrams" | "todo";

const SORTS: { id: ConceptSort; label: string }[] = [
  { id: "canonical", label: t("app.sort.canonical") },
  { id: "recent_updated", label: t("app.sort.recentUpdated") },
  { id: "recent_created", label: t("app.sort.recentCreated") },
  { id: "title", label: t("app.sort.title") },
];

const TABS: { id: StatusTab; label: string }[] = [
  { id: "learning", label: t("common.status.learning") },
  { id: "learned", label: t("common.status.learned") },
  { id: "all", label: t("app.tab.all") },
];

// 탭별 기본 정렬 (사용자가 바꾸면 그 탭 한정으로 기억)
const DEFAULT_SORT: Record<StatusTab, ConceptSort> = {
  learning: "canonical",
  learned: "recent_updated",
  all: "recent_updated",
};

// 레일 = 최상위 작업공간. 순서가 곧 ⌘1~4 의 번호라 모듈 상수로 고정한다
const RAIL: {
  id: Section;
  label: string;
  icon: "layers" | "book" | "workflow" | "calendar-check";
}[] = [
  { id: "todo", label: t("app.rail.todo"), icon: "calendar-check" },
  { id: "til", label: t("app.rail.til"), icon: "layers" },
  { id: "notes", label: t("app.rail.notes"), icon: "book" },
  { id: "diagrams", label: t("app.rail.diagrams"), icon: "workflow" },
];


function App() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  // 섹션 (TIL 개념 / 필기노트). 마지막 선택을 기억
  const [section, setSection] = useState<Section>(() =>
    ((): Section => {
      const s = localStorage.getItem("til.section");
      return s === "notes" || s === "diagrams" || s === "todo" ? s : "til";
    })(),
  );
  useEffect(() => {
    localStorage.setItem("til.section", section);
  }, [section]);

  const pane = usePaneResize({
    storageKey: "amber.concepts.list-width",
    active: section === "til",
  });

  const [status, setStatus] = useState<StatusTab>("learning");
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const [activeTags, setActiveTags] = useState<string[]>([]);
  const [sortByTab, setSortByTab] =
    useState<Record<StatusTab, ConceptSort>>(DEFAULT_SORT);
  const sort = sortByTab[status];
  const setSort = (s: ConceptSort) =>
    setSortByTab((m) => ({ ...m, [status]: s }));

  const [concepts, setConcepts] = useState<ConceptWithTags[]>([]);
  const [counts, setCounts] = useState({ learning: 0, learned: 0, all: 0 });
  const [tags, setTags] = useState<{ name: string; count: number }[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);

  const [addOpen, setAddOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  // 리포트가 백그라운드로 생성 중이면 어느 탭에 있든 할 일 레일에 표시(진행이 안 끊김을 알림)
  const reportBusy = useAnyReportGenerating();
  const [ready, setReady] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);

  // 테마 토글 아이콘 동기화 (설정/시스템/토글 어디서 바뀌어도 반영)
  const [isDark, setIsDark] = useState(() => resolvedTheme() === "dark");
  useEffect(() => {
    const h = () => setIsDark(resolvedTheme() === "dark");
    window.addEventListener(THEME_EVENT, h);
    return () => window.removeEventListener(THEME_EVENT, h);
  }, []);

  useEffect(() => {
    loadConfig().then(setConfig).catch((e) => setLoadError(String(e)));
  }, []);

  // 개념 상세에 미저장 초안이 있는지 — 필터/선택 변경이 ConceptDetail 을 언마운트해
  // 초안을 날리는 걸 막는다 (NotesView 의 pendingOpen 과 같은 패턴)
  const [detailDirty, setDetailDirty] = useState(false);
  const [pendingNav, setPendingNav] = useState<{ run: () => void } | null>(null);
  /** 초안이 있으면 확인 모달로 미루고, 없으면 그대로 실행 */
  const guard = (run: () => void) => {
    if (detailDirty) setPendingNav({ run });
    else run();
  };

  useEffect(() => {
    // 편집 중에는 필터 확정을 미룬다 — 매 글자마다 확인 모달을 띄우는 대신
    // 목록 재필터만 늦춘다. 저장/취소로 초안이 사라지면 바로 반영된다.
    if (detailDirty) return;
    const t = setTimeout(() => setSearch(searchInput), 200);
    return () => clearTimeout(t);
  }, [searchInput, detailDirty]);

  const filter: ConceptFilter = useMemo(
    () => ({ status, search, tags: activeTags, sort }),
    [status, search, activeTags, sort],
  );

  // 세대 번호 — 디바운스 검색이 겹치면 reload 가 큐잉되고, 늦게 도착한 옛 응답이
  // 최신 목록을 덮어쓸 수 있다. selected 가 이 목록에서 파생되므로 상세 판까지 흔들린다.
  const reloadSeq = useRef(0);

  async function reload() {
    const seq = ++reloadSeq.current;
    try {
      const [list, cnt, tg] = await Promise.all([
        listConcepts(filter),
        statusCounts(),
        allTags(),
      ]);
      if (seq !== reloadSeq.current) return; // 밀려난 응답은 버린다
      setConcepts(list);
      setCounts(cnt);
      setTags(tg);
      setReady(true);
      setLoadError(null);
    } catch (e) {
      if (seq !== reloadSeq.current) return;
      setLoadError(String(e));
    }
  }

  useEffect(() => {
    if (config) reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config, filter]);

  // 최신 reload 를 ref 로 유지(리스너 stale closure 방지)
  const reloadRef = useRef(reload);
  reloadRef.current = reload;

  // 개념 선택 이동 공통 로직 (필터에 가려 안 보이는 일 없게 전체 탭 + 필터 초기화)
  const goToConcept = (id: number) =>
    guard(() => {
      setSection("til");
      setStatus("all");
      setActiveTags([]);
      setSearchInput("");
      setSearch("");
      setSelectedId(id);
    });

  // 리스너가 [] deps 로 한 번만 붙으므로 ref 로 최신 구현을 본다 (초안 가드가 stale 이 되지 않게)
  const goToConceptRef = useRef(goToConcept);
  goToConceptRef.current = goToConcept;

  // 위젯 등 다른 창에서의 변경/열기 요청 수신
  useEffect(() => {
    const uns = [
      listen("concept-changed", () => reloadRef.current()),
      listen<{ id: number }>("open-concept", (e) =>
        goToConceptRef.current(e.payload.id),
      ),
    ];
    return () => uns.forEach((u) => u.then((f) => f()));
  }, []);

  // 앱 내 노트↔개념 상호 이동 (같은 창, window CustomEvent)
  useEffect(() => {
    const onConcept = (e: Event) =>
      goToConceptRef.current((e as CustomEvent<{ id: number }>).detail.id);
    const onNote = () => setSection("notes"); // NotesView 가 실제 파일 열기 처리
    window.addEventListener(OPEN_CONCEPT, onConcept);
    window.addEventListener(OPEN_NOTE, onNote);
    return () => {
      window.removeEventListener(OPEN_CONCEPT, onConcept);
      window.removeEventListener(OPEN_NOTE, onNote);
    };
  }, []);

  // 빠른 검색 결과 열기 — 섹션 전환/선택은 기존 경로(nav 이벤트·goToConcept)를 그대로 탄다
  const openHit = (h: SearchHit) => {
    setSearchOpen(false);
    if (h.kind === "concept") {
      goToConcept(h.id);
    } else if (h.kind === "note") {
      openNoteInApp(h.path); // 아래 OPEN_NOTE 리스너가 섹션 전환, NotesView 가 파일 열기
    } else {
      setSection("diagrams");
      openDiagramInApp(h.path); // DiagramsView 의 OPEN_DIAGRAM 리스너가 파일을 연다
    }
  };

  // ⌘K 빠른 검색 · ⌘1~4 레일 전환. 섹션 단축키(⌘S)와 달리 앱 전역이라 activeRef 대신
  // "모달이 떠 있으면 양보" 로 가린다 — 안 보이는 화면이 뒤에서 바뀌지 않게.
  const shieldedRef = useRef(false);
  shieldedRef.current = addOpen || settingsOpen || searchOpen;
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (shieldedRef.current) return;
      if (!(e.metaKey || e.ctrlKey) || e.altKey || e.shiftKey) return;
      const k = e.key.toLowerCase();
      if (k === "k") {
        e.preventDefault();
        setSearchOpen(true);
        return;
      }
      // ⌘F = 보관함 검색(⌘K 와 같은 창), ⌘, = 설정 — PRD MVP 단축키 명세
      if (k === "f") {
        e.preventDefault();
        setSearchOpen(true);
        return;
      }
      if (e.key === ",") {
        e.preventDefault();
        setSettingsOpen(true);
        return;
      }
      const i = Number(e.key) - 1;
      if (Number.isInteger(i) && i >= 0 && i < RAIL.length) {
        e.preventDefault();
        setSection(RAIL[i].id);
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  const selected = concepts.find((c) => c.id === selectedId) ?? null;

  function toggleTag(name: string) {
    guard(() =>
      setActiveTags((prev) =>
        prev.includes(name) ? prev.filter((t) => t !== name) : [...prev, name],
      ),
    );
  }

  async function showWidget() {
    const w = await WebviewWindow.getByLabel("widget");
    if (w) {
      await w.show();
      await w.setFocus();
    }
  }

  const notifyWidget = () => {
    void emitTo("widget", "concept-changed", {});
  };

  const countOf = (id: StatusTab) =>
    id === "learning" ? counts.learning : id === "learned" ? counts.learned : counts.all;

  return (
    <div className="app">
      {/* 좌측 레일 = 최상위 작업공간 전환(세로축). 상단 필터 탭(가로축)과 축을 분리해 계층 혼동 제거 */}
      <nav className="rail">
        <div className="rail-brand" title="Amber">
          <AmberMark size={30} />
          <span>Amber</span>
        </div>
        {RAIL.map((r) => (
          <button
            key={r.id}
            className={`rail-item ${section === r.id ? "active" : ""}`}
            onClick={() => setSection(r.id)}
            title={
              r.id === "todo" && reportBusy
                ? `${r.label} · ${t("app.rail.reportBusy")}`
                : r.label
            }
          >
            <Icon name={r.icon} size={20} />
            <span>{r.label}</span>
            {r.id === "todo" && reportBusy && (
              <span className="rail-busy" aria-label={t("app.rail.reportBusyAria")} />
            )}
          </button>
        ))}
        <span className="rail-spacer" />
        <button
          className="rail-item"
          onClick={() => setSettingsOpen(true)}
          title={t("app.rail.settings")}
        >
          <Icon name="settings" size={20} />
          <span>{t("app.rail.settings")}</span>
        </button>
      </nav>

      <div className="workspace">
      <header className="topbar">
        <span className="brand">
          {section === "til"
            ? t("app.rail.til")
            : section === "notes"
              ? t("app.rail.notes")
              : section === "diagrams"
                ? t("app.rail.diagrams")
                : t("app.rail.todo")}
        </span>
        {section === "til" && (
          <div className="search-wrap">
            <Icon name="search" size={15} className="search-icon" />
            <input
              className="search"
              placeholder={t("app.search.placeholder")}
              value={searchInput}
              onChange={(e) => setSearchInput(e.target.value)}
            />
          </div>
        )}
        <span className="spacer" />
        {section === "til" && (
          <button
            className="btn btn-primary"
            onClick={() => setAddOpen(true)}
          >
            <Icon name="plus" size={15} />
            {t("app.add")}
          </button>
        )}
        <Tooltip label={isDark ? t("app.theme.toLight") : t("app.theme.toDark")}>
          <button
            aria-label={isDark ? t("app.theme.toLight") : t("app.theme.toDark")}
            className="icon-btn"
            onClick={toggleTheme}
          >
            <Icon name={isDark ? "sun" : "moon"} size={17} />
          </button>
        </Tooltip>
        <button className="btn" onClick={showWidget} title={t("app.widget.open")}>
          <Icon name="panel" size={15} />
          {t("app.widget.label")}
        </button>
      </header>

      {/* TIL 섹션 — 노트로 전환해도 언마운트하지 않고 숨김 (스크롤/선택 보존) */}
      <div className={`section-wrap ${section === "til" ? "" : "hidden"}`}>
      <div className="tabsbar">
        <div className="segmented">
          {TABS.map((t) => (
            <button
              key={t.id}
              className={`tab ${status === t.id ? "active" : ""}`}
              onClick={() => guard(() => setStatus(t.id))}
            >
              {t.label}
              <span className="count">{countOf(t.id)}</span>
            </button>
          ))}
        </div>
        <span className="spacer" />
        <Select
          value={sort}
          align="right"
          options={SORTS.map((s) => ({ value: s.id, label: s.label }))}
          onChange={setSort}
        />
      </div>

      {tags.length > 0 && (
        <div className="tagbar">
          {tags.slice(0, 12).map((t) => (
            <TagChip
              key={t.name}
              label={t.name}
              active={activeTags.includes(t.name)}
              onClick={() => toggleTag(t.name)}
            />
          ))}
          {activeTags.length > 0 && (
            <span className="chip btn-like" onClick={() => guard(() => setActiveTags([]))}>
              {t("app.filter.clear")}
            </span>
          )}
        </div>
      )}

      <div className="body" {...pane.bodyProps}>
        <aside className="list">
          {loadError && (
            <div className="error-note" style={{ margin: 12 }}>
              {loadError}
            </div>
          )}
          {ready && concepts.length === 0 && !loadError && (
            <div className="empty" style={{ height: "auto", padding: 40 }}>
              {status === "learning" ? t("app.empty.learning") : t("app.empty.noResults")}
              <button className="btn btn-primary btn-sm" onClick={() => setAddOpen(true)}>
                <Icon name="plus" size={14} />
                {t("app.empty.addFirst")}
              </button>
            </div>
          )}
          {concepts.map((c) => (
            <div
              key={c.id}
              className={`row ${selectedId === c.id ? "selected" : ""}`}
              onClick={() => guard(() => setSelectedId(c.id))}
            >
              <div className="row-top">
                <span className="row-title">{c.title}</span>
                <ConfidenceDots value={c.confidence} />
              </div>
              <div className="row-summary">{c.summary}</div>
              <div className="row-meta">
                <StatusBadge status={c.status} />
                {c.tags.slice(0, 3).map((t) => (
                  <span className="chip" key={t}>
                    #{t}
                  </span>
                ))}
              </div>
            </div>
          ))}
        </aside>

        <div {...pane.resizerProps} />

        <section className="detail">
          {selected ? (
            <ConceptDetail
              key={selected.id}
              concept={selected}
              config={config}
              onDirtyChange={setDetailDirty}
              onChanged={(opts) => {
                if (opts?.deleted) setSelectedId(null);
                reload();
                notifyWidget();
              }}
            />
          ) : (
            <div className="empty">{t("app.empty.selectConcept")}</div>
          )}
        </section>
      </div>
      </div>

      {/* 필기노트 섹션 — 편집 초안 보존을 위해 항상 마운트 */}
      <div className={`section-wrap ${section === "notes" ? "" : "hidden"}`}>
        <NotesView active={section === "notes"} config={config} />
      </div>

      {/* 다이어그램 섹션 — 편집 초안 보존을 위해 항상 마운트 */}
      <div className={`section-wrap ${section === "diagrams" ? "" : "hidden"}`}>
        <DiagramsView active={section === "diagrams"} config={config} />
      </div>

      {/* 할 일 섹션 — 선택 날짜/스크롤 보존을 위해 항상 마운트 */}
      <div className={`section-wrap ${section === "todo" ? "" : "hidden"}`}>
        <TodoView
          active={section === "todo"}
          config={config}
          onOpenSettings={() => setSettingsOpen(true)}
        />
      </div>
      </div>

      {config && (
        <AddConceptModal
          open={addOpen}
          onClose={() => setAddOpen(false)}
          onCreated={() => {
            reload();
            notifyWidget();
          }}
          config={config}
        />
      )}
      {/* 최초 실행 시 AI CLI 감지·연결 온보딩 */}
      {config && !settingsOpen && (
        <AiOnboarding open={!config.onboarded} onDone={setConfig} />
      )}
      <SettingsModal
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        onSaved={setConfig}
      />
      {/* 보관함 전체 빠른 검색 (⌘K) */}
      <SearchModal
        open={searchOpen}
        onClose={() => setSearchOpen(false)}
        onOpenHit={openHit}
      />
      {/* 개념 편집 초안이 있는데 필터/선택을 바꾸려 할 때 */}
      <UnsavedModal
        open={!!pendingNav}
        onKeep={() => setPendingNav(null)}
        onDiscard={() => {
          const p = pendingNav;
          setPendingNav(null);
          setDetailDirty(false);
          p?.run();
        }}
      />
    </div>
  );
}

export default App;
