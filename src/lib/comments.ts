// 필기노트 인라인 질문(노션 댓글식) 저장 계층.
// 노트 본문(.md)을 오염시키지 않도록 같은 폴더의 사이드카 JSON 에 저장한다:
//   vault/notes/<경로>/<이름>.md  ↔  <이름>.comments.json
// 트리는 .md 만 노출하므로 사이드카는 목록에 뜨지 않는다. 이름변경/삭제 시 동행은 notes.ts 가 처리.

import {
  BaseDirectory,
  exists,
  readTextFile,
  remove,
  writeTextFile,
} from "@tauri-apps/plugin-fs";
import { getRoot } from "./workspace";

const BASE = BaseDirectory.AppData;

/** 첫 문답 이후 이어지는 후속 문답 한 턴 */
export interface AskTurn {
  question: string;
  answer: string;
  createdAt: number; // UTC ms
  model?: string;
}

export interface NoteComment {
  id: string;
  /** 렌더된 본문 textContent 공간에서 드래그한 원문 */
  anchor: string;
  /** 같은 텍스트가 여러 번 나올 때 몇 번째인지 (0-based) */
  occurrence: number;
  question: string;
  answer: string;
  createdAt: number; // UTC ms
  model?: string;
  /** 후속 티키타카 (v1 파일엔 없음 — 없으면 첫 문답 하나짜리 스레드) */
  followUps?: AskTurn[];
}

/** 노트 상대경로 → 사이드카 상대경로 */
export function commentsPathFor(noteRel: string): string {
  return noteRel.replace(/\.md$/i, "") + ".comments.json";
}

// 사이드카는 항상 노트와 같은 워크스페이스 루트에 놓인다 (폴더 열기 시에도 동행)
const full = (rel: string) => `${getRoot("notes")}/${rel}`;

export function newCommentId(): string {
  try {
    return crypto.randomUUID();
  } catch {
    return `c_${Date.now()}_${Math.floor(Math.random() * 1e6)}`;
  }
}

/** 노트의 질문 목록 (없거나 깨졌으면 빈 배열) */
export async function loadComments(noteRel: string): Promise<NoteComment[]> {
  try {
    const p = full(commentsPathFor(noteRel));
    if (!(await exists(p, { baseDir: BASE }))) return [];
    const d = JSON.parse(await readTextFile(p, { baseDir: BASE }));
    if (!Array.isArray(d?.comments)) return [];
    const valid = d.comments.filter(
      (c: unknown): c is NoteComment =>
        !!c &&
        typeof (c as NoteComment).anchor === "string" &&
        typeof (c as NoteComment).question === "string",
    );
    // 손으로 편집됐을 수 있는 사이드카라 후속 문답도 형태를 검증해 걸러낸다
    for (const c of valid) {
      if (c.followUps !== undefined && !Array.isArray(c.followUps)) {
        delete c.followUps;
      } else if (c.followUps) {
        c.followUps = c.followUps.filter(
          (t: unknown): t is AskTurn =>
            !!t &&
            typeof (t as AskTurn).question === "string" &&
            typeof (t as AskTurn).answer === "string",
        );
      }
    }
    return valid;
  } catch {
    return [];
  }
}

/** 저장. 목록이 비면 사이드카 파일 자체를 지워 잔여물을 남기지 않는다 */
export async function saveComments(
  noteRel: string,
  comments: NoteComment[],
): Promise<void> {
  const p = full(commentsPathFor(noteRel));
  if (comments.length === 0) {
    if (await exists(p, { baseDir: BASE })) await remove(p, { baseDir: BASE });
    return;
  }
  await writeTextFile(p, JSON.stringify({ version: 1, comments }, null, 1), {
    baseDir: BASE,
  });
}
