// 열린 노트를 디스크와 맞춘다 — 다른 프로그램(에이전트, vim, git)이 파일을 고치면 노트를 다시 열지
// 않아도 화면에 반영된다. 감지는 두 겹이다: vault 폴더의 OS 파일 이벤트(FSEvents, 디바운스)가 주력이고,
// 창 포커스 복귀 때의 비교가 이벤트 유실(슬립, 감시 실패)을 메운다.
//
// 내용 비교가 기준이다. mtime 만 보면 앱이 방금 저장한 파일도 "바뀌었다"로 읽힌다 — 디스크 내용이
// 화면 본문과 같으면 그게 우리 저장이든 touch 든 할 일이 없다.

import { BaseDirectory, watch, type UnwatchFn, type WatchEvent } from "@tauri-apps/plugin-fs";
import { getRoot } from "./workspace";

/** 디스크에서 읽은 내용을 화면에 어떻게 반영할지 */
export type DiskSync =
  /** 화면과 같다 — 할 일 없음(우리 저장, touch, 다른 파일의 이벤트) */
  | "none"
  /** 읽기 모드 — 본문만 바꾼다 */
  | "reload"
  /** 편집 중이지만 고친 게 없다 — 초안까지 바꾸고 커서를 지킨다 */
  | "reload-draft"
  /** 저장 안 된 편집이 있다 — 덮지 않고 사용자에게 묻는다 */
  | "prompt";

export function decideDiskSync(s: {
  disk: string;
  body: string;
  editing: boolean;
  draft: string;
  /** 이미 물어본(또는 "내 것 유지"로 넘긴) 디스크 내용 — 같은 변경으로 배너를 다시 띄우지 않는다 */
  dismissed: string | null;
}): DiskSync {
  if (s.disk === s.body) return "none";
  if (!s.editing) return "reload";
  // 고친 게 없거나, 고친 결과가 마침 디스크와 같으면 잃을 것이 없다
  if (s.draft === s.body || s.draft === s.disk) return "reload-draft";
  if (s.disk === s.dismissed) return "none";
  return "prompt";
}

/** 트리나 열린 노트에 영향을 줄 수 있는 이벤트인가. 우리 원자적 저장의 임시 파일과 접근(읽기)
 *  이벤트는 버린다 — 읽기 이벤트까지 받으면 우리가 파일을 읽을 때마다 다시 읽는 고리가 생긴다. */
export function isRelevantEvent(e: Pick<WatchEvent, "type" | "paths">): boolean {
  if (typeof e.type === "object" && "access" in e.type) return false;
  if (!e.paths.length) return true;
  return e.paths.some((p) => !p.endsWith(".amber-tmp"));
}

/** textarea 내용을 바꿔도 커서와 스크롤이 제자리에 남게 위치를 잡아 둔다. 반환한 함수를 새 값이
 *  그려진 뒤에 부른다. 줄어든 글이면 끝으로 당긴다. */
export function keepCaret(el: HTMLTextAreaElement | null): (nextLen: number) => void {
  if (!el) return () => {};
  const { selectionStart: s, selectionEnd: e, scrollTop } = el;
  const focused = document.activeElement === el;
  return (nextLen) => {
    el.setSelectionRange(Math.min(s, nextLen), Math.min(e, nextLen));
    el.scrollTop = scrollTop;
    if (focused) el.focus();
  };
}

/** 노트 루트 폴더 전체를 감시한다. 원자적 저장은 임시 파일 생성과 rename 으로 이벤트가 여러 개
 *  오므로 250ms 동안 모아 한 번만 알린다. */
export function watchNotesRoot(onChange: () => void): Promise<UnwatchFn> {
  return watch(
    getRoot("notes"),
    (e) => {
      if (isRelevantEvent(e)) onChange();
    },
    // 루트가 절대경로(사용자가 연 폴더)면 baseDir 은 무시된다 — 다른 vault 함수와 같은 규약
    { baseDir: BaseDirectory.AppData, recursive: true, delayMs: 250 },
  );
}
