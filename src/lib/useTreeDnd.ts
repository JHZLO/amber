// 파일 트리 드래그 앤 드롭 '폴더 이동' 훅 (필기노트·다이어그램 공용).
// dnd-kit 의 DragOverlay 패턴: 드래그되는 행을 문서 최상단(portal)에 '떠 있는 복제본'으로 렌더한다.
//   → 중첩 폴더의 overflow:hidden 클리핑을 벗어나므로 하위 뎁스에서도 안 잘린다(원본 방식의 버그 해결).
// 원본 행은 자리에 흐리게 남고, 오버레이가 커서를 세로로 따라온다. 형제는 밀지 않는다(폴더 이동 전용).
// 놓으면 대상 폴더로 '쏙' 빨려드는 흡수 애니메이션 후 move().
//
// - 텍스트 선택 방지: mousedown 에서 preventDefault (안 하면 드래그가 글자를 선택해버린다).
// - 오버레이 위치는 style.transform 명령형 갱신 → 트리 리렌더 없이 1:1 추적.

import {
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type RefObject,
} from "react";
import { parentOf } from "./vaultTree";

const DRAG_THRESHOLD = 5;
const ABSORB_MS = 200;

interface DndTargetNode {
  name: string;
  path: string;
  isDir: boolean;
}

/** 떠 있는 오버레이(원본 행 복제본)에 그릴 정보. 잡는 순간의 원본 위치/크기를 그대로 쓴다. */
export interface DragOverlayState {
  name: string;
  isDir: boolean;
  left: number;
  top: number;
  width: number;
  height: number;
  padLeft: number;
}

/** 행에 스프레드할 DnD DOM 속성 (data-* 마커 + 드래그 시작 핸들러) */
export interface TreeRowDndProps {
  "data-tree-row": true;
  "data-tree-path": string;
  "data-tree-dir": boolean;
  onMouseDown: (e: ReactMouseEvent) => void;
}

export interface TreeDnd {
  /** 떠 있는 오버레이 상태 (드래그 중에만 non-null) — `<TreeDragOverlay>` 에 전달 */
  drag: DragOverlayState | null;
  /** 오버레이 DOM 참조 (위치를 명령형으로 갱신) */
  overlayRef: RefObject<HTMLDivElement | null>;
  /** 트리 컨테이너(.tree)에 얹을 클래스 — 루트가 드롭 대상이면 'drop-root' */
  treeClass: string;
  /** 행에 얹을 DnD 클래스 — 'dragging'(들린 원본, 흐림) | 'drop-target'(대상 폴더) | '' */
  rowClass: (node: DndTargetNode) => string;
  /** 행에 스프레드할 DnD 속성 (data-* + onMouseDown) */
  rowProps: (node: DndTargetNode) => TreeRowDndProps;
  /** 이 폴더가 지금 드롭 대상인지 — 폴더 아이콘을 '열림'으로 바꿔 받아들이는 표시 */
  isDropTarget: (node: { path: string; isDir: boolean }) => boolean;
  /** 드래그 직후의 click 을 한 번 무시해야 하는지 (행 onClick 맨 앞에서 호출) */
  consumeClick: () => boolean;
}

