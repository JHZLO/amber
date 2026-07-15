// vault: 상세 정리본(.md)의 정본 저장소.
// 물리 위치: $APPDATA/dev.jhzlo.til/vault/concepts/<ulid>/index.md
// DB 의 detail_path 는 vault 기준 상대경로('concepts/<ulid>/index.md')만 저장한다.
// frontmatter 는 id(=ulid) 만 둔다. status/confidence/seen 같은 휘발성 메타는 절대 파일에 안 넣음
// → 위젯 조작이 파일/ git diff 를 오염시키지 않게 (PRD §7.3).

import {
  BaseDirectory,
  mkdir,
  writeTextFile,
  readTextFile,
  exists,
} from "@tauri-apps/plugin-fs";
import { invoke } from "@tauri-apps/api/core";

const BASE = BaseDirectory.AppData;
const VAULT = "vault";

/** DB 에 저장할 상대경로 */
export function detailPathFor(ulid: string): string {
  return `concepts/${ulid}/index.md`;
}

function conceptDir(ulid: string): string {
  return `${VAULT}/concepts/${ulid}`;
}

function fullPath(detailPath: string): string {
  return `${VAULT}/${detailPath}`;
}

function buildFrontmatter(ulid: string, body: string): string {
  return `---\nid: ${ulid}\n---\n\n${body.trimStart()}`;
}

/** frontmatter 를 떼어내고 본문만 반환 */
function stripFrontmatter(raw: string): string {
  if (!raw.startsWith("---")) return raw;
  const end = raw.indexOf("\n---", 3);
  if (end === -1) return raw;
  // '\n---' 다음 줄바꿈까지 스킵
  const after = raw.indexOf("\n", end + 1);
  return after === -1 ? "" : raw.slice(after + 1).replace(/^\n+/, "");
}

/** 상세 노트 저장 (없으면 디렉터리 생성). 저장 후 상대경로 반환 */
export async function writeNote(ulid: string, body: string): Promise<string> {
  await mkdir(conceptDir(ulid), { baseDir: BASE, recursive: true });
  const detailPath = detailPathFor(ulid);
  await writeTextFile(fullPath(detailPath), buildFrontmatter(ulid, body), {
    baseDir: BASE,
  });
  return detailPath;
}

/** 상세 노트 본문 읽기 (frontmatter 제외) */
export async function readNote(detailPath: string): Promise<string> {
  const raw = await readTextFile(fullPath(detailPath), { baseDir: BASE });
  return stripFrontmatter(raw);
}

/** 개념 디렉터리 통째로 삭제 (assets 포함). 영구 삭제 대신 macOS 휴지통으로 이동 → 복구 가능 */
export async function deleteConceptDir(ulid: string): Promise<void> {
  const dir = conceptDir(ulid);
  if (await exists(dir, { baseDir: BASE })) {
    await invoke("move_to_trash", { relPath: dir });
  }
}
