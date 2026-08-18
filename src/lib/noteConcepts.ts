// 노트 → 개념 승격 링크(노트 쪽) 저장 계층.
// 노트 본문(.md)을 오염시키지 않도록 같은 폴더의 사이드카 JSON 에 목록으로 저장한다:
//   <루트>/<경로>/<이름>.md  ↔  <이름>.concepts.json
// 개념(amber.db)이 정본이고, 이 사이드카는 "이 노트에서 어떤 개념을 만들었나"의 역참조 목록.
// 트리는 .md 만 노출하므로 사이드카는 목록에 안 뜬다. 이름변경/삭제 동행은 notes.ts 가 처리.

import {
  BaseDirectory,
  exists,
  readTextFile,
  remove,
} from "@tauri-apps/plugin-fs";
import { writeAtomic } from "./vaultTree";
import { getRoot } from "./workspace";

const BASE = BaseDirectory.AppData;

export interface NoteConceptLink {
  conceptId: number;
  title: string;
  /** 승격 시점 선택 텍스트(표시·추적용) */
  anchor: string;
  createdAt: number; // UTC ms
}

/** 노트 상대경로 → 사이드카 상대경로 */
export function conceptsPathFor(noteRel: string): string {
  return noteRel.replace(/\.md$/i, "") + ".concepts.json";
}

const full = (rel: string) => `${getRoot("notes")}/${rel}`;

/** 노트에서 만든 개념 링크 목록 (없거나 깨졌으면 빈 배열) */
export async function loadNoteConcepts(noteRel: string): Promise<NoteConceptLink[]> {
  try {
    const p = full(conceptsPathFor(noteRel));
    if (!(await exists(p, { baseDir: BASE }))) return [];
    const d = JSON.parse(await readTextFile(p, { baseDir: BASE }));
    if (!Array.isArray(d?.links)) return [];
    return d.links.filter(
      (l: unknown): l is NoteConceptLink =>
        !!l && typeof (l as NoteConceptLink).conceptId === "number",
    );
  } catch {
    return [];
  }
}

async function saveLinks(noteRel: string, links: NoteConceptLink[]): Promise<void> {
  const p = full(conceptsPathFor(noteRel));
  if (links.length === 0) {
    if (await exists(p, { baseDir: BASE })) await remove(p, { baseDir: BASE });
    return;
  }
  await writeAtomic(p, JSON.stringify({ version: 1, links }, null, 1));
}

/** 승격 링크 추가 (같은 conceptId 는 갱신) */
export async function addNoteConcept(
  noteRel: string,
  link: NoteConceptLink,
): Promise<void> {
  const list = await loadNoteConcepts(noteRel);
  const next = [...list.filter((l) => l.conceptId !== link.conceptId), link];
  await saveLinks(noteRel, next);
}

/** 개념 삭제/링크 해제 시 노트 쪽에서도 제거 */
export async function removeNoteConcept(
  noteRel: string,
  conceptId: number,
): Promise<void> {
  const list = await loadNoteConcepts(noteRel);
  await saveLinks(
    noteRel,
    list.filter((l) => l.conceptId !== conceptId),
  );
}
