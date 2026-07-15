// 필기노트: $APPDATA/vault/notes/ 아래 실제 디렉토리 + .md 파일이 정본.
// 공용 vaultTree 계층을 마크다운 설정으로 감싼 것 (기존 export 이름 유지).
// 노트에는 인라인 질문 사이드카(<이름>.comments.json)가 붙을 수 있어,
// 이름변경/삭제 시 사이드카가 함께 따라가도록 여기서 감싼다.

import { BaseDirectory, exists, rename } from "@tauri-apps/plugin-fs";
import { invoke } from "@tauri-apps/api/core";
import { createVaultTree } from "./vaultTree";
import { commentsPathFor } from "./comments";

export {
  parentOf,
  invalidNameReason,
  invalidPathReason,
  normalizePath,
  flattenDirs,
} from "./vaultTree";
export type { VaultNode as NoteNode } from "./vaultTree";

const BASE = BaseDirectory.AppData;
const ROOT = "vault/notes";
const full = (rel: string) => `${ROOT}/${rel}`;

const tree = createVaultTree({
  root: ROOT,
  exts: [".md"],
  template: (title) => `# ${title}\n\n`,
});

export const listNoteTree = tree.listTree;
export const readNoteFile = tree.readFile;
export const writeNoteFile = tree.writeFile;
export const noteMtime = tree.fileMtime;
export const createFolder = tree.createFolder;
export const createNote = tree.createFile;

/** 이름 변경 — 노트 파일이면 질문 사이드카도 함께 이동 */
export async function renameEntry(
  relPath: string,
  newName: string,
  isDir: boolean,
): Promise<string> {
  const newRel = await tree.renameEntry(relPath, newName, isDir);
  if (!isDir) {
    const oldSc = full(commentsPathFor(relPath));
    if (await exists(oldSc, { baseDir: BASE })) {
      await rename(oldSc, full(commentsPathFor(newRel)), {
        oldPathBaseDir: BASE,
        newPathBaseDir: BASE,
      });
    }
  }
  return newRel;
}

/** 삭제(휴지통) — 노트 파일이면 질문 사이드카도 함께 (없으면 멱등 성공) */
export async function deleteEntry(relPath: string): Promise<void> {
  await tree.deleteEntry(relPath);
  if (/\.md$/i.test(relPath)) {
    await invoke("move_to_trash", { relPath: full(commentsPathFor(relPath)) });
  }
}
