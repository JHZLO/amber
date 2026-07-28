// 다이어그램 섹션: 좌측 폴더 트리 + 우측 mermaid 렌더/편집 2-pane.
// 정본은 vault/diagrams/ 의 실제 디렉토리/.mmd 파일 (lib/diagrams.ts). 트리 UX 는 필기노트와 동일
// (같은 CSS 클래스 재사용). 읽기 = 렌더된 다이어그램(클릭 확대), 편집 = 소스 | 라이브 프리뷰.

import { useCallback, useEffect, useRef, useState } from "react";
import {
  createDiagram,
  createFolder,
  deleteEntry,
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
import { useTreeDnd } from "../lib/useTreeDnd";
import { DiagramCanvas } from "./DiagramCanvas";
import { DiagramAiModal } from "./DiagramAiModal";
import { Modal, Select, Spinner, Tooltip, TreeDragOverlay, timeAgo } from "../ui";
import { Icon } from "../icons";
import { t } from "../lib/i18n";
import { errText } from "../lib/errors";
import { RootPicker } from "./RootPicker";
import type { AppConfig } from "../lib/config";
import { rootDisplayName, WORKSPACE_EVENT } from "../lib/workspace";
import { OPEN_DIAGRAM } from "../lib/nav";

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
  const [aiOpen, setAiOpen] = useState(false);
  const editorRef = useRef<HTMLTextAreaElement>(null);

  const dirty = editing && draft !== body;

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

  // 워크스페이스 루트("폴더 열기") 변경 → 선택/트리 상태 초기화 후 새 루트 로드
  useEffect(() => {
    const h = (e: Event) => {
      if ((e as CustomEvent).detail !== "diagrams") return;
      setSelected(null);
      setEditing(false);
      setExpanded(new Set());
      setActiveDir("");
      setOpError(null);
      void reload();
    };
    window.addEventListener(WORKSPACE_EVENT, h);
    return () => window.removeEventListener(WORKSPACE_EVENT, h);
  }, [reload]);

  // 편집 중 타이핑 → 350ms 디바운스로 프리뷰 갱신 (mermaid 재렌더 비용 절약)
  useEffect(() => {
    if (!editing) return;
    const t = setTimeout(() => setPreviewChart(draft), 350);
    return () => clearTimeout(t);
  }, [draft, editing]);

  /** dir 와 그 조상 폴더를 모두 펼침 */
  function expandTo(dir: string) {
    if (!dir) return;
    const parts = dir.split("/");
    const paths: string[] = [];
    for (let i = 0; i < parts.length; i++)
      paths.push(parts.slice(0, i + 1).join("/"));
    setExpanded((prev) => new Set([...prev, ...paths]));
  }

  /** 폴더 이름 변경 시 selected/activeDir/expanded 경로 프리픽스 재매핑 */
  function remapPrefix(oldP: string, newP: string) {
    const map = (s: string) =>
      s === oldP
        ? newP
        : s.startsWith(`${oldP}/`)
          ? newP + s.slice(oldP.length)
          : s;
    setExpanded((prev) => new Set([...prev].map(map)));
    setSelected((prev) => (prev ? map(prev) : prev));
    setActiveDir((prev) => map(prev));
  }

  async function doOpen(path: string, opts?: { edit?: boolean }) {
    const seq = ++openSeq.current;
    setSelected(path);
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
        if (t.isDir) remapPrefix(t.path, newRel);
        else if (selected === t.path) setSelected(newRel);
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
      if (isDir) remapPrefix(fromPath, newPath);
      else if (selected === fromPath) setSelected(newPath);
      expandTo(parentOf(newPath));
      setOpError(null);
      void reload();
    },
    onError: setOpError,
  });

  // 컴포넌트가 아닌 렌더 함수 — 렌더마다 트리 DOM 이 리마운트되지 않게
  function renderRows(nodes: DiagramNode[], depth: number) {
    return (
      <>
        {nodes.map((n) => {
          const isOpen = n.isDir && expanded.has(n.path);
          return (
            <div key={n.path} className="tree-branch">
              <div
                className={`tree-row ${n.isDir ? "dir" : ""} ${
                  !n.isDir && selected === n.path ? "selected" : ""
                } ${n.isDir && activeDir === n.path ? "active" : ""} ${dnd.rowClass(n)}`}
                style={{ paddingLeft: 8 + depth * 14 }}
                {...dnd.rowProps(n)}
                onClick={() => {
                  if (dnd.consumeClick()) return;
                  n.isDir ? toggleDir(n) : openFile(n.path);
                }}
              >
                {n.isDir ? (
                  <span className={`caret ${isOpen ? "open" : ""}`}>
                    <Icon name="chevron-right" size={13} />
                  </span>
                ) : (
                  <span className="caret leaf" />
                )}
                <Icon
                  name={
                    n.isDir
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
                <span
                  className="row-actions"
                  onClick={(e) => e.stopPropagation()}
                >
                  {n.isDir && (
                    <>
                      <button
                        className="icon-btn sm"
                        title={t("diagrams.tree.newFileHere")}
                        onClick={() => openNameModal("new-file", n.path)}
                      >
                        <Icon name="file-plus" size={13} />
                      </button>
                      <button
                        className="icon-btn sm"
                        title={t("diagrams.tree.newFolderHere")}
                        onClick={() => openNameModal("new-folder", n.path)}
                      >
                        <Icon name="folder-plus" size={13} />
                      </button>
                    </>
                  )}
                  <button
                    className="icon-btn sm"
                    title={t("diagrams.rename")}
                    onClick={() => openRenameModal(n)}
                  >
                    <Icon name="pencil" size={13} />
                  </button>
                  <button
                    className="icon-btn sm danger"
                    title={t("common.delete")}
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
                </span>
              </div>
              {n.isDir && n.children && n.children.length > 0 && (
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

  const fileName = selected
    ? selected
        .slice(selected.lastIndexOf("/") + 1)
        .replace(/\.(mmd|mermaid)$/i, "")
    : "";
  const crumbDirs = selected
    ? parentOf(selected).split("/").filter(Boolean)
    : [];

  return (
    <div className="body">
      <aside className="list">
        <div className="notes-tree-head">
          <RootPicker section="diagrams" />
          <span className="spacer" />
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

      <section className="detail dgm">
        {selected ? (
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
        </p>
      </Modal>

      {/* 저장 안 된 변경 → 다른 파일로 이동 */}
      <Modal
        open={!!pendingOpen}
        title={t("diagrams.unsaved.title")}
        narrow
        onClose={() => setPendingOpen(null)}
        footer={
          <>
            <button className="btn btn-sm" onClick={() => setPendingOpen(null)}>
              {t("diagrams.unsaved.keep")}
            </button>
            <button
              className="btn btn-sm btn-danger-ghost"
              onClick={() => {
                const p = pendingOpen;
                setPendingOpen(null);
                if (p) void doOpen(p);
              }}
            >
              {t("diagrams.unsaved.discard")}
            </button>
          </>
        }
      >
        <p style={{ margin: 0 }}>{t("diagrams.unsaved.body")}</p>
      </Modal>

      {/* 외부에서 먼저 수정됨 → 덮어쓰기/다시 읽기 선택 */}
      <Modal
        open={!!conflict}
        title={t("diagrams.conflict.title")}
        narrow
        onClose={() => setConflict(null)}
        footer={
          <>
            <button className="btn btn-sm" onClick={() => setConflict(null)}>
              {t("diagrams.unsaved.keep")}
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
