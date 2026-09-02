// 다이어그램 섹션: 좌측 폴더 트리 + 우측 mermaid 렌더/편집 2-pane.
// 정본은 vault/diagrams/ 의 실제 디렉토리/.mmd 파일 (lib/diagrams.ts). 트리 UX 는 필기노트와 동일
// (같은 CSS 클래스 재사용). 읽기 = 렌더된 다이어그램(클릭 확대), 편집 = 소스 | 라이브 프리뷰.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  createDiagram,
  createFolder,
  deleteEntry,
  diagramFileExists,
  diagramMtime,
  flattenDirs,
  invalidNameReason,
  invalidPathReason,
  listDiagramTree,
  moveEntry,
  parentOf,
  readDiagramFile,
  renameEntry,
  writeDiagramFile,
  type DiagramNode,
} from "../lib/diagrams";
import { ancestorPaths, remapPath, remapPaths } from "../lib/vaultTree";
import { useTreeDnd } from "../lib/useTreeDnd";
import { usePaneResize } from "../lib/usePaneResize";
import { DiagramCanvas } from "./DiagramCanvas";
import { DiagramAiModal } from "./DiagramAiModal";
import {
  Modal,
  Select,
  Spinner,
  Tooltip,
  TreeDragOverlay,
  UnsavedModal,
  timeAgo,
} from "../ui";
import { Icon } from "../icons";
import { aiOutputLang, t } from "../lib/i18n";
import { errText } from "../lib/errors";
import { RootPicker } from "./RootPicker";
import type { AppConfig } from "../lib/config";
import { rootDisplayName, WORKSPACE_EVENT } from "../lib/workspace";
import { OPEN_DIAGRAM } from "../lib/nav";
import { DbConnectionModal } from "./DbConnectionModal";
import { SchemaOverview } from "./SchemaOverview";
import { DiffView } from "./DiffView";
import {
  DB_CONNECTIONS_EVENT,
  connStatus,
  connStatusDot,
  connStatusLabel,
  deleteConnection,
  enabledSchemas,
  envLabel,
  indexConnections,
  isStale,
  listConnections,
  notifyConnectionsChanged,
  prefAudit,
  readSnapshot,
  remapConnectionFolders,
  schemaFolder,
  setSchemaAudit,
  syncSchema,
  type DbConnection,
  type DbSchemaPref,
} from "../lib/dbconn";
import {
  diffIsEmpty,
  formatDbHeader,
  parseDbHeader,
  type SchemaDiff,
  type SchemaSnapshot,
} from "../lib/schemaSnapshot";
import { ERD_GEN_VERSION, generateErd } from "../lib/erdGen";

// 이동/생성 위치 Select 값 인코딩 (루트 '' ↔ '/')
const encodeDir = (d: string) => (d ? `/${d}` : "/");
const decodeDir = (v: string) => (v === "/" ? "" : v.slice(1));

const errMsg = errText; // Rust 코드화 에러까지 번역 (lib/errors.ts)

type NameModalState =
  | { kind: "new-file" | "new-folder"; name: string; dir: string }
  | { kind: "rename"; name: string; target: DiagramNode };

type DeleteTarget = { name: string; path: string; isDir: boolean };

// 저장하려는 순간 디스크가 더 새로울 때(외부 편집) 사용자에게 넘길 선택지
type ConflictState = { path: string; diskMtime: number };

