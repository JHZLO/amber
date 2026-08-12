// 섹션별 작업 폴더(루트) — IDE 의 "폴더 열기" 모델.
// 값은 절대경로("/Users/…/rust-notes") 또는 appdata 상대경로(기본 보관함 "vault/notes").
// fs 플러그인은 절대경로가 오면 baseDir 를 무시하므로(PathBuf::push 의미론) 두 형태가 공존 가능.
// localStorage 에 영속, 변경 시 이벤트를 쏘아 열려 있는 뷰가 리로드하게 한다.

import { appDataDir, homeDir, join } from "@tauri-apps/api/path";
import { t } from "./i18n";

export const DEFAULT_ROOTS = {
  notes: "vault/notes",
  diagrams: "vault/diagrams",
} as const;

export type SectionKey = keyof typeof DEFAULT_ROOTS;

export const WORKSPACE_EVENT = "amber-workspace-change";

const rootKey = (s: SectionKey) => `amber.root.${s}`;
const recentKey = (s: SectionKey) => `amber.recent-roots.${s}`;

export function getRoot(s: SectionKey): string {
  return localStorage.getItem(rootKey(s)) || DEFAULT_ROOTS[s];
}

export function isDefaultRoot(s: SectionKey, root = getRoot(s)): boolean {
  return root === DEFAULT_ROOTS[s];
}

/** 트리 헤더 등에 보일 짧은 이름 (사용자 폴더명은 그대로, 기본 보관함만 번역) */
export function rootDisplayName(s: SectionKey, root = getRoot(s)): string {
  if (root === DEFAULT_ROOTS[s]) return t("settings.root.default");
  return root.split("/").filter(Boolean).pop() || root;
}

/** 루트의 절대경로를 ~ 축약 표기로 (트리 헤더의 보조 경로) — 기본 보관함은 appdata 상대라 풀어준다 */
export async function rootDisplayPath(s: SectionKey, root = getRoot(s)): Promise<string> {
  const abs = isDefaultRoot(s, root) ? await join(await appDataDir(), root) : root;
  const home = (await homeDir()).replace(/\/+$/, "");
  return abs === home || abs.startsWith(home + "/") ? `~${abs.slice(home.length)}` : abs;
}

/** 최근 연 폴더 목록 (절대경로, 최신순, 기본 보관함 제외) */
export function getRecentRoots(s: SectionKey): string[] {
  try {
    const arr = JSON.parse(localStorage.getItem(recentKey(s)) ?? "[]");
    return Array.isArray(arr) ? arr.filter((v) => typeof v === "string") : [];
  } catch {
    return [];
  }
}

function addRecent(s: SectionKey, root: string) {
  if (root === DEFAULT_ROOTS[s]) return;
  const next = [root, ...getRecentRoots(s).filter((r) => r !== root)].slice(0, 6);
  localStorage.setItem(recentKey(s), JSON.stringify(next));
}

/** 루트 변경 + 최근 목록 갱신 + 변경 이벤트 발행 */
export function setRoot(s: SectionKey, root: string) {
  if (root === DEFAULT_ROOTS[s]) localStorage.removeItem(rootKey(s));
  else localStorage.setItem(rootKey(s), root);
  addRecent(s, root);
  window.dispatchEvent(new CustomEvent(WORKSPACE_EVENT, { detail: s }));
}
