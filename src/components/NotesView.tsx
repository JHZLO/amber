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
import { usePaneResize } from "../lib/usePaneResize";
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
import { t } from "../lib/i18n";
import { errText } from "../lib/errors";
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

const errMsg = errText; // Rust 코드화 에러까지 번역 (lib/errors.ts)

/** 감소 모션이면 즉시 이동 — CSS `@media (prefers-reduced-motion)` 은 JS scrollTo 의
 *  behavior 를 막지 못하므로 여기서 직접 본다(DESIGN.md §모션: 모든 모션에 감소 대응). */
const scrollBehavior = (): ScrollBehavior =>
  window.matchMedia("(prefers-reduced-motion: reduce)").matches
    ? "auto"
    : "smooth";

/** 목차 안에서 활성 항목이 보이도록 최소한만 스크롤한다.
 *
 *  `scrollIntoView` 를 쓰면 안 된다 — 그건 **스크롤 가능한 조상을 전부** 움직여서 본문
 *  컨테이너(.detail)까지 끌어당긴다. 본문 스크롤이 활성 항목을 바꾸고 그게 다시 본문을
 *  스크롤하는 되먹임이 생긴다. 그래서 목차 자신의 scrollTop 만 직접 건드린다. */
const followActiveInToc = (nav: HTMLElement, id: string) => {
  if (!nav.clientHeight) return; // 좁은 창에서 목차는 display:none (styles.css @media)
  const el = nav.querySelector<HTMLElement>(`[data-toc-id="${CSS.escape(id)}"]`);
  if (!el) return;
  const MARGIN = 12; // 가장자리에 딱 붙지 않게 — 위/아래로 한 항목쯤 더 보이는 여유
  const navBox = nav.getBoundingClientRect();
  const box = el.getBoundingClientRect();
  const above = box.top - navBox.top; // 음수 = 위로 잘려나감
  const below = box.bottom - navBox.bottom; // 양수 = 아래로 잘려나감
  let delta = 0;
  if (above < MARGIN) delta = above - MARGIN;
  else if (below > -MARGIN) delta = below + MARGIN;
  if (!delta) return;
  nav.scrollTo({ top: nav.scrollTop + delta, behavior: scrollBehavior() });
};

type NameModalState =
  | { kind: "new-note" | "new-folder"; name: string; dir: string }
  | { kind: "rename"; name: string; target: NoteNode };

type DeleteTarget = { name: string; path: string; isDir: boolean };

