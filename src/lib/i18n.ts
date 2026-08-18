// UI 다국어 — 사용자 노출 문자열은 전부 t(key) 로 찾는다 (.claude/DESIGN.md §13).
//
// 언어는 localStorage 에 저장하고 **페이지 로드 시 한 번 고정**한다. 바꾸면 화면을 다시
// 불러온다(설정에서 reload) — 모듈 상수로 라벨 맵을 만드는 코드가 많아, 리액티브 재렌더보다
// 리로드가 단순하고 안전하다. 위젯 창은 LANG_CHANGED_EVENT 를 받아 스스로 reload 한다.
//
// 사전은 도메인별 파일(src/lib/messages/*)로 나뉜다 — 키는 "도메인.이름" 으로 접두어를 붙여
// 충돌을 막고, 각 파일의 en 은 Record<keyof typeof ko, string> 타입이라 번역 누락이 컴파일
// 에러로 잡힌다.

import { commonMessages } from "./messages/common";
import { appMessages } from "./messages/app";
import { conceptsMessages } from "./messages/concepts";
import { notesMessages } from "./messages/notes";
import { diagramsMessages } from "./messages/diagrams";
import { todosMessages } from "./messages/todos";
import { reportMessages } from "./messages/report";
import { settingsMessages } from "./messages/settings";

export type Lang = "ko" | "en";

const KEY = "amber.lang";

/** 언어 변경을 다른 창(위젯)에 알리는 Tauri 이벤트 — 받으면 location.reload() */
export const LANG_CHANGED_EVENT = "lang-changed";

export function getLang(): Lang {
  // vitest(node) 엔 localStorage 가 없다 — 라이브러리 기본은 ko (기존 테스트 기준)
  if (typeof localStorage === "undefined") return "ko";
  const s = localStorage.getItem(KEY);
  if (s === "ko" || s === "en") return s;
  // 미설정이면 시스템 언어를 따른다
  return navigator.language?.toLowerCase().startsWith("ko") ? "ko" : "en";
}

/** 저장만 한다 — 적용(reload·위젯 통지)은 호출부(설정) 몫 */
export function setLang(l: Lang): void {
  localStorage.setItem(KEY, l);
}

const KO = {
  ...commonMessages.ko,
  ...appMessages.ko,
  ...conceptsMessages.ko,
  ...notesMessages.ko,
  ...diagramsMessages.ko,
  ...todosMessages.ko,
  ...reportMessages.ko,
  ...settingsMessages.ko,
};
const EN: Record<MsgKey, string> = {
  ...commonMessages.en,
  ...appMessages.en,
  ...conceptsMessages.en,
  ...notesMessages.en,
  ...diagramsMessages.en,
  ...todosMessages.en,
  ...reportMessages.en,
  ...settingsMessages.en,
};

export type MsgKey = keyof typeof KO;

// 페이지 로드 시 고정 — 이후 setLang 은 다음 로드에 반영된다
const LANG: Lang = getLang();

// index.html 의 lang="en" 은 스캐폴드 기본값이다. 여기서 해석된 언어로 덮어써야
// 웹뷰의 하이픈 처리·스크린리더 발음이 실제 콘텐츠 언어를 따른다.
// 언어 변경은 창을 리로드하므로(SettingsModal) 반응형으로 만들 필요는 없다.
// i18n 은 순수 lib(date·vaultTree 등)도 import 하고 그쪽 테스트는 node 환경에서 돌아 DOM 이 없다.
if (typeof document !== "undefined") document.documentElement.lang = LANG;

/** 현재 언어의 문자열. vars 는 "{name}" 자리 치환 */
export function t(key: MsgKey, vars?: Record<string, string | number>): string {
  let s: string = (LANG === "en" ? EN[key] : KO[key]) ?? key;
  if (vars)
    for (const [k, v] of Object.entries(vars)) s = s.split(`{${k}}`).join(String(v));
  return s;
}

/** Intl/toLocaleDateString 계열에 넘길 로케일 */
export function dateLocale(): string {
  return LANG === "ko" ? "ko-KR" : "en-US";
}
