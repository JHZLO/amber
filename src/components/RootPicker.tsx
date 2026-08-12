// 트리 헤더의 워크스페이스 루트 전환기 — VS Code 의 "폴더 열기 / Open Recent" 대응.
// 현재 루트 이름을 보여주고, 클릭하면 최근 폴더 목록 + 기본 보관함 + 폴더 열기 메뉴.

import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  DEFAULT_ROOTS,
  getRecentRoots,
  getRoot,
  isDefaultRoot,
  rootDisplayName,
  rootDisplayPath,
  setRoot,
  WORKSPACE_EVENT,
  type SectionKey,
} from "../lib/workspace";
import { t } from "../lib/i18n";
import { Icon } from "../icons";

export function RootPicker({ section }: { section: SectionKey }) {
  const [root, setRootState] = useState(() => getRoot(section));
  const [path, setPath] = useState(""); // 루트의 ~ 축약 절대경로 (비동기 해석)
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  // 외부(다른 뷰/토글)에서 루트가 바뀌어도 표시 동기화
  useEffect(() => {
    const h = () => setRootState(getRoot(section));
    window.addEventListener(WORKSPACE_EVENT, h);
    return () => window.removeEventListener(WORKSPACE_EVENT, h);
  }, [section]);

  useEffect(() => {
    let alive = true;
    void rootDisplayPath(section, root).then((p) => {
      if (alive) setPath(p);
    });
    return () => {
      alive = false;
    };
  }, [section, root]);

  const place = useCallback(() => {
    const r = triggerRef.current?.getBoundingClientRect();
    if (r) setPos({ top: r.bottom + 6, left: r.left });
  }, []);

  useEffect(() => {
    if (!open) return;
    place();
    const onDown = (e: MouseEvent) => {
      const t = e.target as Node;
      if (triggerRef.current?.contains(t) || menuRef.current?.contains(t)) return;
      setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open, place]);

  function choose(r: string) {
    setOpen(false);
    if (r !== root) setRoot(section, r);
  }

  async function pickFolder() {
    setOpen(false);
    const dir = await openDialog({
      directory: true,
      multiple: false,
      title: t("settings.root.openDialogTitle"),
    });
    if (typeof dir === "string" && dir) setRoot(section, dir);
  }

  const recents = getRecentRoots(section).filter((r) => r !== root);

  return (
    <>
      <button
        ref={triggerRef}
        className="root-picker"
        onClick={() => setOpen((v) => !v)}
        title={isDefaultRoot(section, root) ? t("settings.root.defaultTitle") : root}
      >
        <Icon name="folder-open" size={14} />
        <span className="root-picker-name">
          {/* 기본 보관함 라벨은 언어를 따라간다 — 커스텀 루트는 폴더 이름 그대로 */}
          {isDefaultRoot(section, root)
            ? t("settings.root.default")
            : rootDisplayName(section, root)}
        </span>
        {path && <span className="root-picker-path">{path}</span>}
        <svg className="select-caret" width="10" height="6" viewBox="0 0 10 6">
          <path
            d="M1 1l4 4 4-4"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </button>
      {open &&
        pos &&
        createPortal(
          <div
            ref={menuRef}
            className="select-menu root-menu"
            style={{ top: pos.top, left: pos.left }}
          >
            {!isDefaultRoot(section, root) && (
              <button
                className="select-item"
                onClick={() => choose(DEFAULT_ROOTS[section])}
              >
                <span className="select-check" />
                <span className="root-menu-item">
                  <span>{t("settings.root.default")}</span>
                  <span className="root-menu-path">{t("settings.root.appData")}</span>
                </span>
              </button>
            )}
            {recents.map((r) => (
              <button key={r} className="select-item" onClick={() => choose(r)}>
                <span className="select-check" />
                <span className="root-menu-item">
                  <span>{r.split("/").filter(Boolean).pop()}</span>
                  <span className="root-menu-path">{r}</span>
                </span>
              </button>
            ))}
            {(recents.length > 0 || !isDefaultRoot(section, root)) && (
              <div className="root-menu-divider" />
            )}
            <button className="select-item" onClick={() => void pickFolder()}>
              <span className="select-check" />
              <span className="root-menu-item">
                <span>{t("settings.root.openFolder")}</span>
                <span className="root-menu-path">
                  {t("settings.root.openFolderDesc")}
                </span>
              </span>
            </button>
          </div>,
          document.body,
        )}
    </>
  );
}
