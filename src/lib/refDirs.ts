// AI 모달의 참고 폴더 — 최근 목록(localStorage)과 표시 이름. 폴더를 CLI 에 여는 건 Rust(ai.rs)가 한다.
//
// 왜 최근 목록인가: 같은 저장소를 두고 여러 번 글을 쓰게 되는데 매번 폴더 다이얼로그를 열면 첨부가
// 귀찮아 안 하게 된다. 최근 것은 칩으로 남겨 두고 체크만 하게 한다(RootPicker 의 최근 폴더와 같은 문법).

const KEY = "amber.ai.refDirs";
const MAX = 6;

/** 최근 목록 맨 앞에 끼운다(이미 있으면 앞으로 당김) — 순수 함수 */
export function pushRecent(list: readonly string[], dir: string, max = MAX): string[] {
  const d = dir.trim();
  if (!d) return [...list];
  return [d, ...list.filter((x) => x !== d)].slice(0, max);
}

/** 경로의 마지막 이름 — 칩 라벨. 전체 경로는 툴팁으로 */
export function refDirName(path: string): string {
  const parts = path.replace(/\/+$/, "").split("/");
  return parts[parts.length - 1] || path;
}

export function loadRecentRefDirs(): string[] {
  try {
    const raw = localStorage.getItem(KEY);
    const arr: unknown = raw ? JSON.parse(raw) : [];
    return Array.isArray(arr)
      ? arr.filter((x): x is string => typeof x === "string" && x.length > 0).slice(0, MAX)
      : [];
  } catch {
    return [];
  }
}

/** 고른 폴더를 최근 목록에 남기고 새 목록을 돌려준다 */
export function rememberRefDir(dir: string): string[] {
  const next = pushRecent(loadRecentRefDirs(), dir);
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    // 저장에 실패해도 이번 요청에는 지장이 없다
  }
  return next;
}