export function useTreeDnd(opts: {
  move: (fromPath: string, toDir: string) => Promise<string>;
  onMoved: (fromPath: string, newPath: string, isDir: boolean) => void;
  onError: (msg: string) => void;
}): TreeDnd {
  const [dragPath, setDragPath] = useState<string | null>(null);
  const [dropDir, setDropDir] = useState<string | null>(null);
  const [drag, setDrag] = useState<DragOverlayState | null>(null);
  const dropDirRef = useRef<string | null>(null);
  const suppressClick = useRef(false);
  const overlayRef = useRef<HTMLDivElement | null>(null);

  function setDrop(d: string | null) {
    dropDirRef.current = d;
    setDropDir(d);
  }

  // path 로 트리 행 DOM 찾기 (특수문자 안전 — 셀렉터 대신 dataset 순회).
  // scope 를 반드시 넘긴다: 노트·다이어그램 트리가 둘 다 항상 마운트돼 있어(숨김만 됨)
  // document 전체를 훑으면 같은 이름의 폴더에서 안 보이는 쪽 행을 잡는다.
  function findRow(path: string, scope?: HTMLElement | null): HTMLElement | null {
    const rows = (scope ?? document).querySelectorAll<HTMLElement>("[data-tree-row]");
    for (const r of rows) if (r.dataset.treePath === path) return r;
    return null;
  }

  // 커서 밑 요소 → 대상 폴더. 폴더 행이면 그 폴더, 파일 행이면 상위 폴더, 트리 여백이면 루트('').
  function targetDirFrom(el: Element | null): string | null {
    const row = el?.closest<HTMLElement>("[data-tree-row]");
    if (row) {
      const path = row.dataset.treePath ?? "";
      return row.dataset.treeDir === "true" ? path : parentOf(path);
    }
    if (el?.closest(".tree")) return ""; // 트리 안 여백 = 루트
    return null;
  }

  // 이동 불가면 null: 같은 폴더(no-op) / 폴더를 자기 자신·하위로
  function validTarget(from: DndTargetNode, dir: string | null): string | null {
    if (dir == null) return null;
    if (dir === parentOf(from.path)) return null;
    if (from.isDir && (dir === from.path || dir.startsWith(`${from.path}/`)))
      return null;
    return dir;
  }

  function onRowMouseDown(e: ReactMouseEvent, node: DndTargetNode) {
    if (e.button !== 0) return; // 좌클릭만
    if ((e.target as Element).closest(".row-actions")) return; // 행 안 버튼은 드래그 아님
    e.preventDefault(); // 네이티브 텍스트 선택/드래그 방지 (안 하면 글자가 선택된다)

    const rowEl = e.currentTarget as HTMLElement;
    const treeEl = rowEl.closest<HTMLElement>(".tree");
    const rect = rowEl.getBoundingClientRect();
    const padLeft = parseFloat(getComputedStyle(rowEl).paddingLeft) || 8;
    const startX = e.clientX;
    const startY = e.clientY;
    let started = false;

    const onMove = (ev: MouseEvent) => {
      if (!started) {
        if (Math.hypot(ev.clientX - startX, ev.clientY - startY) < DRAG_THRESHOLD)
          return;
        started = true;
        setDragPath(node.path);
        setDrag({
          name: node.name,
          isDir: node.isDir,
          left: rect.left,
          top: rect.top,
          width: rect.width,
          height: rect.height,
          padLeft,
        });
        document.body.classList.add("dragging-rows");
      }
      // 오버레이가 커서를 세로로 따라온다 (portal 이라 클리핑 없음)
      const ov = overlayRef.current;
      if (ov) ov.style.transform = `translateY(${ev.clientY - startY}px)`;
      const el = document.elementFromPoint(ev.clientX, ev.clientY);
      setDrop(validTarget(node, targetDirFrom(el)));
    };

    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      if (!started) return; // 순수 클릭 — onClick 이 처리
      document.body.classList.remove("dragging-rows");
      const target = dropDirRef.current;
      setDrop(null);
      // 드래그 뒤 따라오는 click 억제
      suppressClick.current = true;
      setTimeout(() => {
        suppressClick.current = false;
      }, 0);

      const ov = overlayRef.current;
      if (target != null) {
        // '쏙 들어가는' 흡수 — 오버레이가 대상 폴더(루트면 트리 상단)로 축소·페이드
        if (ov) {
          const dest = target === "" ? treeEl : findRow(target, treeEl);
          const dr = dest?.getBoundingClientRect();
          ov.style.transition = `transform ${ABSORB_MS}ms cubic-bezier(0.4, 0, 1, 1), opacity ${ABSORB_MS}ms ease`;
          if (dr) {
            const tx = dr.left + 14 - rect.left;
            const ty = dr.top + dr.height / 2 - rect.height / 2 - rect.top;
            ov.style.transform = `translate(${tx}px, ${ty}px) scale(0.3)`;
          } else {
            ov.style.transform = `translateY(${ov.getBoundingClientRect().top - rect.top}px) scale(0.4)`;
          }
          ov.style.opacity = "0";
        }
        window.setTimeout(() => setDrag(null), ABSORB_MS);
        opts
          .move(node.path, target)
          .then((newPath) => {
            setDragPath(null);
            opts.onMoved(node.path, newPath, node.isDir);
          })
          .catch((err) => {
            setDragPath(null);
            opts.onError(err instanceof Error ? err.message : String(err));
          });
      } else {
        // 유효 대상 없음 — 제자리 페이드 후 제거
        if (ov) {
          ov.style.transition = "opacity 0.14s ease";
          ov.style.opacity = "0";
        }
        window.setTimeout(() => setDrag(null), 140);
        setDragPath(null);
      }
    };

    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  function consumeClick(): boolean {
    if (!suppressClick.current) return false;
    suppressClick.current = false;
    return true;
  }

  return {
    drag,
    overlayRef,
    treeClass: dropDir === "" ? "drop-root" : "",
    rowClass: (node) =>
      dragPath === node.path
        ? "dragging"
        : node.isDir && dropDir === node.path
          ? "drop-target"
          : "",
    rowProps: (node) => ({
      "data-tree-row": true,
      "data-tree-path": node.path,
      "data-tree-dir": node.isDir,
      onMouseDown: (e) => onRowMouseDown(e, node),
    }),
    isDropTarget: (node) => node.isDir && dropDir === node.path,
    consumeClick,
  };
}