// 저장하려는 순간 디스크가 더 새로울 때(외부 편집) 사용자에게 넘길 선택지
type ConflictState = { path: string; diskMtime: number };

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
  // 한 번이라도 펼친 적 있는 폴더 — 접어도 유지한다(다시 펼칠 때 깜빡이지 않게)
  const [mountedDirs, setMountedDirs] = useState<Set<string>>(new Set());
  const [activeDir, setActiveDir] = useState("");
  const [selected, setSelected] = useState<string | null>(null);

  const [body, setBody] = useState("");
  const [mtime, setMtime] = useState<number | null>(null);
  const [commentCount, setCommentCount] = useState(0);
  const [loadingBody, setLoadingBody] = useState(false);
  const [readError, setReadError] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [previewMd, setPreviewMd] = useState(""); // 편집 프리뷰용(디바운스)
  const [busy, setBusy] = useState(false);
  const [opError, setOpError] = useState<string | null>(null);
  const [conflict, setConflict] = useState<ConflictState | null>(null);
  // 열기 세대 번호 — 늦게 도착한 이전 노트의 응답이 지금 버퍼를 덮지 않게
  const openSeq = useRef(0);

  const [nameModal, setNameModal] = useState<NameModalState | null>(null);
  const [modalError, setModalError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<DeleteTarget | null>(null);
  const [pendingOpen, setPendingOpen] = useState<string | null>(null);
  const [pendingRoot, setPendingRoot] = useState(false);
  const [aiOpen, setAiOpen] = useState(false);
  const [promote, setPromote] = useState<PromoteTarget | null>(null);
  const [madeConcepts, setMadeConcepts] = useState<NoteConceptLink[]>([]);

  // 우측 플로팅 목차 (읽기 모드, h1~h3)
  const detailRef = useRef<HTMLElement | null>(null);
  const mdRef = useRef<HTMLDivElement | null>(null);
  const tocRef = useRef<HTMLElement | null>(null);
  const [toc, setToc] = useState<{ id: string; text: string; level: number }[]>(
    [],
  );
  const [activeHeading, setActiveHeading] = useState("");

  const dirty = editing && draft !== body;
  // WORKSPACE_EVENT 리스너가 [] 성격으로 붙으므로 ref 로 최신 dirty 를 본다
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;

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

  // 긴 글은 목차 자체가 스크롤된다(styles.css .note-toc). 본문을 내리다 활성 항목이 그
  // 스크롤 밖으로 나가면 하이라이트가 안 보여 목차가 멈춘 것처럼 읽힌다 — 따라 움직인다.
  useEffect(() => {
    if (!activeHeading || !tocRef.current) return;
    followActiveInToc(tocRef.current, activeHeading);
    // toc 도 의존성이다 — 헤딩 id 는 인덱스 기반(note-h-0…)이라 다른 글로 넘어가도 활성 id 가
    // 그대로일 수 있다. 그러면 목차만 이전 글의 스크롤 위치에 남는다.
  }, [activeHeading, toc]);

  function scrollToHeading(id: string) {
    setActiveHeading(id);
    document
      .getElementById(id)
      ?.scrollIntoView({ behavior: scrollBehavior(), block: "start" });
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

  // 워크스페이스 루트("폴더 열기") 변경 → 선택/트리 상태 초기화 후 새 루트 로드.
  // 같은 트리의 다른 노트를 여는 것도 확인을 받는 마당에, 더 많이 잃는 이 전환이 무경고면 안 된다.
  const applyRootChange = useCallback(() => {
    setSelected(null);
    setEditing(false);
    setExpanded(new Set());
    setMountedDirs(new Set());
    setActiveDir("");
    setOpError(null);
    void reload();
  }, [reload]);

  useEffect(() => {
    const h = (e: Event) => {
      if ((e as CustomEvent).detail !== "notes") return;
      // 이벤트는 섹션 이름만 싣고 오므로, 진행 시점에 getRoot 로 새 루트를 읽는다
      if (dirtyRef.current) setPendingRoot(true);
      else applyRootChange();
    };
    window.addEventListener(WORKSPACE_EVENT, h);
    return () => window.removeEventListener(WORKSPACE_EVENT, h);
  }, [applyRootChange]);

  // 편집 중 타이핑 → 350ms 디바운스로 프리뷰 갱신 (mermaid 펜스 안에서 글자마다 재렌더 방지)
  useEffect(() => {
    if (!editing) return;
    const t = setTimeout(() => setPreviewMd(draft), 350);
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
    setMountedDirs((prev) => new Set([...prev, ...paths]));
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
    setMountedDirs((prev) => new Set([...prev].map(map)));
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
    setCommentCount(0); // 새 노트의 질문 수는 레이어가 로드 후 갱신
    setLoadingBody(true);
    try {
      const b = await readNoteFile(path);
      const m = await noteMtime(path);
      if (seq !== openSeq.current) return; // 그 사이 다른 노트를 열었다
      setBody(b);
      if (opts?.edit) {
        setDraft(b);
        setPreviewMd(b);
        setEditing(true);
      }
      setMtime(m);
    } catch (e) {
      if (seq !== openSeq.current) return;
      // 실패를 본문으로 위장하지 않는다 — 그 문자열이 초안이 되면 ⌘S 한 번에 원본이 날아간다
      setBody("");
      setReadError(errMsg(e));
      setMtime(null);
    } finally {
      if (seq === openSeq.current) setLoadingBody(false);
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
    // 마운트는 펼침보다 먼저 일어나야 0fr→1fr 트랜지션이 첫 프레임부터 돈다
    setMountedDirs((prev) => (prev.has(n.path) ? prev : new Set(prev).add(n.path)));
    setActiveDir(n.path);
  }

  async function save(opts?: { force?: boolean }) {
    if (!editing || !selected || busy || readError) return;
    setBusy(true);
    try {
      // 열 때 잡아둔 mtime 보다 디스크가 새로우면 외부(Obsidian/vim/git)가 먼저 고친 것 —
      // 조용히 덮지 않고 사용자에게 선택을 넘긴다
      if (!opts?.force) {
        const cur = await noteMtime(selected);
        if (cur !== null && mtime !== null && cur > mtime) {
          setConflict({ path: selected, diskMtime: cur });
          return;
        }
      }
      await writeNoteFile(selected, draft);
      setBody(draft);
      setEditing(false);
      setMtime((await noteMtime(selected)) ?? Date.now());
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
      const cur = await noteMtime(selected);
      if (cur !== null && mtime !== null && cur > mtime)
        setConflict({ path: selected, diskMtime: cur });
      return;
    }
    await doOpen(selected);
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

  const pane = usePaneResize({ storageKey: "amber.notes.list-width", active });

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
                      <Tooltip label={t("notes.row.newNoteHere")}>
                        <button
                          aria-label={t("notes.row.newNoteHere")}
                          className="icon-btn sm"
                          onClick={() => openNameModal("new-note", n.path)}
                        >
                          <Icon name="file-plus" size={13} />
                        </button>
                      </Tooltip>
                      <Tooltip label={t("notes.row.newFolderHere")}>
                        <button
                          aria-label={t("notes.row.newFolderHere")}
                          className="icon-btn sm"
                          onClick={() => openNameModal("new-folder", n.path)}
                        >
                          <Icon name="folder-plus" size={13} />
                        </button>
                      </Tooltip>
                    </>
                  )}
                  <Tooltip label={t("notes.rename")}>
                    <button
                      aria-label={t("notes.rename")}
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

  const fileName = selected
    ? selected.slice(selected.lastIndexOf("/") + 1).replace(/\.md$/i, "")
    : "";
  const crumbDirs = selected ? parentOf(selected).split("/").filter(Boolean) : [];

  // 삭제 확인 문구 — 언어별 어순이 달라 "{name}" 자리에 <b>이름</b>을 끼워 넣는다
  const delMsg = t(
    confirmDelete?.isDir ? "notes.delete.confirmFolder" : "notes.delete.confirmNote",
  ).split("{name}");

  return (
    <div className="body" {...pane.bodyProps}>
      <aside className="list">
        <div className="notes-tree-head">
          <RootPicker section="notes" />
          <span className="spacer" />
          <Tooltip label={t("notes.tooltip.newNote", { dir: encodeDir(activeDir) })}>
            <button
              className="icon-btn sm"
              aria-label={t("notes.newNote")}
              onClick={() => openNameModal("new-note")}
            >
              <Icon name="file-plus" size={15} />
            </button>
          </Tooltip>
          <Tooltip label={t("notes.tooltip.newFolder", { dir: encodeDir(activeDir) })}>
            <button
              className="icon-btn sm"
              aria-label={t("notes.newFolder")}
              onClick={() => openNameModal("new-folder")}
            >
              <Icon name="folder-plus" size={15} />
            </button>
          </Tooltip>
          <Tooltip label={t("notes.tooltip.refresh")}>
            <button
              className="icon-btn sm"
              aria-label={t("notes.refresh")}
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
              {t("notes.tree.empty.lead")}
              <br />
              {t("notes.tree.empty.sub")}
            </p>
            <button
              className="btn btn-primary btn-sm"
              onClick={() => openNameModal("new-note")}
            >
              <Icon name="file-plus" size={14} />
              {t("notes.tree.firstNote")}
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

      <div {...pane.resizerProps} />

      <section className="detail" ref={detailRef}>
        {selected ? (
          <div className={`notes-detail ${editing ? "editing" : ""}`}>
            <div className="note-crumb">
              <Icon name="folder" size={12} />
              {[t("notes.title"), ...crumbDirs].join(" › ")}
            </div>
            <div className="detail-head">
              <h1 className="detail-title">{fileName}</h1>
            </div>

            {opError && <div className="error-note">{opError}</div>}
            {readError && (
              <div className="error-note">
                {t("notes.readError", { error: readError })}
              </div>
            )}

            <div className="detail-actions detail-actions-split">
              <div className="detail-actions-group">
                {editing ? (
                  <>
                    <button
                      className="btn btn-primary btn-sm"
                      onClick={() => void save()}
                      disabled={busy}
                    >
                      {busy ? t("notes.saving") : t("notes.saveCmd")}
                    </button>
                    <button
                      className="btn btn-sm"
                      onClick={() => setEditing(false)}
                      disabled={busy}
                    >
                      {t("common.cancel")}
                    </button>
                    <button
                      className="btn btn-sm"
                      onClick={() => setAiOpen(true)}
                      disabled={busy || !config?.provider}
                      title={t("notes.ai.fromDraftTip")}
                    >
                      <Icon name="sparkles" size={14} />
                      {t("notes.aiWrite")}
                    </button>
                  </>
                ) : (
                  <>
                    <button
                      className="btn btn-sm"
                      onClick={() => {
                        setDraft(body);
                        setPreviewMd(body);
                        setEditing(true);
                      }}
                      disabled={busy || loadingBody || !!readError}
                    >
                      <Icon name="pencil" size={14} />
                      {t("notes.edit")}
                    </button>
                    <button
                      className="btn btn-sm"
                      onClick={() => setAiOpen(true)}
                      disabled={
                        busy || loadingBody || !!readError || !config?.provider
                      }
                      title={t("notes.ai.fromNoteTip")}
                    >
                      <Icon name="sparkles" size={14} />
                      {t("notes.aiWrite")}
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
                  {t("common.delete")}
                </button>
              </div>
            </div>

            {loadingBody ? (
              <Spinner />
            ) : readError ? null : editing ? (
              // 좌 소스 / 우 라이브 프리뷰 (PRD §4.2 편집 모드)
              <div className="note-edit-split">
                <textarea
                  className="textarea note-textarea"
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  spellCheck={false}
                />
                <div className="markdown note-preview">
                  <Markdown>{previewMd}</Markdown>
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
                  <nav className="note-toc" ref={tocRef}>
                    <div className="note-toc-label">{t("notes.toc")}</div>
                    {toc.map((t) => (
                      <button
                        key={t.id}
                        data-toc-id={t.id}
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
                  <Icon name="layers" size={12} />
                  {t("notes.madeConcepts")}
                </span>
                {madeConcepts.map((c) => (
                  <button
                    key={c.conceptId}
                    className="chip btn-like"
                    title={t("notes.openConcept", { anchor: c.anchor })}
                    onClick={() => openConceptInApp(c.conceptId)}
                  >
                    {c.title}
                  </button>
                ))}
              </div>
            )}

            <div className="detail-meta">
              {rootDisplayName("notes")}/{selected}
              {mtime !== null && (
                <> · {t("notes.meta.modified", { time: timeAgo(mtime) })}</>
              )}
              {commentCount > 0 && (
                <>
                  {" · "}
                  {commentCount === 1
                    ? t("notes.meta.qcount.one")
                    : t("notes.meta.qcount.other", { n: commentCount })}
                </>
              )}
            </div>
          </div>
        ) : (
          <div className="notes-empty">
            <div className="notes-empty-icon">
              <Icon name="book" size={30} />
            </div>
            <h2 className="notes-empty-title">{t("notes.title")}</h2>
            <p className="notes-empty-sub">
              {t("notes.empty.lead")}
              <br />
              {t("notes.empty.sub")}
            </p>
            <div className="notes-empty-actions">
              <button
                className="btn btn-primary btn-sm"
                onClick={() => openNameModal("new-note")}
              >
                <Icon name="file-plus" size={14} />
                {t("notes.newNote")}
              </button>
              <button
                className="btn btn-sm"
                onClick={() => openNameModal("new-folder")}
              >
                <Icon name="folder-plus" size={14} />
                {t("notes.newFolder")}
              </button>
            </div>
            <div className="notes-empty-tips">
              <span className="notes-empty-tip">
                <Icon name="sparkles" size={12} /> {t("notes.empty.tipAi")}
              </span>
              <span className="notes-empty-tip">
                <Icon name="expand" size={12} /> {t("notes.empty.tipMermaid")}
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
            ? t("notes.newNote")
            : nameModal?.kind === "new-folder"
              ? t("notes.newFolder")
              : t("notes.rename")
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
                ? t("notes.working")
                : nameModal?.kind === "rename"
                  ? t("notes.renameConfirm")
                  : t("notes.create")}
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
                <label>{t("notes.field.location")}</label>
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
                  ? t("notes.field.folderName")
                  : t("notes.field.noteName")}
              </label>
              <input
                className="input"
                autoFocus
                value={nameModal.name}
                placeholder={
                  nameModal.kind === "new-folder"
                    ? t("notes.ph.folderName")
                    : t("notes.ph.noteName")
                }
                onChange={(e) =>
                  setNameModal((m) =>
                    m ? { ...m, name: e.target.value } : m,
                  )
                }
              />
              {nameModal.kind !== "rename" && (
                <div className="hint">{t("notes.hint.path")}</div>
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
            ? t("notes.delete.folderTitle")
            : t("notes.delete.noteTitle")
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
              {busy ? t("notes.deleting") : t("common.delete")}
            </button>
          </>
        }
      >
        <p style={{ margin: 0 }}>
          {delMsg[0]}
          <b>{confirmDelete?.name}</b>
          {delMsg[1]}
          <br />
          {t("notes.delete.trashHint")}
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
            setPreviewMd(md);
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
        title={t("notes.conflict.title")}
        narrow
        onClose={() => setConflict(null)}
        footer={
          <>
            <button className="btn btn-sm" onClick={() => setConflict(null)}>
              {t("notes.keepEditing")}
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
              {t("notes.conflict.reload")}
            </button>
            <button
              className="btn btn-sm btn-danger-ghost"
              onClick={() => {
                setConflict(null);
                void save({ force: true });
              }}
              disabled={busy}
            >
              {t("notes.conflict.overwrite")}
            </button>
          </>
        }
      >
        <p style={{ margin: 0 }}>
          {t("notes.conflict.body")}
          {conflict && (
            <>
              {" · "}
              {t("notes.conflict.diskTime", { time: timeAgo(conflict.diskMtime) })}
            </>
          )}
          {"."}
          <br />
          {t("notes.conflict.warn")}
        </p>
      </Modal>
    </div>
  );
}
