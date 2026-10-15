// 노트 머리의 경로 줄 — 누르면 지금 노트 파일의 절대 경로를 복사한다(⌥⌘C 도 같다).
// 탐색기 머리의 경로 복사(RootPicker 의 .root-path)와 같은 문법이다: 툴팁에 복사될 경로를 먼저 보이고,
// 누르면 끝의 복사 표식이 1.5초 체크로 바뀐다(따로 알림을 띄우지 않는다).

import { useEffect, useRef, useState } from "react";
import { Icon } from "../icons";
import { t } from "../lib/i18n";
import { getRoot, rootDisplayName, rootPaths } from "../lib/workspace";
import { Tooltip } from "../ui";

export function NoteCrumb({
  path,
  active,
}: {
  /** 루트 기준 노트 경로(`TIL/이름.md`) */
  path: string;
  /** 노트 섹션이 화면에 있는가 — 안 보이는 화면이 단축키를 가로채지 않게 */
  active: boolean;
}) {
  const root = getRoot("notes");
  // 루트의 경로 — 표시는 ~ 축약, 복사는 절대경로 (비동기 해석)
  const [rootAt, setRootAt] = useState<{ abs: string; display: string } | null>(null);
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let alive = true;
    void rootPaths("notes", root).then((p) => {
      if (alive) setRootAt(p);
    });
    return () => {
      alive = false;
    };
  }, [root]);

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const under = (base: string) => `${base.replace(/\/+$/, "")}/${path}`;

  // 붙여넣는 쪽(터미널, Claude Code)이 확실하도록 ~ 축약이 아닌 절대경로를 넣는다
  async function copy() {
    if (!rootAt) return;
    try {
      await navigator.clipboard.writeText(under(rootAt.abs));
      setCopied(true);
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 1500);
    } catch {
      /* 클립보드 실패는 조용히 무시 */
    }
  }

  // ⌥⌘C — VS Code 의 Copy Path 와 같은 키. ⌥ 가 e.key 를 다른 글자로 바꾸므로 물리 키(code)로 본다
  const copyRef = useRef(copy);
  copyRef.current = copy;
  useEffect(() => {
    if (!active) return;
    const h = (e: KeyboardEvent) => {
      if (e.metaKey && e.altKey && !e.ctrlKey && !e.shiftKey && e.code === "KeyC") {
        e.preventDefault();
        void copyRef.current();
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [active]);

  const dirs = path.split("/").slice(0, -1);
  return (
    <div className="note-crumb">
      <Tooltip
        label={
          copied
            ? t("notes.copied")
            : t("notes.crumb.copyTip", { path: rootAt ? under(rootAt.display) : path })
        }
      >
        <button
          className={`note-crumb-btn${copied ? " copied" : ""}`}
          aria-label={t("settings.root.copyPath")}
          onClick={() => void copy()}
        >
          <Icon name="folder" size={12} />
          {[rootDisplayName("notes"), ...dirs].join(" › ")}
          <span className="note-crumb-copy">
            <Icon name={copied ? "check" : "copy"} size={11} />
          </span>
        </button>
      </Tooltip>
    </div>
  );
}
