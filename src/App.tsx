import { useEffect, useMemo, useRef, useState } from "react";
import { emitTo, listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import "./styles.css";
import type { AppConfig } from "./lib/config";
import { loadConfig } from "./lib/config";
import type {
  ConceptFilter,
  ConceptSort,
  ConceptStatus,
  ConceptWithTags,
} from "./types";
import { allTags, listConcepts, statusCounts } from "./lib/db";
import { ConfidenceDots, Select, StatusBadge, TagChip } from "./ui";
import { Icon } from "./icons";
import { ConceptDetail } from "./components/ConceptDetail";
import { AddConceptModal } from "./components/AddConceptModal";
import { SettingsModal } from "./components/SettingsModal";
import { NotesView } from "./components/NotesView";
import { DiagramsView } from "./components/DiagramsView";
import { THEME_EVENT, resolvedTheme, toggleTheme } from "./lib/theme";

type StatusTab = ConceptStatus | "all";
type Section = "til" | "notes" | "diagrams";

const SORTS: { id: ConceptSort; label: string }[] = [
  { id: "canonical", label: "자신감 낮은 순" },
  { id: "recent_updated", label: "최근 수정순" },
  { id: "recent_created", label: "최근 추가순" },
  { id: "title", label: "제목순" },
];

const TABS: { id: StatusTab; label: string }[] = [
  { id: "learning", label: "학습중" },
  { id: "learned", label: "학습완료" },
  { id: "all", label: "전체" },
];

// 탭별 기본 정렬 (사용자가 바꾸면 그 탭 한정으로 기억)
const DEFAULT_SORT: Record<StatusTab, ConceptSort> = {
  learning: "canonical",
  learned: "recent_updated",
  all: "recent_updated",
};

function App() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  // 섹션 (TIL 개념 / 필기노트). 마지막 선택을 기억
  const [section, setSection] = useState<Section>(() =>
    ((): Section => {
      const s = localStorage.getItem("til.section");
      return s === "notes" || s === "diagrams" ? s : "til";
    })(),
  );
  useEffect(() => {
    localStorage.setItem("til.section", section);
  }, [section]);

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

  useEffect(() => {
    const t = setTimeout(() => setSearch(searchInput), 200);
    return () => clearTimeout(t);
  }, [searchInput]);

  const filter: ConceptFilter = useMemo(
    () => ({ status, search, tags: activeTags, sort }),
    [status, search, activeTags, sort],
  );

  async function reload() {
    try {
      const [list, cnt, tg] = await Promise.all([
        listConcepts(filter),
        statusCounts(),
        allTags(),
      ]);
      setConcepts(list);
      setCounts(cnt);
      setTags(tg);
      setReady(true);
      setLoadError(null);
    } catch (e) {
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

  // 위젯 등 다른 창에서의 변경/열기 요청 수신
  useEffect(() => {
    const uns = [
      listen("concept-changed", () => reloadRef.current()),
      listen<{ id: number }>("open-concept", (e) => {
        // 필터에 가려 안 보이는 일이 없도록 전체 탭 + 필터 초기화 후 선택
        setSection("til");
        setStatus("all");
        setActiveTags([]);
        setSearchInput("");
        setSearch("");
        setSelectedId(e.payload.id);
      }),
    ];
    return () => uns.forEach((u) => u.then((f) => f()));
  }, []);

  const selected = concepts.find((c) => c.id === selectedId) ?? null;

  function toggleTag(name: string) {
    setActiveTags((prev) =>
      prev.includes(name) ? prev.filter((t) => t !== name) : [...prev, name],
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

  const RAIL: {
    id: Section;
    label: string;
    icon: "layers" | "book" | "workflow";
  }[] = [
    { id: "til", label: "개념", icon: "layers" },
    { id: "notes", label: "필기노트", icon: "book" },
    { id: "diagrams", label: "다이어그램", icon: "workflow" },
  ];

  return (
    <div className="app">
      {/* 좌측 레일 = 최상위 작업공간 전환(세로축). 상단 필터 탭(가로축)과 축을 분리해 계층 혼동 제거 */}
      <nav className="rail">
        <div className="rail-brand">Amber</div>
        {RAIL.map((r) => (
          <button
            key={r.id}
            className={`rail-item ${section === r.id ? "active" : ""}`}
            onClick={() => setSection(r.id)}
            title={r.label}
          >
            <Icon name={r.icon} size={20} />
            <span>{r.label}</span>
          </button>
        ))}
        <span className="rail-spacer" />
        <button
          className="rail-item"
          onClick={() => setSettingsOpen(true)}
          title="설정"
        >
          <Icon name="settings" size={20} />
          <span>설정</span>
        </button>
      </nav>

      <div className="workspace">
      <header className="topbar">
        <span className="brand">
          {section === "til"
            ? "개념"
            : section === "notes"
              ? "필기노트"
              : "다이어그램"}
        </span>
        {section === "til" && (
          <div className="search-wrap">
            <Icon name="search" size={15} className="search-icon" />
            <input
              className="search"
              placeholder="검색 (제목·요약·태그)…"
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
            disabled={!config}
          >
            <Icon name="plus" size={15} />
            추가
          </button>
        )}
        <button
          className="icon-btn"
          onClick={toggleTheme}
          title={isDark ? "라이트 모드로" : "다크 모드로"}
        >
          <Icon name={isDark ? "sun" : "moon"} size={17} />
        </button>
        <button className="btn" onClick={showWidget} title="바탕화면 위젯 열기">
          <Icon name="panel" size={15} />
          위젯
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
              onClick={() => setStatus(t.id)}
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
            <span className="chip btn-like" onClick={() => setActiveTags([])}>
              필터 초기화
            </span>
          )}
        </div>
      )}

      <div className="body">
        <aside className="list">
          {loadError && (
            <div className="error-note" style={{ margin: 12 }}>
              {loadError}
            </div>
          )}
          {ready && concepts.length === 0 && !loadError && (
            <div className="empty" style={{ height: "auto", padding: 40 }}>
              {status === "learning" ? "학습 중인 개념이 없어요." : "결과가 없어요."}
              <button className="btn btn-primary btn-sm" onClick={() => setAddOpen(true)}>
                <Icon name="plus" size={14} />첫 개념 추가
              </button>
            </div>
          )}
          {concepts.map((c) => (
            <div
              key={c.id}
              className={`row ${selectedId === c.id ? "selected" : ""}`}
              onClick={() => setSelectedId(c.id)}
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

        <section className="detail">
          {selected ? (
            <ConceptDetail
              key={selected.id}
              concept={selected}
              config={config}
              onChanged={(opts) => {
                if (opts?.deleted) setSelectedId(null);
                reload();
                notifyWidget();
              }}
            />
          ) : (
            <div className="empty">개념을 선택하면 상세가 여기 나와요.</div>
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
        <DiagramsView active={section === "diagrams"} />
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
      <SettingsModal
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        onSaved={setConfig}
      />
    </div>
  );
}

export default App;
