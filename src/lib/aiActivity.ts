// AI 실행 중 도구 호출(Activity) → 한 줄 문구. 참고 폴더를 훑는 몇 분짜리 실행에서
// "멈춘 게 아니라 읽고 있다"를 보이는 용도다. 도구 이름은 CLI 가 정한다(claude: Read/Glob/Grep,
// codex: command/search) — 그 밖의 도구는 이름을 그대로 보인다.

import type { AiActivity } from "./ai";
import { t } from "./i18n";

/** 긴 경로는 뒤쪽 두 마디만 — 파일명만 남기면 같은 이름이 흔하고(index.ts), 전체를 두면 한 줄을 넘친다 */
export function shortTarget(target: string, keep = 2): string {
  const s = target.trim();
  if (!s.includes("/")) return s.length > 60 ? `…${s.slice(-59)}` : s;
  const parts = s.replace(/\/+$/, "").split("/").filter(Boolean);
  const tail = parts.slice(-keep).join("/");
  return parts.length > keep ? `…/${tail}` : tail;
}

export function describeActivity(a: AiActivity): string {
  const target = a.target ? shortTarget(a.target) : "";
  // 도구 시작 신호는 대상보다 먼저 온다(입력이 아직 스트리밍 중) — 그때는 무엇을 하는지만 말한다
  if (!target && ["Read", "Glob", "Grep", "LS"].includes(a.tool)) return t("common.ai.activity.files");
  switch (a.tool) {
    case "Read":
      return t("common.ai.activity.read", { target });
    case "Glob":
    case "Grep":
    case "search":
      return t("common.ai.activity.search", { target });
    case "LS":
      return t("common.ai.activity.list", { target });
    case "command":
    case "Bash":
      return t("common.ai.activity.run", { target });
    default:
      return target ? `${a.tool} · ${target}` : t("common.ai.activity.tool", { tool: a.tool });
  }
}
