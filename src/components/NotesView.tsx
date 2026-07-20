// 필기노트 섹션: 좌측 폴더 트리 + 우측 마크다운 읽기/편집 2-pane.
// 정본은 vault/notes/ 의 실제 디렉토리/.md 파일 (lib/notes.ts). DB 없음.

import { useCallback, useEffect, useRef, useState } from "react";
import { Markdown } from "./Markdown";
import { NoteCommentLayer } from "./NoteComments";
import {
  createFolder,
  createNote,
  deleteEntry,
  flattenDirs,
  invalidNameReason,
  invalidPathReason,
  listNoteTree,
  moveEntry,
  noteMtime,
  parentOf,
  readNoteFile,
  renameEntry,
  writeNoteFile,
  type NoteNode,
} from "../lib/notes";
import { useTreeDnd } from "../lib/useTreeDnd";
import { Modal, Select, Spinner, TreeDragOverlay, timeAgo } from "../ui";
import { Icon } from "../icons";
import type { AppConfig } from "../lib/config";
import { NoteAiModal } from "./NoteAiModal";
import { RootPicker } from "./RootPicker";
import { rootDisplayName, WORKSPACE_EVENT } from "../lib/workspace";
import {
  PromoteConceptModal,
  type PromoteTarget,
} from "./PromoteConceptModal";
import { loadNoteConcepts, type NoteConceptLink } from "../lib/noteConcepts";
import { openConceptInApp, OPEN_NOTE } from "../lib/nav";
import { emit } from "@tauri-apps/api/event";

// 이동/생성 위치 Select 값 인코딩 (루트 '' ↔ '/')
const encodeDir = (d: string) => (d ? `/${d}` : "/");
const decodeDir = (v: string) => (v === "/" ? "" : v.slice(1));

const errMsg = (e: unknown) => (e instanceof Error ? e.message : String(e));

type NameModalState =
  | { kind: "new-note" | "new-folder"; name: string; dir: string }
  | { kind: "rename"; name: string; target: NoteNode };

type DeleteTarget = { name: string; path: string; isDir: boolean };

