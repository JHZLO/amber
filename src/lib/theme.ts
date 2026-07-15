// 앱 테마(라이트/다크/시스템). localStorage 에 보관하고 <html data-theme> 로 적용.
// 값은 CSS 변수로 :root / :root[data-theme="dark"] 에서 정의된다.

export type ThemePref = "system" | "light" | "dark";

const KEY = "til.theme";
const darkMq = () => window.matchMedia("(prefers-color-scheme: dark)");

export function getThemePref(): ThemePref {
  const v = localStorage.getItem(KEY);
  return v === "light" || v === "dark" ? v : "system";
}

function resolved(pref: ThemePref): "light" | "dark" {
  if (pref === "system") return darkMq().matches ? "dark" : "light";
  return pref;
}

export const THEME_EVENT = "til-theme-change";

function apply(pref: ThemePref) {
  document.documentElement.dataset.theme = resolved(pref);
  window.dispatchEvent(new CustomEvent(THEME_EVENT));
}

export function setThemePref(pref: ThemePref) {
  if (pref === "system") localStorage.removeItem(KEY);
  else localStorage.setItem(KEY, pref);
  apply(pref);
}

/** 현재 실제 적용된 테마(system 은 해석된 값) */
export function resolvedTheme(): "light" | "dark" {
  return document.documentElement.dataset.theme === "dark" ? "dark" : "light";
}

/** 라이트↔다크 원클릭 토글 (명시적 pref 로 고정) */
export function toggleTheme() {
  setThemePref(resolvedTheme() === "dark" ? "light" : "dark");
}

/** 앱 시작 시 1회. 초기 적용 + 시스템/다른 창 변경 동기화 */
export function initTheme() {
  apply(getThemePref());
  // pref=system 일 때 OS 다크모드 토글 반영
  darkMq().addEventListener("change", () => {
    if (getThemePref() === "system") apply("system");
  });
  // 다른 창(위젯↔메인)에서 바꾼 테마 실시간 반영
  window.addEventListener("storage", (e) => {
    if (e.key === KEY) apply(getThemePref());
  });
}
