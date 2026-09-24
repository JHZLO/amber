// 필기노트: 현재 워크스페이스 루트(기본 = $APPDATA/vault/notes, "폴더 열기"로 임의 로컬 폴더)
// 아래 실제 디렉토리 + .md 파일이 정본. 공용 vaultTree 계층을 마크다운 설정으로 감싼 것.
// 노트에는 인라인 질문 사이드카(<이름>.comments.json)가 붙을 수 있어,
// 이름변경/삭제 시 사이드카가 함께 따라가도록 여기서 감싼다.

import { BaseDirectory, exists, readTextFile, rename } from "@tauri-apps/plugin-fs";
import { invoke } from "@tauri-apps/api/core";
import { createVaultTree, parentOf } from "./vaultTree";
import { ASSET_DIR, copyAssetsForMove } from "./noteAssets";
import { commentsPathFor } from "./comments";
import { conceptsPathFor } from "./noteConcepts";
import { getRoot } from "./workspace";
import { repointConceptSource } from "./db";

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
  // 붙여 넣은 이미지 폴더 — 노트 옆에 있지만 트리에는 안 보인다(lib/noteAssets.ts)
  hiddenDirs: [ASSET_DIR],
});

/** 노트가 들어 있는 폴더의 경로(워크스페이스 루트 포함) — 이미지 저장/해석의 기준 */
export function noteDirPath(noteRel: string): string {
  const parent = parentOf(noteRel);
  return parent ? full(parent) : getRoot("notes");
}

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
  // 개념 → 노트 역참조(source.noteRel)는 DB 에 있어 파일 이동만으로는 따라오지 않는다.
  // 놔두면 "출처 노트 열기"가 죽은 경로를 연다.
  await repointConceptSource(relPath, newRel, isDir).catch(() => {});
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

/** 다른 폴더로 이동 — 노트 파일이면 사이드카(질문/개념링크)도 함께.
 *  폴더 이동 시 사이드카는 폴더 하위 파일이라 디렉터리째 함께 이동한다(별도 처리 불필요). */
export async function moveEntry(
  relPath: string,
  targetDir: string,
): Promise<string> {
  const newRel = await tree.moveEntry(relPath, targetDir);
  await repointConceptSource(relPath, newRel, !/\.md$/i.test(relPath)).catch(() => {});
  if (/\.md$/i.test(relPath)) {
    // 본문의 `![](_assets/…)` 는 노트 폴더 기준 상대경로다. 폴더를 옮기면 _assets 가 따라가지만
    // 노트 하나만 옮기면 그림이 옛 폴더에 남아 깨진다 — 가리키는 파일만 새 폴더로 복사한다.
    // 실패해도 이동 자체는 이미 끝났으니 되돌리지 않는다(그림만 깨진 채로 남는다).
    try {
      const md = await readTextFile(full(newRel), { baseDir: BASE });
      await copyAssetsForMove(md, noteDirPath(relPath), noteDirPath(newRel));
    } catch {
      /* 그림 복사 실패가 이동을 실패로 만들지 않는다 */
    }
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

/** 삭제(휴지통) — 노트 파일이면 사이드카(질문/개념링크)도 함께 (없으면 멱등 성공) */
export async function deleteEntry(relPath: string): Promise<void> {
  await tree.deleteEntry(relPath);
  if (/\.md$/i.test(relPath)) {
    for (const sc of sidecarsFor(relPath)) {
      await invoke("move_to_trash", { relPath: full(sc) });
    }
  }
}
