// 백업 브리지 — 고른 폴더에 vault 사본 + DB 스냅샷을 만든다.
// 파일 복사와 스냅샷(VACUUM INTO)은 전부 Rust 몫: amber.db 는 WAL 모드라
// 프론트에서 파일을 그냥 복사하면 최근 쓰기를 놓친 반쪽짜리 사본이 된다.

import { invoke } from "@tauri-apps/api/core";

/** `<destDir>/amber-backup-<타임스탬프>/` 를 만들고 그 절대경로를 돌려준다.
 *  실패 시 사용자에게 보여줄 한국어 메시지로 reject. */
export function createBackup(destDir: string): Promise<string> {
  return invoke<string>("create_backup", { destDir });
}
