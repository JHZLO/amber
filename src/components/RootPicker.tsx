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
  rootPaths,
  setRoot,
  WORKSPACE_EVENT,
  type SectionKey,
} from "../lib/workspace";
import { t } from "../lib/i18n";
import { Icon } from "../icons";
import { Tooltip } from "../ui";

/** LEFT-TO-RIGHT MARK — 아래 `.root-path-text` 주석 참고 */
const LRM = "‎";

export function RootPicker({ section }: { section: SectionKey }) {
  const [root, setRootState] = useState(() => getRoot(section));
  // 현재 루트의 경로 — 표시는 ~ 축약, 복사는 절대경로 (비동기 해석)
  const [paths, setPaths] = useState<{ abs: string; display: string } | null>(null);
  const [copied, setCopied] = useState(false);
  const copyTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
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
    void rootPaths(section, root).then((p) => {
      if (alive) setPaths(p);
    });
    return () => {
      alive = false;
    };
  }, [section, root]);

  useEffect(
    () => () => {
      if (copyTimer.current) clearTimeout(copyTimer.current);
    },
    [],
  );

  // 경로 복사 — 붙여넣는 쪽(터미널/Finder)이 확실하도록 ~ 축약이 아닌 절대경로를 넣는다
  async function copyPath() {
    if (!paths) return;
    try {
      await navigator.clipboard.writeText(paths.abs);
      setCopied(true);
      if (copyTimer.current) clearTimeout(copyTimer.current);
      copyTimer.current = setTimeout(() => setCopied(false), 1500);
    } catch {
      /* 클립보드 실패는 조용히 무시 */
    }
  }

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
      {paths && (
        <Tooltip label={t(copied ? "settings.root.copied" : "settings.root.copyPath")}>
          <button
            className="root-path"
            aria-label={t("settings.root.copyPath")}
            onClick={() => void copyPath()}
          >
            {/* U+200E(LRM): 경로가 `~` 로 시작하면 rtl 문맥(왼쪽 말줄임)에서 bidi 가 그 중립문자를
                줄 끝으로 밀어 `…notes/~` 로 읽힌다. 문자열을 LTR 로 못 박아 순서를 지킨다. */}
            <span className="root-path-text">{LRM + paths.display}</span>
            <Icon name={copied ? "check" : "copy"} size={11} />
          </button>
        </Tooltip>
      )}
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
