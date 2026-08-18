// 백업 브리지 — 고른 폴더에 vault 사본 + DB 스냅샷을 만든다.
// 파일 복사와 스냅샷(VACUUM INTO)은 전부 Rust 몫: amber.db 는 WAL 모드라
// 프론트에서 파일을 그냥 복사하면 최근 쓰기를 놓친 반쪽짜리 사본이 된다.

import { invoke } from "@tauri-apps/api/core";
import { DEFAULT_ROOTS, getRoot, isDefaultRoot, type SectionKey } from "./workspace";

/** 백업에 함께 담아야 할 커스텀 작업 폴더들. 기본 보관함은 이미 vault/ 로 복사되므로 제외. */
export function customRoots(): [SectionKey, string][] {
  return (Object.keys(DEFAULT_ROOTS) as SectionKey[])
    .filter((s) => !isDefaultRoot(s))
    .map((s) => [s, getRoot(s)]);
}

/** `<destDir>/amber-backup-<로컬 타임스탬프>/` 를 만들고 그 절대경로를 돌려준다.
 *  실패 시 사용자에게 보여줄 한국어 메시지로 reject. */
export function createBackup(destDir: string): Promise<string> {
  // 폴더명이 사용자 시계와 일치하게 — report 와 같은 getTimezoneOffset 부호 규약
  return invoke<string>("create_backup", {
    destDir,
    tzOffsetMin: new Date().getTimezoneOffset(),
    // "폴더 열기"로 바꾼 루트는 vault/ 밖에 있어 Rust 가 알 방법이 없다 — 여기서 넘겨야 백업에 담긴다
    extraRoots: customRoots(),
  });
}
