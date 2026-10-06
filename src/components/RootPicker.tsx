// 트리 헤더의 워크스페이스 루트 전환기 — VS Code 의 "폴더 열기 / Open Recent" 대응.
// 현재 루트 이름을 보여주고, 클릭하면 최근 폴더 목록 + 기본 보관함 + 폴더 열기 메뉴.

import { useEffect, useRef, useState } from "react";
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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

/** LEFT-TO-RIGHT MARK — 아래 `.root-path-text` 주석 참고 */
const LRM = "‎";

export function RootPicker({ section }: { section: SectionKey }) {
  const [root, setRootState] = useState(() => getRoot(section));
  // 현재 루트의 경로 — 표시는 ~ 축약, 복사는 절대경로 (비동기 해석)
  const [paths, setPaths] = useState<{ abs: string; display: string } | null>(null);
  const [copied, setCopied] = useState(false);
  const copyTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

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

  function choose(r: string) {
    if (r !== root) setRoot(section, r);
  }

  async function pickFolder() {
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
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            className="root-picker"
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
        </DropdownMenuTrigger>
        <DropdownMenuContent className="root-menu">
          {!isDefaultRoot(section, root) && (
            <DropdownMenuItem onSelect={() => choose(DEFAULT_ROOTS[section])}>
              <span className="select-check" />
              <span className="root-menu-item">
                <span>{t("settings.root.default")}</span>
                <span className="root-menu-path">{t("settings.root.appData")}</span>
              </span>
            </DropdownMenuItem>
          )}
          {recents.map((r) => (
            <DropdownMenuItem key={r} onSelect={() => choose(r)}>
              <span className="select-check" />
              <span className="root-menu-item">
                <span>{r.split("/").filter(Boolean).pop()}</span>
                <span className="root-menu-path">{r}</span>
              </span>
            </DropdownMenuItem>
          ))}
          {(recents.length > 0 || !isDefaultRoot(section, root)) && (
            <DropdownMenuSeparator className="root-menu-divider" />
          )}
          <DropdownMenuItem onSelect={() => void pickFolder()}>
            <span className="select-check" />
            <span className="root-menu-item">
              <span>{t("settings.root.openFolder")}</span>
              <span className="root-menu-path">{t("settings.root.openFolderDesc")}</span>
            </span>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
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
    </>
  );
}
