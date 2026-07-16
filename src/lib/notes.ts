// 필기노트: 현재 워크스페이스 루트(기본 = $APPDATA/vault/notes, "폴더 열기"로 임의 로컬 폴더)
// 아래 실제 디렉토리 + .md 파일이 정본. 공용 vaultTree 계층을 마크다운 설정으로 감싼 것.
// 노트에는 인라인 질문 사이드카(<이름>.comments.json)가 붙을 수 있어,
// 이름변경/삭제 시 사이드카가 함께 따라가도록 여기서 감싼다.

import { BaseDirectory, exists, rename } from "@tauri-apps/plugin-fs";
import { invoke } from "@tauri-apps/api/core";
import { createVaultTree } from "./vaultTree";
import { commentsPathFor } from "./comments";
import { conceptsPathFor } from "./noteConcepts";
import { getRoot } from "./workspace";

export {
  parentOf,
  invalidNameReason,
  invalidPathReason,
  normalizePath,
  flattenDirs,
} from "./vaultTree";
export type { VaultNode as NoteNode } from "./vaultTree";

const BASE = BaseDirectory.AppData;
const full = (rel: string) => `${getRoot("notes")}/${rel}`;

const tree = createVaultTree({
  root: () => getRoot("notes"),
  exts: [".md"],
  template: (title) => `# ${title}\n\n`,
});

export const listNoteTree = tree.listTree;
export const readNoteFile = tree.readFile;
export const writeNoteFile = tree.writeFile;
export const noteMtime = tree.fileMtime;
export const createFolder = tree.createFolder;
export const createNote = tree.createFile;

// 노트에 딸린 사이드카들(질문·개념 링크) — 이름변경/삭제 시 함께 따라간다
const sidecarsFor = (rel: string) => [
  commentsPathFor(rel),
  conceptsPathFor(rel),
];

/** 이름 변경 — 노트 파일이면 사이드카(질문/개념링크)도 함께 이동 */
export async function renameEntry(
  relPath: string,
  newName: string,
  isDir: boolean,
): Promise<string> {
  const newRel = await tree.renameEntry(relPath, newName, isDir);
  if (!isDir) {
    for (const sc of sidecarsFor(relPath)) {
      const oldSc = full(sc);
      if (await exists(oldSc, { baseDir: BASE })) {
        await rename(oldSc, full(conceptsOrCommentsTarget(sc, relPath, newRel)), {
          oldPathBaseDir: BASE,
          newPathBaseDir: BASE,
        });
      }
    }
  }
  return newRel;
}

// 사이드카 상대경로를 새 노트 이름 기준으로 재매핑 (comments/concepts 각각)
function conceptsOrCommentsTarget(sc: string, oldRel: string, newRel: string): string {
  return sc === commentsPathFor(oldRel)
    ? commentsPathFor(newRel)
    : conceptsPathFor(newRel);
}

/** 삭제(휴지통) — 노트 파일이면 사이드카(질문/개념링크)도 함께 (없으면 멱등 성공) */
export async function deleteEntry(relPath: string): Promise<void> {
  await tree.deleteEntry(relPath);
  if (/\.md$/i.test(relPath)) {
    for (const sc of sidecarsFor(relPath)) {
      await invoke("move_to_trash", { relPath: full(sc) });
    }
  }
}