export function DiagramsView({
  active,
  config,
}: {
  active: boolean;
  config: AppConfig | null;
}) {
  const [tree, setTree] = useState<DiagramNode[] | null>(null);
  const [treeError, setTreeError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  // 한 번이라도 펼친 적 있는 폴더 — 접어도 유지한다(다시 펼칠 때 깜빡이지 않게)
  const [mountedDirs, setMountedDirs] = useState<Set<string>>(new Set());
  const [activeDir, setActiveDir] = useState("");
  const [selected, setSelected] = useState<string | null>(null);

  const [body, setBody] = useState("");
  const [mtime, setMtime] = useState<number | null>(null);
  const [loadingBody, setLoadingBody] = useState(false);
  const [readError, setReadError] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [previewChart, setPreviewChart] = useState(""); // 편집 프리뷰용(디바운스)
  const [busy, setBusy] = useState(false);
  const [opError, setOpError] = useState<string | null>(null);
  const [conflict, setConflict] = useState<ConflictState | null>(null);
  // 열기 세대 번호 — 늦게 도착한 이전 파일의 응답이 지금 버퍼를 덮지 않게
  const openSeq = useRef(0);

  const [nameModal, setNameModal] = useState<NameModalState | null>(null);
  const [modalError, setModalError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<DeleteTarget | null>(null);
  const [pendingOpen, setPendingOpen] = useState<string | null>(null);
  const [pendingRoot, setPendingRoot] = useState(false);
  const [aiOpen, setAiOpen] = useState(false);
  const editorRef = useRef<HTMLTextAreaElement>(null);

  // ---- DB 스키마 연동 (lib/dbconn.ts) ----
  // 연결은 트리 안의 폴더다: folder_path 가 연결 폴더, 그 아래 스키마 폴더. 여기서는 경로로 알아본다.
  const [connections, setConnections] = useState<DbConnection[]>([]);
  const connIndex = useMemo(() => indexConnections(connections), [connections]);
  // 스키마 폴더 경로 → schema.json 스냅샷 (null = 아직 없음)
  const [snapByFolder, setSnapByFolder] = useState<Map<string, SchemaSnapshot | null>>(new Map());
  // 동기화 중인 스키마 폴더 — 트리 행 아이콘이 돈다
  const [syncing, setSyncing] = useState<Set<string>>(new Set());
  // 자동 동기화 차례를 기다리는 스키마 폴더 — 트리 행에 "대기 중". 연결 폴더 행은 진행 n/N 을 보여준다
  const [queued, setQueued] = useState<Set<string>>(new Set());
  const syncingRef = useRef<Set<string>>(new Set());
  // 이 세션에서 동기화로 알게 된 구조 변화 — 배너 문구의 재료(없으면 시각만 보여준다)
  const [diffByFolder, setDiffByFolder] = useState<Map<string, SchemaDiff>>(new Map());
  const [dbError, setDbError] = useState<string | null>(null);
  // 우측 pane 에 스키마 개요를 띄우는 선택 — 파일 선택(selected)과 배타
  const [selectedSchema, setSelectedSchema] = useState<{
    conn: DbConnection;
    pref: DbSchemaPref;
  } | null>(null);
  const [pendingSchema, setPendingSchema] = useState<{
    conn: DbConnection;
    pref: DbSchemaPref;
  } | null>(null);
  const [dbModal, setDbModal] = useState<{ open: boolean; connection: DbConnection | null }>({
    open: false,
    connection: null,
  });
  const [dbDiffOpen, setDbDiffOpen] = useState(false);
  // 세션당 한 번만 자동 동기화를 시도한 스키마 폴더 — 실패한 연결이 이벤트마다 다시 두드리지 않게
  const attempted = useRef<Set<string>>(new Set());

  const dirty = editing && draft !== body;
  // WORKSPACE_EVENT 리스너가 최신 dirty 를 보게 하는 ref
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;

  /** 연결 목록 + 각 스키마의 스냅샷을 다시 읽는다 (탭 진입·워크스페이스 변경·설정에서의 변경) */
  const loadConnections = useCallback(async () => {
    try {
      const list = await listConnections();
      setConnections(list);
      const idx = indexConnections(list);
      const snaps = new Map<string, SchemaSnapshot | null>();
      for (const [folder, { conn, pref }] of idx.schemaByFolder) {
        snaps.set(folder, await readSnapshot(conn, pref.name).catch(() => null));
      }
      setSnapByFolder(snaps);
      return { list, snaps };
    } catch (e) {
      setDbError(errMsg(e));
      return null;
    }
  }, []);

  /** 행(last_sync_at·last_error)만 다시 읽는다 — 스냅샷은 동기화가 이미 상태에 넣었다 */
  const refreshConnectionRows = useCallback(async () => {
    try {
      setConnections(await listConnections());
    } catch {
      /* 다음 로드에서 잡힌다 */
    }
  }, []);

  const fullErdPath = (conn: DbConnection, pref: DbSchemaPref) =>
    `${schemaFolder(conn, pref.name)}/${t("diagrams.db.fullErdFile")}.mmd`;

  /** 전체 ERD 파일이 없으면 만든다. 있으면 건드리지 않는다 — 갱신은 배너 → 초안 → ⌘S 로만 */
  async function ensureFullErd(
    conn: DbConnection,
    pref: DbSchemaPref,
    snapshot: SchemaSnapshot,
  ): Promise<string> {
    const path = fullErdPath(conn, pref);
    if (!(await diagramFileExists(path))) {
      const audit = prefAudit(pref);
      const { mermaid } = generateErd(snapshot, {
        // 라벨 언어는 AI 변환(DDL → ERD)과 같은 설정을 따른다 — UI 가 영어라도 ERD 는 한국어로 받을 수 있게
        lang: aiOutputLang(),
        audit,
        header: formatDbHeader(conn.name, pref.name, new Date(), snapshot.fingerprint, {
          audit,
          gen: ERD_GEN_VERSION,
        }),
      });
      await writeDiagramFile(path, mermaid);
      await reload();
      expandTo(schemaFolder(conn, pref.name));
    }
    return path;
  }

  /** 스키마 하나를 DB 에서 다시 읽어 스냅샷을 갱신한다. generate 면 전체 ERD 가 없을 때 만든다.
   *  같은 폴더가 이미 도는 중이면 겹쳐 돌리지 않는다. */
  async function syncFolder(
    conn: DbConnection,
    pref: DbSchemaPref,
    opts?: { generate?: boolean },
  ): Promise<SchemaSnapshot | null> {
    const folder = schemaFolder(conn, pref.name);
    if (syncingRef.current.has(folder)) return snapByFolder.get(folder) ?? null;
    syncingRef.current.add(folder);
    setSyncing((prev) => new Set(prev).add(folder));
    setDbError(null);
    try {
      const r = await syncSchema(conn, pref.name);
      setSnapByFolder((m) => new Map(m).set(folder, r.snapshot));
      if (r.diff && !diffIsEmpty(r.diff)) {
        const d = r.diff;
        setDiffByFolder((m) => new Map(m).set(folder, d));
      }
      if (opts?.generate) await ensureFullErd(conn, pref, r.snapshot);
      return r.snapshot;
    } catch (e) {
      setDbError(t("diagrams.db.syncFail", { msg: errMsg(e) }));
      return null;
    } finally {
      syncingRef.current.delete(folder);
      setSyncing((prev) => {
        const n = new Set(prev);
        n.delete(folder);
        return n;
      });
      void refreshConnectionRows();
    }
  }

  /** 스냅샷이 없는 활성 스키마를 세션당 한 번 채운다 — 새 연결(설정에서 만든 것 포함)의 첫 동기화·첫 ERD */
  /** 한 연결에서 동시에 읽는 스키마 수 — 터널 너머 왕복이 병목이라 셋이면 체감이 다르다(Rust 풀은 4) */
  const SYNC_WORKERS = 3;

  async function autoSyncMissing(
    list: DbConnection[],
    snaps: Map<string, SchemaSnapshot | null>,
  ) {
    for (const conn of list) {
      const todo = enabledSchemas(conn).filter((pref) => {
        const folder = schemaFolder(conn, pref.name);
        return !snaps.get(folder) && !attempted.current.has(folder);
      });
      if (!todo.length) continue;
      // 이 연결의 이번 세션 시도는 지금 전부 — 실패해도 이벤트마다 다시 두드리지 않는다(실측: 키체인
      // 확인창이 스키마 수만큼 떴다). 사용자가 [동기화]로 다시 시도한다.
      const folders = todo.map((p) => schemaFolder(conn, p.name));
      for (const f of folders) attempted.current.add(f);
      setQueued((q) => new Set([...q, ...folders]));
      expandTo(conn.folder_path); // 진행이 보이게 연결 폴더를 펼친다

      let failed = false;
      const queue = [...todo];
      const worker = async () => {
        while (queue.length && !failed) {
          const pref = queue.shift()!;
          const folder = schemaFolder(conn, pref.name);
          setQueued((q) => {
            const n = new Set(q);
            n.delete(folder);
            return n;
          });
          const ok = await syncFolder(conn, pref, { generate: true });
          // 접속·키체인 실패는 연결 단위의 사고다 — 남은 스키마는 이번엔 건너뛴다
          if (!ok) failed = true;
        }
      };
      await Promise.all(Array.from({ length: SYNC_WORKERS }, worker));
      setQueued((q) => {
        const n = new Set(q);
        for (const f of folders) n.delete(f);
        return n;
      });
    }
  }
  const autoSyncRef = useRef(autoSyncMissing);
  autoSyncRef.current = autoSyncMissing;

  useEffect(() => {
    if (!active) return;
    void loadConnections().then((r) => r && autoSyncRef.current(r.list, r.snaps));
  }, [active, loadConnections]);

  useEffect(() => {
    const h = () =>
      void loadConnections().then((r) => r && autoSyncRef.current(r.list, r.snaps));
    window.addEventListener(DB_CONNECTIONS_EVENT, h);
    return () => window.removeEventListener(DB_CONNECTIONS_EVENT, h);
  }, [loadConnections]);

  /** 감사 테이블 포함 토글 — 스키마 설정에 저장하고, 화면의 선택(pref)도 새 값으로 바꾼다.
   *  열린 ERD 가 있으면 헤더의 표식과 달라져 변경 배너가 뜬다(다시 생성은 사용자가 고른다). */
  async function toggleAudit(conn: DbConnection, pref: DbSchemaPref, audit: boolean) {
    try {
      await setSchemaAudit(conn, pref.name, audit);
      const list = await listConnections();
      setConnections(list);
      const c2 = list.find((c) => c.id === conn.id);
      const p2 = c2?.schemas.find((p) => p.name === pref.name);
      if (c2 && p2) setSelectedSchema({ conn: c2, pref: p2 });
    } catch (e) {
      setDbError(errMsg(e));
    }
  }

  /** 스키마 폴더 클릭 → 우측에 스키마 개요. 스냅샷이 오래됐으면(10분) 한 번 다시 읽는다 */
  function doOpenSchema(conn: DbConnection, pref: DbSchemaPref) {
    openSeq.current++; // 열리던 파일의 늦은 응답이 화면을 덮지 않게
    setSelected(null);
    setEditing(false);
    setReadError(null);
    setOpError(null);
    setLoadingBody(false);
    const folder = schemaFolder(conn, pref.name);
    setSelectedSchema({ conn, pref });
    setActiveDir(folder);
    expandTo(folder);
    const snap = snapByFolder.get(folder) ?? null;
    if (isStale(snap)) void syncFolder(conn, pref, { generate: !snap });
  }

  function openSchema(conn: DbConnection, pref: DbSchemaPref) {
    if (dirty) {
      setPendingSchema({ conn, pref });
      return;
    }
    doOpenSchema(conn, pref);
  }

  /** 노드 정보 카드의 "라인 N" — 에디터로 점프 (읽기 모드였다면 편집 모드로 진입) */
  function jumpToLine(line: number) {
    const goto = () => {
      const ta = editorRef.current;
      if (!ta) return;
      const lines = ta.value.split("\n");
      let pos = 0;
      for (let i = 0; i < Math.min(line - 1, lines.length); i++)
        pos += lines[i].length + 1;
      ta.focus();
      ta.setSelectionRange(pos, pos + (lines[line - 1]?.length ?? 0));
      const lh = parseFloat(getComputedStyle(ta).lineHeight) || 20;
      ta.scrollTop = Math.max(0, (line - 3) * lh);
    };
    if (editing) {
      goto();
    } else {
      setDraft(body);
      setPreviewChart(body);
      setEditing(true);
      // 편집 UI 마운트 후 포커스 이동
      requestAnimationFrame(() => requestAnimationFrame(goto));
    }
  }

  const reload = useCallback(async () => {
    try {
      setTree(await listDiagramTree());
      setTreeError(null);
    } catch (e) {
      setTreeError(errMsg(e));
    }
  }, []);

  // 탭에 들어올 때마다 트리 갱신 (Finder/git 외부 변경 반영)
  useEffect(() => {
    if (active) void reload();
  }, [active, reload]);

  // 워크스페이스 루트("폴더 열기") 변경 → 선택/트리 상태 초기화 후 새 루트 로드.
  // 같은 트리의 다른 파일을 여는 것도 확인을 받으므로, 더 많이 잃는 이 전환도 초안을 지킨다.
  const applyRootChange = useCallback(() => {
    setSelected(null);
    setSelectedSchema(null);
    setEditing(false);
    setExpanded(new Set());
    setMountedDirs(new Set());
    setActiveDir("");
    setOpError(null);
    void reload();
    void loadConnections();
  }, [reload, loadConnections]);

  useEffect(() => {
    const h = (e: Event) => {
      if ((e as CustomEvent).detail !== "diagrams") return;
      if (dirtyRef.current) setPendingRoot(true);
      else applyRootChange();
    };
    window.addEventListener(WORKSPACE_EVENT, h);
    return () => window.removeEventListener(WORKSPACE_EVENT, h);
  }, [applyRootChange]);

  // 편집 중 타이핑 → 350ms 디바운스로 프리뷰 갱신 (mermaid 재렌더 비용 절약)
  useEffect(() => {
    if (!editing) return;
    const t = setTimeout(() => setPreviewChart(draft), 350);
    return () => clearTimeout(t);
  }, [draft, editing]);

  /** dir 와 그 조상 폴더를 모두 펼침 */
  function expandTo(dir: string) {
    if (!dir) return;
    const paths = ancestorPaths(dir);
    setExpanded((prev) => new Set([...prev, ...paths]));
    setMountedDirs((prev) => new Set([...prev, ...paths]));
  }

  /** 폴더 이름 변경 시 selected/activeDir/expanded 경로 프리픽스 재매핑 */
  function remapPrefix(oldP: string, newP: string) {
    const map = (s: string) => remapPath(s, oldP, newP);
    setExpanded((prev) => remapPaths(prev, oldP, newP));
    setMountedDirs((prev) => remapPaths(prev, oldP, newP));
    setSelected((prev) => (prev ? map(prev) : prev));
    setActiveDir((prev) => map(prev));
  }

  async function doOpen(path: string, opts?: { edit?: boolean }) {
    const seq = ++openSeq.current;
    setSelected(path);
    setSelectedSchema(null);
    setActiveDir(parentOf(path));
    setEditing(false);
    setOpError(null);
    setReadError(null);
    setLoadingBody(true);
    try {
      const b = await readDiagramFile(path);
      const m = await diagramMtime(path);
      if (seq !== openSeq.current) return; // 그 사이 다른 파일을 열었다
      setBody(b);
      if (opts?.edit) {
        setDraft(b);
        setPreviewChart(b);
        setEditing(true);
      }
      setMtime(m);
      // DB 연동 파일이면 스냅샷이 오래됐을 때(10분) 한 번 다시 읽는다 — 배너가 최신 구조를 말하게
      if (parseDbHeader(b)) {
        const hit = connIndex.schemaByFolder.get(parentOf(path));
        if (hit && isStale(snapByFolder.get(schemaFolder(hit.conn, hit.pref.name)) ?? null))
          void syncFolder(hit.conn, hit.pref);
      }
    } catch (e) {
      if (seq !== openSeq.current) return;
      // 못 읽은 파일을 빈 본문으로 열어두면 그 빈 초안이 ⌘S 로 원본을 덮는다
      setBody("");
      setReadError(errMsg(e));
      setMtime(null);
    } finally {
      if (seq === openSeq.current) setLoadingBody(false);
    }
  }

  function openFile(path: string) {
    if (path === selected && !editing) return;
    if (dirty) {
      setPendingOpen(path);
      return;
    }
    void doOpen(path);
  }

  // 빠른 검색의 다이어그램 결과 → 이 파일을 연다 (섹션 전환은 App, NotesView 의 OPEN_NOTE 짝)
  const openFileRef = useRef(openFile);
  openFileRef.current = openFile;
  useEffect(() => {
    const h = (e: Event) => {
      const path = (e as CustomEvent<{ path: string }>).detail?.path;
      if (typeof path === "string") openFileRef.current(path);
    };
    window.addEventListener(OPEN_DIAGRAM, h);
    return () => window.removeEventListener(OPEN_DIAGRAM, h);
  }, []);

  function toggleDir(n: DiagramNode) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(n.path)) next.delete(n.path);
      else next.add(n.path);
      return next;
    });
    // 마운트는 펼침보다 먼저 일어나야 0fr→1fr 트랜지션이 첫 프레임부터 돈다
    setMountedDirs((prev) => (prev.has(n.path) ? prev : new Set(prev).add(n.path)));
    setActiveDir(n.path);
  }

  function startEdit() {
    setDraft(body);
    setPreviewChart(body);
    setEditing(true);
  }

  /** AI 변환 결과는 초안으로만 반영 — 파일은 사용자가 ⌘S 로 저장할 때까지 그대로 */
  function applyAiResult(mermaid: string) {
    setDraft(mermaid);
    setPreviewChart(mermaid);
    setEditing(true);
  }

  async function save(opts?: { force?: boolean }) {
    if (!editing || !selected || busy || readError) return;
    setBusy(true);
    try {
      // 열 때 잡아둔 mtime 보다 디스크가 새로우면 외부(Finder/vim/git)가 먼저 고친 것 —
      // 조용히 덮지 않고 사용자에게 선택을 넘긴다
      if (!opts?.force) {
        const cur = await diagramMtime(selected);
        if (cur !== null && mtime !== null && cur > mtime) {
          setConflict({ path: selected, diskMtime: cur });
          return;
        }
      }
      await writeDiagramFile(selected, draft);
      setBody(draft);
      setEditing(false);
      setMtime((await diagramMtime(selected)) ?? Date.now());
      setOpError(null);
    } catch (e) {
      setOpError(errMsg(e));
    } finally {
      setBusy(false);
    }
  }

  /** 툴바 새로고침 — 트리만 갱신하면 열린 버퍼가 옛 내용으로 남는다.
   *  편집 중이면 초안은 그대로 두고 디스크가 더 새로운지만 확인해 외부 변경 모달로 넘긴다. */
  async function refreshAll() {
    await reload();
    if (!selected) return;
    if (editing) {
      const cur = await diagramMtime(selected);
      if (cur !== null && mtime !== null && cur > mtime)
        setConflict({ path: selected, diskMtime: cur });
      return;
    }
    await doOpen(selected);
  }

  // ⌘S 저장 (다이어그램 섹션이 보일 때 + 편집 중일 때만)
  const saveRef = useRef(save);
  saveRef.current = save;
  const activeRef = useRef(active);
  activeRef.current = active;
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (!activeRef.current) return;
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void saveRef.current();
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  // 창 포커스 복귀 시 트리 갱신
  const reloadRef = useRef(reload);
  reloadRef.current = reload;
  useEffect(() => {
    const onFocus = () => {
      if (activeRef.current) void reloadRef.current();
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  function openNameModal(kind: "new-file" | "new-folder", dir?: string) {
    setModalError(null);
    setNameModal({ kind, name: "", dir: dir ?? activeDir });
  }

  function openRenameModal(target: DiagramNode) {
    setModalError(null);
    setNameModal({ kind: "rename", name: target.name, target });
  }

  async function submitNameModal() {
    const m = nameModal;
    if (!m || busy) return;
    const name = m.name.trim();
    const reason =
      m.kind === "rename" ? invalidNameReason(name) : invalidPathReason(name);
    if (reason) {
      setModalError(reason);
      return;
    }
    setBusy(true);
    try {
      if (m.kind === "rename") {
        const t = m.target;
        const newRel = await renameEntry(t.path, name, t.isDir);
        if (t.isDir) {
          remapPrefix(t.path, newRel);
          // 연결 폴더거나 그 조상이면 프로필의 folder_path 도 따라간다
          if (await remapConnectionFolders(t.path, newRel)) await loadConnections();
        } else if (selected === t.path) setSelected(newRel);
        setNameModal(null);
        await reload();
      } else if (m.kind === "new-file") {
        const rel = await createDiagram(m.dir, name);
        setNameModal(null);
        await reload();
        expandTo(parentOf(rel));
        if (!dirty) await doOpen(rel, { edit: true });
      } else {
        const rel = await createFolder(m.dir, name);
        setNameModal(null);
        await reload();
        expandTo(rel);
        setActiveDir(rel);
      }
    } catch (e) {
      setModalError(errMsg(e));
    } finally {
      setBusy(false);
    }
  }

  async function doDelete() {
    const t = confirmDelete;
    if (!t || busy) return;
    setBusy(true);
    try {
      await deleteEntry(t.path);
      // 연결 폴더(또는 그 조상)를 지우면 프로필도 함께 — 폴더가 곧 연결이다. 행만 남으면 같은 이름으로
      // 다시 만들 때 folder_path UNIQUE 에 걸리고(실측), 설정에는 "폴더 없음" 유령이 남는다.
      const orphaned = connections.filter(
        (c) => c.folder_path === t.path || c.folder_path.startsWith(`${t.path}/`),
      );
      for (const c of orphaned) await deleteConnection(c);
      if (orphaned.length) {
        notifyConnectionsChanged();
        await loadConnections();
      }
      if (
        selectedSchema &&
        schemaFolder(selectedSchema.conn, selectedSchema.pref.name).startsWith(t.path)
      ) {
        setSelectedSchema(null);
      }
      if (
        selected &&
        (selected === t.path || selected.startsWith(`${t.path}/`))
      ) {
        setSelected(null);
        setEditing(false);
      }
      if (activeDir === t.path || activeDir.startsWith(`${t.path}/`))
        setActiveDir(parentOf(t.path));
      setConfirmDelete(null);
      await reload();
    } catch (e) {
      setOpError(errMsg(e));
      setConfirmDelete(null);
    } finally {
      setBusy(false);
    }
  }

  // 파일 트리 드래그 이동 — 다른 폴더/루트로 놓으면 파일·폴더를 옮긴다
  const dnd = useTreeDnd({
    move: moveEntry,
    onMoved: (fromPath, newPath, isDir) => {
      if (isDir) {
        remapPrefix(fromPath, newPath);
        void remapConnectionFolders(fromPath, newPath).then((hit) => {
          if (hit) void loadConnections();
        });
      } else if (selected === fromPath) setSelected(newPath);
      expandTo(parentOf(newPath));
      setOpError(null);
      void reload();
    },
    onError: setOpError,
  });

  const pane = usePaneResize({ storageKey: "amber.diagrams.list-width", active });

  // 컴포넌트가 아닌 렌더 함수 — 렌더마다 트리 DOM 이 리마운트되지 않게
  function renderRows(nodes: DiagramNode[], depth: number) {
    return (
      <>
        {nodes.map((n) => {
          const isOpen = n.isDir && expanded.has(n.path);
          // DB 연동: 이 폴더가 연결 폴더인가 / 스키마 폴더인가
          const connHit = n.isDir ? connIndex.byFolder.get(n.path) : undefined;
          const schemaHit = n.isDir ? connIndex.schemaByFolder.get(n.path) : undefined;
          const schemaSelected =
            !!schemaHit &&
            !!selectedSchema &&
            schemaFolder(selectedSchema.conn, selectedSchema.pref.name) === n.path;
          const isSyncing = !!schemaHit && syncing.has(n.path);
          const isQueued = !!schemaHit && !isSyncing && queued.has(n.path);
          const schemaSnap = schemaHit ? snapByFolder.get(n.path) : undefined;
          // 연결 행: 아래 스키마 중 돌고 있거나 기다리는 것이 있으면 n/N 진행
          const connProgress = (() => {
            if (!connHit) return null;
            const all = enabledSchemas(connHit).map((p) => schemaFolder(connHit, p.name));
            const active = all.filter((f) => syncing.has(f) || queued.has(f)).length;
            return active ? { done: all.length - active, total: all.length } : null;
          })();
          return (
            <div key={n.path} className="tree-branch">
              <div
                className={`tree-row ${n.isDir ? "dir" : ""} ${
                  (!n.isDir && selected === n.path) || schemaSelected ? "selected" : ""
                } ${n.isDir && activeDir === n.path && !schemaSelected ? "active" : ""} ${
                  isSyncing ? "db-syncing" : ""
                } ${dnd.rowClass(n)}`}
                style={{ paddingLeft: 8 + depth * 14 }}
                {...dnd.rowProps(n)}
                onClick={() => {
                  if (dnd.consumeClick()) return;
                  if (schemaHit) {
                    // 스키마 폴더는 클릭 = 개요(우측), 펼침은 셰브론이 맡는다(처음 클릭엔 함께 펼친다)
                    openSchema(schemaHit.conn, schemaHit.pref);
                    if (!expanded.has(n.path)) toggleDir(n);
                    return;
                  }
                  n.isDir ? toggleDir(n) : openFile(n.path);
                }}
              >
                {n.isDir ? (
                  <span
                    className={`caret ${isOpen ? "open" : ""}`}
                    onClick={(e) => {
                      if (!schemaHit) return;
                      e.stopPropagation();
                      toggleDir(n);
                    }}
                  >
                    <Icon name="chevron-right" size={13} />
                  </span>
                ) : (
                  <span className="caret leaf" />
                )}
                <Icon
                  name={
                    connHit
                      ? "database"
                      : schemaHit
                        ? isSyncing
                          ? "refresh"
                          : "table"
                        : n.isDir
                          ? isOpen || dnd.isDropTarget(n)
                            ? "folder-open"
                            : "folder"
                          : "workflow"
                  }
                  size={14}
                  className="tree-ico"
                />
                <span className="label" title={n.name}>
                  {n.name}
                </span>
                {schemaHit?.pref.label && <span className="tree-sub">{schemaHit.pref.label}</span>}
                {connHit && (
                  <span className="tree-count">
                    {connProgress ? (
                      // 동기화가 도는 동안은 상태 점 대신 진행 — 사용자가 기다리는 이유가 여기 보인다
                      t("diagrams.db.tree.progress", connProgress)
                    ) : (
                      <>
                        {/* 트리 행에는 상태 글자가 들어갈 자리가 없어 점이 유일한 신호다 —
                            색만으로 끝내지 않도록 툴팁이 같은 상태를 단어로 준다(DESIGN §2 결과 색) */}
                        <Tooltip label={connStatusLabel(connStatus(connHit))}>
                          <span className={`db-dot ${connStatusDot(connStatus(connHit))}`} />
                        </Tooltip>
                        {envLabel(connHit.env)}
                      </>
                    )}
                  </span>
                )}
                {schemaHit && isSyncing && (
                  <span className="tree-count">{t("diagrams.db.tree.syncing")}</span>
                )}
                {schemaHit && isQueued && (
                  <span className="tree-count">{t("diagrams.db.tree.queued")}</span>
                )}
                {schemaHit && !isSyncing && !isQueued && schemaSnap && (
                  <span className="tree-count">{schemaSnap.tables.length}</span>
                )}
                <span
                  className="row-actions"
                  onClick={(e) => e.stopPropagation()}
                >
                  {n.isDir && (
                    <>
                      <Tooltip label={t("diagrams.tree.newFileHere")}>
                        <button
                          aria-label={t("diagrams.tree.newFileHere")}
                          className="icon-btn sm"
                          onClick={() => openNameModal("new-file", n.path)}
                        >
                          <Icon name="file-plus" size={13} />
                        </button>
                      </Tooltip>
                      <Tooltip label={t("diagrams.tree.newFolderHere")}>
                        <button
                          aria-label={t("diagrams.tree.newFolderHere")}
                          className="icon-btn sm"
                          onClick={() => openNameModal("new-folder", n.path)}
                        >
                          <Icon name="folder-plus" size={13} />
                        </button>
                      </Tooltip>
                    </>
                  )}
                  {schemaHit && (
                    <Tooltip label={t("diagrams.db.tree.syncHere")}>
                      <button
                        aria-label={t("diagrams.db.tree.syncHere")}
                        className="icon-btn sm"
                        disabled={isSyncing}
                        onClick={() => void syncFolder(schemaHit.conn, schemaHit.pref)}
                      >
                        <Icon name="refresh" size={13} />
                      </button>
                    </Tooltip>
                  )}
                  <Tooltip label={t("diagrams.rename")}>
                    <button
                      aria-label={t("diagrams.rename")}
                      className="icon-btn sm"
                      onClick={() => openRenameModal(n)}
                    >
                      <Icon name="pencil" size={13} />
                    </button>
                  </Tooltip>
                  <Tooltip label={t("common.delete")}>
                    <button
                      aria-label={t("common.delete")}
                      className="icon-btn sm danger"
                      onClick={() =>
                        setConfirmDelete({
                          name: n.name,
                          path: n.path,
                          isDir: n.isDir,
                        })
                      }
                    >
                      <Icon name="trash" size={13} />
                    </button>
                  </Tooltip>
                </span>
              </div>
              {/* 한 번이라도 펼친 폴더만 자식을 마운트한다 — 접힌 채로도 전부 렌더하면
                  1,000개 규모에서 수천 개 엘리먼트가 살아 있고 창 포커스마다 전부 재조정된다.
                  0fr→1fr 트랜지션을 살리려면 마운트와 .open 을 같은 프레임에 주면 안 되므로
                  mountedDirs 에 넣는 시점(펼침 클릭)과 isOpen 이 자연히 한 프레임 어긋난다. */}
              {n.isDir && n.children && n.children.length > 0 && mountedDirs.has(n.path) && (
                <div className={`tree-children ${isOpen ? "open" : ""}`}>
                  <div className="tree-children-inner">
                    {renderRows(n.children, depth + 1)}
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </>
    );
  }

  const dirOptions = flattenDirs(tree ?? []).map((d) => ({
    value: encodeDir(d),
    label: encodeDir(d),
  }));

  /** 열린 파일이 DB 연동 파일이면(헤더 + 스키마 폴더 안) 그 맥락 — 배너·동기화 버튼·출처 메타의 재료 */
  const dbFile = useMemo(() => {
    if (!selected) return null;
    const hdr = parseDbHeader(body);
    if (!hdr) return null;
    const hit = connIndex.schemaByFolder.get(parentOf(selected));
    if (!hit) return null;
    const folder = schemaFolder(hit.conn, hit.pref.name);
    const snap = snapByFolder.get(folder) ?? null;
    const schemaChanged = !!snap && hdr.fingerprint !== null && hdr.fingerprint !== snap.fingerprint;
    // 감사 테이블 포함 설정이 파일을 만들 때와 다르면 구조가 같아도 파일은 낡았다
    const optionChanged = hdr.audit !== prefAudit(hit.pref);
    // 생성 규칙이 새로워졌으면(참조 추론·설명 규칙 등) 같은 스냅샷에서도 다른 파일이 나온다
    const rulesChanged = hdr.gen !== ERD_GEN_VERSION;
    return {
      hdr,
      conn: hit.conn,
      pref: hit.pref,
      snap,
      stale: schemaChanged || (!!snap && (optionChanged || rulesChanged)),
      schemaChanged,
      optionChanged,
      diff: diffByFolder.get(folder) ?? null,
      syncing: syncing.has(folder),
    };
  }, [selected, body, connIndex, snapByFolder, diffByFolder, syncing]);

  /** 최신 스냅샷으로 다시 만든 소스 — [변경 보기]의 오른쪽, [다시 생성]의 초안 */
  function regeneratedSource(): string {
    if (!dbFile?.snap) return "";
    const audit = prefAudit(dbFile.pref);
    return generateErd(dbFile.snap, {
      lang: aiOutputLang(),
      audit,
      header: formatDbHeader(dbFile.conn.name, dbFile.pref.name, new Date(), dbFile.snap.fingerprint, {
        audit,
        gen: ERD_GEN_VERSION,
      }),
    }).mermaid;
  }

  /** [다시 생성] — 새 소스는 초안으로만. 파일은 ⌘S 전까지 그대로(AI 변환과 같은 길) */
  function regenerateFromSnapshot() {
    const src = regeneratedSource();
    if (src) applyAiResult(src);
  }

  function treeHasFile(nodes: DiagramNode[], path: string): boolean {
    for (const n of nodes) {
      if (n.path === path) return true;
      if (n.isDir && n.children && path.startsWith(`${n.path}/`) && treeHasFile(n.children, path))
        return true;
    }
    return false;
  }

  const fileName = selected
    ? selected
        .slice(selected.lastIndexOf("/") + 1)
        .replace(/\.(mmd|mermaid)$/i, "")
    : "";
  const crumbDirs = selected
    ? parentOf(selected).split("/").filter(Boolean)
    : [];

  return (
    <div className="body" {...pane.bodyProps}>
      <aside className="list">
        <div className="notes-tree-head">
          <RootPicker section="diagrams" />
          <span className="spacer" />
          <Tooltip label={t("diagrams.db.tooltip.add")}>
            <button
              className="icon-btn sm"
              aria-label={t("diagrams.db.addConnection")}
              onClick={() => setDbModal({ open: true, connection: null })}
            >
              <Icon name="database" size={15} />
            </button>
          </Tooltip>
          <Tooltip label={t("diagrams.tooltip.newFileAt", { dir: encodeDir(activeDir) })}>
            <button
              className="icon-btn sm"
              aria-label={t("diagrams.newFile")}
              onClick={() => openNameModal("new-file")}
            >
              <Icon name="file-plus" size={15} />
            </button>
          </Tooltip>
          <Tooltip label={t("diagrams.tooltip.newFolderAt", { dir: encodeDir(activeDir) })}>
            <button
              className="icon-btn sm"
              aria-label={t("diagrams.newFolder")}
              onClick={() => openNameModal("new-folder")}
            >
              <Icon name="folder-plus" size={15} />
            </button>
          </Tooltip>
          <Tooltip label={t("diagrams.tooltip.refresh")}>
            <button
              className="icon-btn sm"
              aria-label={t("diagrams.refresh")}
              onClick={() => void refreshAll()}
            >
              <Icon name="refresh" size={14} />
            </button>
          </Tooltip>
        </div>

        {treeError && (
          <div className="error-note" style={{ margin: 12 }}>
            {treeError}
          </div>
        )}
        {tree === null && !treeError && <Spinner />}
        {tree && tree.length === 0 && (
          <div className="tree-empty">
            <p>
              {t("diagrams.empty.tree1")}
              <br />
              {t("diagrams.empty.tree2")}
            </p>
            <button
              className="btn btn-primary btn-sm"
              onClick={() => openNameModal("new-file")}
            >
              <Icon name="file-plus" size={14} />
              {t("diagrams.empty.create")}
            </button>
          </div>
        )}
        {tree && tree.length > 0 && (
          <div
            className={`tree ${dnd.treeClass}`}
            onMouseDown={(e) => {
              if (e.detail > 1) e.preventDefault();
            }}
          >
            {renderRows(tree, 0)}
          </div>
        )}
      </aside>

      {dnd.drag && (
        <TreeDragOverlay drag={dnd.drag} leafIcon="workflow" overlayRef={dnd.overlayRef} />
      )}

      <div {...pane.resizerProps} />

      <section className="detail dgm">
        {selectedSchema ? (
          <SchemaOverview
            conn={selectedSchema.conn}
            pref={selectedSchema.pref}
            snapshot={snapByFolder.get(schemaFolder(selectedSchema.conn, selectedSchema.pref.name)) ?? null}
            syncing={syncing.has(schemaFolder(selectedSchema.conn, selectedSchema.pref.name))}
            error={dbError}
            hasFullErd={treeHasFile(tree ?? [], fullErdPath(selectedSchema.conn, selectedSchema.pref))}
            onSync={() => void syncFolder(selectedSchema.conn, selectedSchema.pref)}
            onGenerate={() => {
              const snap = snapByFolder.get(schemaFolder(selectedSchema.conn, selectedSchema.pref.name));
              if (!snap) return;
              void ensureFullErd(selectedSchema.conn, selectedSchema.pref, snap).then((path) =>
                openFile(path),
              );
            }}
            onOpenFull={() => openFile(fullErdPath(selectedSchema.conn, selectedSchema.pref))}
            onToggleAudit={(audit) => void toggleAudit(selectedSchema.conn, selectedSchema.pref, audit)}
          />
        ) : selected ? (
          // 읽기/편집 모두 화면 높이 고정('editing' 레이아웃) — 캔버스가 남은 공간을 채우고 팬/줌
          <div className="notes-detail editing">
            {/* 컴팩트 헤더 한 줄: 크럼+제목(좌) / 액션(우) — 캔버스에 최대 공간 */}
            <div className="dgm-head">
              <div className="dgm-head-info">
                <div className="note-crumb">
                  <Icon name="folder" size={12} />
                  {[t("diagrams.title"), ...crumbDirs].join(" › ")}
                </div>
                <h1 className="dgm-title">{fileName}</h1>
              </div>
              <span className="spacer" />
              {dbFile && (
                <button
                  className="btn btn-sm"
                  onClick={() => void syncFolder(dbFile.conn, dbFile.pref)}
                  disabled={busy || dbFile.syncing}
                >
                  <Icon name="refresh" size={14} />
                  {dbFile.syncing ? t("diagrams.db.syncingShort") : t("diagrams.db.sync")}
                </button>
              )}
              <Tooltip
                label={
                  config?.provider
                    ? t("diagrams.ai.tooltip")
                    : t("diagrams.ai.tooltipNoProvider")
                }
              >
                <button
                  className="btn btn-sm"
                  onClick={() => setAiOpen(true)}
                  disabled={
                    busy || loadingBody || !!readError || !config?.provider
                  }
                >
                  <Icon name="sparkles" size={14} />
                  DDL → ERD
                </button>
              </Tooltip>
              {editing ? (
                <>
                  <button
                    className="btn btn-primary btn-sm"
                    onClick={() => void save()}
                    disabled={busy}
                  >
                    {busy ? t("diagrams.saving") : `${t("common.save")} (⌘S)`}
                  </button>
                  <button
                    className="btn btn-sm"
                    onClick={() => setEditing(false)}
                    disabled={busy}
                  >
                    {t("common.cancel")}
                  </button>
                </>
              ) : (
                <button
                  className="btn btn-sm"
                  onClick={startEdit}
                  disabled={busy || loadingBody || !!readError}
                >
                  <Icon name="pencil" size={14} />
                  {t("diagrams.edit")}
                </button>
              )}
              <button
                className="btn btn-sm btn-danger-ghost"
                onClick={() =>
                  setConfirmDelete({
                    name: fileName,
                    path: selected,
                    isDir: false,
                  })
                }
                disabled={busy}
              >
                <Icon name="trash" size={14} />
                {t("common.delete")}
              </button>
            </div>

            {opError && <div className="error-note">{opError}</div>}
            {dbFile && dbError && <div className="error-note">{dbError}</div>}
            {dbFile?.stale && dbFile.snap && !editing && (
              <div className="db-banner">
                <Icon name="database" size={14} />
                <span>
                  <b>
                    {dbFile.schemaChanged
                      ? t("diagrams.db.banner.changed")
                      : dbFile.optionChanged
                        ? t("diagrams.db.banner.optionChanged")
                        : t("diagrams.db.banner.rulesChanged")}
                  </b>
                  {" · "}
                  {dbFile.diff && dbFile.schemaChanged
                    ? t("diagrams.db.banner.detail", {
                        ta: dbFile.diff.tablesAdded.length,
                        tr: dbFile.diff.tablesRemoved.length,
                        ca: dbFile.diff.columnsAdded.length,
                        cr: dbFile.diff.columnsRemoved.length,
                        cc: dbFile.diff.columnsChanged.length + dbFile.diff.constraintTables.length,
                      })
                    : t("diagrams.db.banner.stale", {
                        time: dbFile.hdr.generatedAt,
                        ago: timeAgo(dbFile.snap.synced_at),
                      })}
                </span>
                <span className="spacer" />
                <button className="btn btn-sm" onClick={() => setDbDiffOpen(true)}>
                  {t("diagrams.db.banner.viewDiff")}
                </button>
                <button className="btn btn-sm btn-danger-ghost" onClick={regenerateFromSnapshot}>
                  {t("diagrams.db.regenerate")}
                </button>
              </div>
            )}
            {readError && (
              <div className="error-note">
                {t("diagrams.readError", { msg: readError })}
              </div>
            )}

            {loadingBody ? (
              <Spinner />
            ) : readError ? null : editing ? (
              // 좌 소스 / 우 라이브 렌더 캔버스 (스튜디오와 동일한 팬/줌)
              <div className="note-edit-split">
                <textarea
                  ref={editorRef}
                  className="textarea note-textarea"
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  spellCheck={false}
                />
                <DiagramCanvas chart={previewChart} onJumpToLine={jumpToLine} />
              </div>
            ) : (
              <DiagramCanvas chart={body} onJumpToLine={jumpToLine} />
            )}

            <div className="detail-meta">
              {rootDisplayName("diagrams")}/{selected}
              {mtime !== null && (
                <> · {t("diagrams.meta.modified", { ago: timeAgo(mtime) })}</>
              )}
              {dbFile && <> · {t("diagrams.db.meta.generated", { time: dbFile.hdr.generatedAt })}</>}
            </div>
          </div>
        ) : (
          <div className="notes-empty">
            <div className="notes-empty-icon">
              <Icon name="workflow" size={30} />
            </div>
            <h2 className="notes-empty-title">{t("diagrams.title")}</h2>
            <p className="notes-empty-sub">
              {t("diagrams.empty.sub1")}
              <br />
              {t("diagrams.empty.sub2")}
            </p>
            <div className="notes-empty-actions">
              <button
                className="btn btn-primary btn-sm"
                onClick={() => openNameModal("new-file")}
              >
                <Icon name="file-plus" size={14} />
                {t("diagrams.newFile")}
              </button>
              <button
                className="btn btn-sm"
                onClick={() => openNameModal("new-folder")}
              >
                <Icon name="folder-plus" size={14} />
                {t("diagrams.newFolder")}
              </button>
            </div>
            <div className="notes-empty-tips">
              <span className="notes-empty-tip">
                <Icon name="pencil" size={12} /> {t("diagrams.empty.tipEdit")}
              </span>
              <span className="notes-empty-tip">
                <Icon name="expand" size={12} /> {t("diagrams.empty.tipZoom")}
              </span>
            </div>
          </div>
        )}
      </section>

      {/* 새 다이어그램 / 새 폴더 / 이름 변경 */}
      <Modal
        open={!!nameModal}
        title={
          nameModal?.kind === "new-file"
            ? t("diagrams.newFile")
            : nameModal?.kind === "new-folder"
              ? t("diagrams.newFolder")
              : t("diagrams.rename")
        }
        onClose={() => setNameModal(null)}
        footer={
          <>
            <button
              className="btn btn-sm"
              onClick={() => setNameModal(null)}
              disabled={busy}
            >
              {t("common.cancel")}
            </button>
            <button
              className="btn btn-primary btn-sm"
              onClick={() => void submitNameModal()}
              disabled={busy}
            >
              {busy
                ? t("diagrams.working")
                : nameModal?.kind === "rename"
                  ? t("diagrams.modal.renameConfirm")
                  : t("diagrams.modal.create")}
            </button>
          </>
        }
      >
        {nameModal && (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void submitNameModal();
            }}
          >
            {nameModal.kind !== "rename" && (
              <div className="field">
                <label>{t("diagrams.modal.location")}</label>
                <Select
                  value={encodeDir(nameModal.dir)}
                  options={dirOptions}
                  onChange={(v) =>
                    setNameModal((m) =>
                      m && m.kind !== "rename"
                        ? { ...m, dir: decodeDir(v) }
                        : m,
                    )
                  }
                  block
                />
              </div>
            )}
            <div className="field">
              <label>
                {nameModal.kind === "new-folder"
                  ? t("diagrams.modal.folderName")
                  : t("diagrams.modal.fileName")}
              </label>
              <input
                className="input"
                autoFocus
                value={nameModal.name}
                placeholder={
                  nameModal.kind === "new-folder"
                    ? t("diagrams.modal.folderPh")
                    : t("diagrams.modal.filePh")
                }
                onChange={(e) =>
                  setNameModal((m) => (m ? { ...m, name: e.target.value } : m))
                }
              />
              {nameModal.kind !== "rename" && (
                <div className="hint">{t("diagrams.modal.pathHint")}</div>
              )}
            </div>
            {modalError && <div className="error-note">{modalError}</div>}
          </form>
        )}
      </Modal>

      {/* 삭제 확인 */}
      <Modal
        open={!!confirmDelete}
        title={
          confirmDelete?.isDir
            ? t("diagrams.delete.folderTitle")
            : t("diagrams.delete.fileTitle")
        }
        narrow
        onClose={() => setConfirmDelete(null)}
        footer={
          <>
            <button
              className="btn btn-sm"
              onClick={() => setConfirmDelete(null)}
              disabled={busy}
            >
              {t("common.cancel")}
            </button>
            <button
              className="btn btn-sm btn-danger-ghost"
              onClick={() => void doDelete()}
              disabled={busy}
            >
              {busy ? t("diagrams.deleting") : t("common.delete")}
            </button>
          </>
        }
      >
        <p style={{ margin: 0 }}>
          <b>{confirmDelete?.name}</b>{" "}
          {confirmDelete?.isDir
            ? t("diagrams.delete.bodyDir")
            : t("diagrams.delete.bodyFile")}
          <br />
          {t("diagrams.delete.trashNote")}
          {/* 연결·스키마 폴더는 "DB 를 지우나?" 하는 걱정이 따라온다 — 무엇이 지워지는지 그 자리에서 말한다 */}
          {confirmDelete?.isDir && connIndex.byFolder.has(confirmDelete.path) && (
            <>
              <br />
              {t("diagrams.db.delete.connection")}
            </>
          )}
          {confirmDelete?.isDir && connIndex.schemaByFolder.has(confirmDelete.path) && (
            <>
              <br />
              {t("diagrams.db.delete.localOnly")}
            </>
          )}
        </p>
      </Modal>

      {/* 저장 안 된 변경 → 다른 파일로 이동 */}
      <UnsavedModal
        open={!!pendingOpen}
        onKeep={() => setPendingOpen(null)}
        onDiscard={() => {
          const p = pendingOpen;
          setPendingOpen(null);
          if (p) void doOpen(p);
        }}
      />

      {/* 저장 안 된 변경 → 작업 폴더 전환 */}
      <UnsavedModal
        open={pendingRoot}
        onKeep={() => setPendingRoot(false)}
        onDiscard={() => {
          setPendingRoot(false);
          applyRootChange();
        }}
      />

      {/* 외부에서 먼저 수정됨 → 덮어쓰기/다시 읽기 선택 */}
      <Modal
        open={!!conflict}
        title={t("diagrams.conflict.title")}
        narrow
        onClose={() => setConflict(null)}
        footer={
          <>
            <button className="btn btn-sm" onClick={() => setConflict(null)}>
              {t("common.unsaved.keep")}
            </button>
            <button
              className="btn btn-sm"
              onClick={() => {
                const p = conflict?.path;
                setConflict(null);
                if (p) void doOpen(p);
              }}
              disabled={busy}
            >
              {t("diagrams.conflict.reread")}
            </button>
            <button
              className="btn btn-sm btn-danger-ghost"
              onClick={() => {
                setConflict(null);
                void save({ force: true });
              }}
              disabled={busy}
            >
              {t("diagrams.conflict.overwrite")}
            </button>
          </>
        }
      >
        <p style={{ margin: 0 }}>
          {t("diagrams.conflict.body1")}
          {conflict && (
            <>
              {" · "}
              {t("diagrams.conflict.diskMtime", {
                ago: timeAgo(conflict.diskMtime),
              })}
            </>
          )}
          .
          <br />
          {t("diagrams.conflict.body2")}
        </p>
      </Modal>

      {/* 저장 안 된 변경 → 스키마 개요로 이동 */}
      <UnsavedModal
        open={!!pendingSchema}
        onKeep={() => setPendingSchema(null)}
        onDiscard={() => {
          const p = pendingSchema;
          setPendingSchema(null);
          if (p) doOpenSchema(p.conn, p.pref);
        }}
      />

      {/* DB 연결 추가/편집 — 저장되면 스냅샷이 없는 스키마를 채우며 첫 ERD 를 만든다 */}
      <DbConnectionModal
        open={dbModal.open}
        connection={dbModal.connection}
        onClose={() => setDbModal({ open: false, connection: null })}
        onSaved={(c) => {
          // 폴더는 모달이 이미 만들었다 — 먼저 트리에 보이고 펼친 뒤 동기화가 행마다 진행을 채운다
          void (async () => {
            await reload();
            expandTo(c.folder_path);
            setActiveDir(c.folder_path);
            const r = await loadConnections();
            if (r) await autoSyncRef.current(r.list, r.snaps);
          })();
        }}
      />

      {/* 변경 보기 — 지금 파일 vs 최신 스냅샷으로 만든 소스 */}
      <Modal
        open={dbDiffOpen}
        title={t("diagrams.db.banner.diffTitle")}
        onClose={() => setDbDiffOpen(false)}
        wide
        footer={
          <>
            <button className="btn btn-sm" onClick={() => setDbDiffOpen(false)}>
              {t("common.close")}
            </button>
            <button
              className="btn btn-sm btn-danger-ghost"
              onClick={() => {
                setDbDiffOpen(false);
                regenerateFromSnapshot();
              }}
            >
              {t("diagrams.db.regenerate")}
            </button>
          </>
        }
      >
        {dbDiffOpen && dbFile?.snap && <DiffView oldText={body} newText={regeneratedSource()} />}
        <div className="hint" style={{ marginTop: 10 }}>
          {t("diagrams.db.banner.applyHint")}
        </div>
      </Modal>

      {/* 스키마 DDL → ERD 변환 (결과는 에디터 초안으로만 반영) */}
      <DiagramAiModal
        open={aiOpen}
        currentSource={editing ? draft : body}
        config={config}
        onClose={() => setAiOpen(false)}
        onApplied={applyAiResult}
      />
    </div>
  );
}