export function NotesView({
  active,
  config,
}: {
  active: boolean;
  config: AppConfig | null;
}) {
  const [tree, setTree] = useState<NoteNode[] | null>(null);
  const [treeError, setTreeError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [activeDir, setActiveDir] = useState("");
  const [selected, setSelected] = useState<string | null>(null);

  const [body, setBody] = useState("");
  const [mtime, setMtime] = useState<number | null>(null);
  const [commentCount, setCommentCount] = useState(0);
  const [loadingBody, setLoadingBody] = useState(false);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [opError, setOpError] = useState<string | null>(null);

  const [nameModal, setNameModal] = useState<NameModalState | null>(null);
  const [modalError, setModalError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<DeleteTarget | null>(null);
  const [pendingOpen, setPendingOpen] = useState<string | null>(null);
  const [aiOpen, setAiOpen] = useState(false);
  const [promote, setPromote] = useState<PromoteTarget | null>(null);
  const [madeConcepts, setMadeConcepts] = useState<NoteConceptLink[]>([]);

  // 우측 플로팅 목차 (읽기 모드, h1~h3)
  const detailRef = useRef<HTMLElement | null>(null);
  const mdRef = useRef<HTMLDivElement | null>(null);
  const [toc, setToc] = useState<{ id: string; text: string; level: number }[]>(
    [],
  );
  const [activeHeading, setActiveHeading] = useState("");

  const dirty = editing && draft !== body;

  // 본문 렌더 후 DOM 에서 헤딩 수집 — 소스 정규식과 달리 코드블록 안 '#' 오탐이 없다.
  // id 는 인덱스 기반이라 같은 제목이 중복돼도 안전.
  useEffect(() => {
    if (editing || loadingBody || !selected || !mdRef.current) {
      setToc([]);
      return;
    }
    const hs = Array.from(
      mdRef.current.querySelectorAll<HTMLHeadingElement>("h1, h2, h3"),
    );
    const entries = hs
      .map((el, i) => {
        el.id = `note-h-${i}`;
        return {
          id: el.id,
          text: el.textContent?.trim() ?? "",
          level: Number(el.tagName[1]),
        };
      })
      .filter((e) => e.text);
    setToc(entries);
    setActiveHeading(entries[0]?.id ?? "");
  }, [body, editing, loadingBody, selected]);

  // 스크롤 스파이: 스크롤 위치 기준 현재 섹션 하이라이트
  useEffect(() => {
    const container = detailRef.current;
    if (!container || toc.length === 0) return;
    let ticking = false;
    const compute = () => {
      ticking = false;
      const cTop = container.getBoundingClientRect().top;
      let cur = toc[0].id;
      for (const t of toc) {
        const el = document.getElementById(t.id);
        if (!el) continue;
        if (el.getBoundingClientRect().top - cTop <= 96) cur = t.id;
        else break;
      }
      // 값이 바뀔 때만 setState (같으면 React 가 bail-out → 재렌더 없음)
      setActiveHeading(cur);
    };
    // rAF 스로틀: 스크롤 이벤트당 레이아웃 측정을 프레임당 1회로 제한
    const onScroll = () => {
      if (ticking) return;
      ticking = true;
      requestAnimationFrame(compute);
    };
    compute();
    container.addEventListener("scroll", onScroll, { passive: true });
    return () => container.removeEventListener("scroll", onScroll);
  }, [toc]);

  function scrollToHeading(id: string) {
    setActiveHeading(id);
    document
      .getElementById(id)
      ?.scrollIntoView({ behavior: "smooth", block: "start" });
  }

  const reload = useCallback(async () => {
    try {
      setTree(await listNoteTree());
      setTreeError(null);
    } catch (e) {
      setTreeError(errMsg(e));
    }
  }, []);

  // 필기노트 탭에 들어올 때마다 트리를 다시 읽는다 (Finder/git/외부 편집 자동 반영)
  useEffect(() => {
    if (active) void reload();
  }, [active, reload]);

  // 워크스페이스 루트("폴더 열기") 변경 → 선택/트리 상태 초기화 후 새 루트 로드
  useEffect(() => {
    const h = (e: Event) => {
      if ((e as CustomEvent).detail !== "notes") return;
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
    setSelected(path);
    setActiveDir(parentOf(path));
    setEditing(false);
    setOpError(null);
    setCommentCount(0); // 새 노트의 질문 수는 레이어가 로드 후 갱신
    setLoadingBody(true);
    try {
      const b = await readNoteFile(path);
      setBody(b);
      if (opts?.edit) {
        setDraft(b);
        setEditing(true);
      }
      setMtime(await noteMtime(path));
    } catch {
      setBody("_(노트를 읽을 수 없습니다)_");
      setMtime(null);
    } finally {
      setLoadingBody(false);
    }
  }

  function openNote(path: string) {
    if (path === selected && !editing) return;
    if (dirty) {
      setPendingOpen(path);
      return;
    }
    void doOpen(path);
  }

  // 선택된 노트에서 만든 개념(역참조) 로드
  const loadMade = useCallback((rel: string | null) => {
    if (!rel) {
      setMadeConcepts([]);
      return;
    }
    loadNoteConcepts(rel)
      .then(setMadeConcepts)
      .catch(() => setMadeConcepts([]));
  }, []);
  useEffect(() => {
    loadMade(selected);
  }, [selected, loadMade]);

  // 개념 상세의 "출처 노트 열기" → 이 노트를 연다 (섹션 전환은 App 이 처리)
  const openNoteRef = useRef(openNote);
  openNoteRef.current = openNote;
  useEffect(() => {
    const h = (e: Event) => {
      const path = (e as CustomEvent<{ path: string }>).detail?.path;
      if (typeof path === "string") openNoteRef.current(path);
    };
    window.addEventListener(OPEN_NOTE, h);
    return () => window.removeEventListener(OPEN_NOTE, h);
  }, []);

  function toggleDir(n: NoteNode) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(n.path)) next.delete(n.path);
      else next.add(n.path);
      return next;
    });
    setActiveDir(n.path);
  }

  async function save() {
    if (!editing || !selected || busy) return;
    setBusy(true);
    try {
      await writeNoteFile(selected, draft);
      setBody(draft);
      setEditing(false);
      setMtime(Date.now());
      setOpError(null);
    } catch (e) {
      setOpError(errMsg(e));
    } finally {
      setBusy(false);
    }
  }

  // ⌘S 저장 (노트 섹션이 보일 때 + 편집 중일 때만)
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

  // 창에 포커스가 돌아오면(예: Finder 에서 파일 편집 후 복귀) 노트 섹션일 때 트리 갱신.
  // 편집 중 초안은 트리와 별개라 영향 없음.
  const reloadRef = useRef(reload);
  reloadRef.current = reload;
  useEffect(() => {
    const onFocus = () => {
      if (activeRef.current) void reloadRef.current();
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  function openNameModal(kind: "new-note" | "new-folder", dir?: string) {
    setModalError(null);
    setNameModal({ kind, name: "", dir: dir ?? activeDir });
  }

  function openRenameModal(target: NoteNode) {
    setModalError(null);
    setNameModal({ kind: "rename", name: target.name, target });
  }

  async function submitNameModal() {
    const m = nameModal;
    if (!m || busy) return;
    const name = m.name.trim();
    // 생성은 'CS/네트워크' 다단계 경로 허용, 이름 변경은 단일 이름만
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
        else if (selected === t.path) {
          setSelected(newRel);
        }
        setNameModal(null);
        await reload();
      } else if (m.kind === "new-note") {
        const rel = await createNote(m.dir, name);
        setNameModal(null);
        await reload();
        expandTo(parentOf(rel));
        // 편집 중 초안이 있으면 열지 않고 생성만 (초안 보호)
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

  // 파일 트리 드래그 이동 — 다른 폴더/루트로 놓으면 파일·폴더를 옮긴다 (사이드카 함께).
  const dnd = useTreeDnd({
    move: moveEntry,
    onMoved: (fromPath, newPath, isDir) => {
      if (isDir) remapPrefix(fromPath, newPath);
      else if (selected === fromPath) setSelected(newPath);
      expandTo(parentOf(newPath)); // 옮겨간 위치를 펼쳐 보여준다
      setOpError(null);
      void reload();
    },
    onError: setOpError,
  });

  // 컴포넌트가 아닌 렌더 함수 — 렌더마다 트리 DOM 이 리마운트되지 않게.
  // 하위는 조건부 언마운트 대신 항상 렌더하고 CSS grid(0fr↔1fr)로 펼침 → 부드러운 전개 애니메이션.
  function renderRows(nodes: NoteNode[], depth: number) {
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
                  n.isDir ? toggleDir(n) : openNote(n.path);
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
                      : "file"
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
                        title="이 폴더에 새 노트"
                        onClick={() => openNameModal("new-note", n.path)}
                      >
                        <Icon name="file-plus" size={13} />
                      </button>
                      <button
                        className="icon-btn sm"
                        title="이 폴더에 새 폴더"
                        onClick={() => openNameModal("new-folder", n.path)}
                      >
                        <Icon name="folder-plus" size={13} />
                      </button>
                    </>
                  )}
                  <button
                    className="icon-btn sm"
                    title="이름 변경"
                    onClick={() => openRenameModal(n)}
                  >
                    <Icon name="pencil" size={13} />
                  </button>
                  <button
                    className="icon-btn sm danger"
                    title="삭제"
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
    ? selected.slice(selected.lastIndexOf("/") + 1).replace(/\.md$/i, "")
    : "";
  const crumbDirs = selected ? parentOf(selected).split("/").filter(Boolean) : [];

  return (
    <div className="body">
      <aside className="list">
        <div className="notes-tree-head">
          <RootPicker section="notes" />
          <span className="spacer" />
          <button
            className="icon-btn sm"
            title={`새 노트 (위치: ${encodeDir(activeDir)})`}
            onClick={() => openNameModal("new-note")}
          >
            <Icon name="file-plus" size={15} />
          </button>
          <button
            className="icon-btn sm"
            title={`새 폴더 (위치: ${encodeDir(activeDir)})`}
            onClick={() => openNameModal("new-folder")}
          >
            <Icon name="folder-plus" size={15} />
          </button>
          <button
            className="icon-btn sm"
            title="새로고침 (Finder 에서 바꾼 내용 반영)"
            onClick={() => void reload()}
          >
            <Icon name="refresh" size={14} />
          </button>
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
              아직 노트가 없어요.
              <br />
              폴더로 분류하며 마크다운으로 기록해 보세요.
            </p>
            <button
              className="btn btn-primary btn-sm"
              onClick={() => openNameModal("new-note")}
            >
              <Icon name="file-plus" size={14} />첫 노트 만들기
            </button>
          </div>
        )}
        {tree && tree.length > 0 && (
          // 연타 시 더블/트리플클릭 텍스트 선택이 사이드바 전체로 번지는 것 차단.
          // (행은 user-select:none 이라 선택이 상위로 "탈출"함 → mousedown 기본동작 자체를 막음.
          //  클릭 이벤트는 별개라 토글/열기/버튼은 정상 동작.)
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
        <TreeDragOverlay drag={dnd.drag} leafIcon="file" overlayRef={dnd.overlayRef} />
      )}

      <section className="detail" ref={detailRef}>
        {selected ? (
          <div className={`notes-detail ${editing ? "editing" : ""}`}>
            <div className="note-crumb">
              <Icon name="folder" size={12} />
              {["필기노트", ...crumbDirs].join(" › ")}
            </div>
            <div className="detail-head">
              <h1 className="detail-title">{fileName}</h1>
            </div>

            {opError && <div className="error-note">{opError}</div>}

            <div className="detail-actions detail-actions-split">
              <div className="detail-actions-group">
                {editing ? (
                  <>
                    <button
                      className="btn btn-primary btn-sm"
                      onClick={() => void save()}
                      disabled={busy}
                    >
                      {busy ? "저장 중…" : "저장 (⌘S)"}
                    </button>
                    <button
                      className="btn btn-sm"
                      onClick={() => setEditing(false)}
                      disabled={busy}
                    >
                      취소
                    </button>
                    <button
                      className="btn btn-sm"
                      onClick={() => setAiOpen(true)}
                      disabled={busy || !config?.provider}
                      title="현재 초안을 바탕으로 AI가 작성/보강"
                    >
                      <Icon name="sparkles" size={14} />
                      AI 작성
                    </button>
                  </>
                ) : (
                  <>
                    <button
                      className="btn btn-sm"
                      onClick={() => {
                        setDraft(body);
                        setEditing(true);
                      }}
                      disabled={busy || loadingBody}
                    >
                      <Icon name="pencil" size={14} />
                      편집
                    </button>
                    <button
                      className="btn btn-sm"
                      onClick={() => setAiOpen(true)}
                      disabled={busy || loadingBody || !config?.provider}
                      title="현재 노트를 바탕으로 AI가 작성/보강"
                    >
                      <Icon name="sparkles" size={14} />
                      AI 작성
                    </button>
                  </>
                )}
              </div>
              <div className="detail-actions-group">
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
                  삭제
                </button>
              </div>
            </div>

            {loadingBody ? (
              <Spinner />
            ) : editing ? (
              // 좌 소스 / 우 라이브 프리뷰 (PRD §4.2 편집 모드)
              <div className="note-edit-split">
                <textarea
                  className="textarea note-textarea"
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  spellCheck={false}
                />
                <div className="markdown note-preview">
                  <Markdown>{draft}</Markdown>
                </div>
              </div>
            ) : (
              <div className="note-read-wrap">
                <div className="markdown" ref={mdRef}>
                  <Markdown>{body}</Markdown>
                </div>
                {/* 드래그 → 질문(AI 답변) / 개념으로(승격). 본문 밖 사이드카에 저장 */}
                <NoteCommentLayer
                  key={selected}
                  noteRel={selected}
                  body={body}
                  containerRef={mdRef}
                  config={config}
                  onCountChange={setCommentCount}
                  onPromote={(selection) =>
                    setPromote({ noteRel: selected, selection, noteBody: body })
                  }
                />
                {toc.length >= 2 && (
                  <nav className="note-toc">
                    <div className="note-toc-label">목차</div>
                    {toc.map((t) => (
                      <button
                        key={t.id}
                        className={`note-toc-item lv${t.level} ${
                          activeHeading === t.id ? "active" : ""
                        }`}
                        title={t.text}
                        onClick={() => scrollToHeading(t.id)}
                      >
                        {t.text}
                      </button>
                    ))}
                  </nav>
                )}
              </div>
            )}

            {!editing && madeConcepts.length > 0 && (
              <div className="note-made-concepts">
                <span className="note-made-label">
                  <Icon name="layers" size={12} />이 노트에서 만든 개념
                </span>
                {madeConcepts.map((c) => (
                  <button
                    key={c.conceptId}
                    className="chip btn-like"
                    title={`개념 열기 — “${c.anchor}”`}
                    onClick={() => openConceptInApp(c.conceptId)}
                  >
                    {c.title}
                  </button>
                ))}
              </div>
            )}

            <div className="detail-meta">
              {rootDisplayName("notes")}/{selected}
              {mtime !== null && <> · 수정 {timeAgo(mtime)}</>}
              {commentCount > 0 && <> · 질문 {commentCount}개</>}
            </div>
          </div>
        ) : (
          <div className="notes-empty">
            <div className="notes-empty-icon">
              <Icon name="book" size={30} />
            </div>
            <h2 className="notes-empty-title">필기노트</h2>
            <p className="notes-empty-sub">
              왼쪽에서 노트를 열거나, 새 노트를 만들어 기록을 시작하세요.
              <br />
              폴더로 분류하고 마크다운으로 자유롭게 적을 수 있어요.
            </p>
            <div className="notes-empty-actions">
              <button
                className="btn btn-primary btn-sm"
                onClick={() => openNameModal("new-note")}
              >
                <Icon name="file-plus" size={14} />새 노트
              </button>
              <button
                className="btn btn-sm"
                onClick={() => openNameModal("new-folder")}
              >
                <Icon name="folder-plus" size={14} />새 폴더
              </button>
            </div>
            <div className="notes-empty-tips">
              <span className="notes-empty-tip">
                <Icon name="sparkles" size={12} /> AI 작성으로 초안을 받아 보강
              </span>
              <span className="notes-empty-tip">
                <Icon name="expand" size={12} /> mermaid 다이어그램 렌더 · 확대
              </span>
            </div>
          </div>
        )}
      </section>

      {/* 새 노트 / 새 폴더 / 이름 변경 */}
      <Modal
        open={!!nameModal}
        title={
          nameModal?.kind === "new-note"
            ? "새 노트"
            : nameModal?.kind === "new-folder"
              ? "새 폴더"
              : "이름 변경"
        }
        onClose={() => setNameModal(null)}
        footer={
          <>
            <button
              className="btn btn-sm"
              onClick={() => setNameModal(null)}
              disabled={busy}
            >
              취소
            </button>
            <button
              className="btn btn-primary btn-sm"
              onClick={() => void submitNameModal()}
              disabled={busy}
            >
              {busy
                ? "처리 중…"
                : nameModal?.kind === "rename"
                  ? "변경"
                  : "만들기"}
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
                <label>위치</label>
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
                {nameModal.kind === "new-folder" ? "폴더 이름" : "노트 이름"}
              </label>
              <input
                className="input"
                autoFocus
                value={nameModal.name}
                placeholder={
                  nameModal.kind === "new-folder"
                    ? "예: 네트워크  ·  CS/네트워크"
                    : "예: TCP 혼잡 제어  ·  네트워크/TCP"
                }
                onChange={(e) =>
                  setNameModal((m) =>
                    m ? { ...m, name: e.target.value } : m,
                  )
                }
              />
              {nameModal.kind !== "rename" && (
                <div className="hint">
                  / 로 구분하면 중간 폴더가 자동으로 만들어져요.
                </div>
              )}
            </div>
            {modalError && <div className="error-note">{modalError}</div>}
          </form>
        )}
      </Modal>

      {/* 삭제 확인 */}
      <Modal
        open={!!confirmDelete}
        title={confirmDelete?.isDir ? "폴더 삭제" : "노트 삭제"}
        narrow
        onClose={() => setConfirmDelete(null)}
        footer={
          <>
            <button
              className="btn btn-sm"
              onClick={() => setConfirmDelete(null)}
              disabled={busy}
            >
              취소
            </button>
            <button
              className="btn btn-sm btn-danger-ghost"
              onClick={() => void doDelete()}
              disabled={busy}
            >
              {busy ? "삭제 중…" : "삭제"}
            </button>
          </>
        }
      >
        <p style={{ margin: 0 }}>
          <b>{confirmDelete?.name}</b>{" "}
          {confirmDelete?.isDir ? "폴더와 안의 모든 노트를" : "노트를"}{" "}
          삭제할까요?
          <br />
          되돌릴 수 없어요.
        </p>
      </Modal>

      {/* AI 작성 — 결과는 에디터 초안으로 적용, 저장은 사용자가 ⌘S */}
      {selected && (
        <NoteAiModal
          open={aiOpen}
          title={fileName}
          currentBody={editing ? draft : body}
          config={config}
          onClose={() => setAiOpen(false)}
          onApplied={(md) => {
            setDraft(md);
            setEditing(true);
          }}
        />
      )}

      {/* 선택 영역 → 개념 승격 */}
      <PromoteConceptModal
        target={promote}
        config={config}
        onClose={() => setPromote(null)}
        onDone={(_id, title) => {
          setPromote(null);
          loadMade(selected); // 노트 푸터의 "만든 개념" 갱신
          void emit("concept-changed"); // 개념 탭·위젯 반영
          setOpError(null);
          void title;
        }}
      />

      {/* 저장 안 된 변경 → 다른 노트로 이동 */}
      <Modal
        open={!!pendingOpen}
        title="저장하지 않은 변경"
        narrow
        onClose={() => setPendingOpen(null)}
        footer={
          <>
            <button
              className="btn btn-sm"
              onClick={() => setPendingOpen(null)}
            >
              계속 편집
            </button>
            <button
              className="btn btn-sm btn-danger-ghost"
              onClick={() => {
                const p = pendingOpen;
                setPendingOpen(null);
                if (p) void doOpen(p);
              }}
            >
              버리고 이동
            </button>
          </>
        }
      >
        <p style={{ margin: 0 }}>
          지금 노트에 저장하지 않은 변경이 있어요. 버리고 이동할까요?
        </p>
      </Modal>
    </div>
  );
}
