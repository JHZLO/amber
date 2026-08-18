// vault 하위 디렉터리 트리(실제 폴더 + 파일)를 다루는 공용 계층.
// 필기노트(vault/notes, .md)와 다이어그램(vault/diagrams, .mmd)이
// 루트/확장자/새 파일 템플릿만 달리해 같은 구현을 공유한다. DB 없음 — 파일시스템이 정본.

import {
  BaseDirectory,
  exists,
  mkdir,
  readDir,
  readTextFile,
  remove,
  rename,
  stat,
  writeTextFile,
} from "@tauri-apps/plugin-fs";
import { invoke } from "@tauri-apps/api/core";
import { t } from "./i18n";

const BASE = BaseDirectory.AppData;

/** 임시 파일에 쓰고 rename 으로 덮는다 — "파일이 정본"인 앱에서 저장이 파일을 잃는 유일한 지점을 막는다.
 *  writeTextFile 은 truncate 후 write 라, 그 사이에 전원이 끊기거나 디스크가 차면 0바이트/반쪽 파일이
 *  남고 이전 버전은 어디에도 없다(이 경로는 휴지통도 .bak 도 안 쓴다).
 *  같은 볼륨의 APFS rename 은 원자적이라 크래시 시 옛 파일 아니면 새 파일 중 하나가 온전히 남는다.
 *  fsync 는 넣지 않는다 — 이 보장에 필요 없고 저장마다 비용만 든다. */
export async function writeAtomic(path: string, content: string): Promise<void> {
  const tmp = `${path}.amber-tmp`;
  try {
    await writeTextFile(tmp, content, { baseDir: BASE });
    await rename(tmp, path, { oldPathBaseDir: BASE, newPathBaseDir: BASE });
  } catch (e) {
    // 실패한 임시 파일이 트리에 보이면 안 된다. 정리 실패는 원래 오류를 가리지 않게 삼킨다.
    try {
      if (await exists(tmp, { baseDir: BASE })) await remove(tmp, { baseDir: BASE });
    } catch {
      /* 원래 오류가 더 중요하다 */
    }
    throw e;
  }
}

export interface VaultNode {
  /** 표시명 (파일은 확장자 제거) */
  name: string;
  /** 루트 기준 상대경로 (파일은 확장자 포함) */
  path: string;
  isDir: boolean;
  children?: VaultNode[];
}

/** 상위 폴더의 상대경로 ('' = 루트) */
export function parentOf(rel: string): string {
  const i = rel.lastIndexOf("/");
  return i === -1 ? "" : rel.slice(0, i);
}

/** 단일 이름(경로 구분자 불가) 검증. 문제가 있으면 사유 문자열, 없으면 null */
export function invalidNameReason(name: string): string | null {
  const n = name.trim();
  if (!n) return t("common.name.empty");
  if (/[/\\:]/.test(n)) return t("common.name.badChars");
  if (n.startsWith(".")) return t("common.name.leadingDot");
  if (n.length > 80) return t("common.name.tooLong");
  return null;
}

/** 'CS/네트워크' 같은 다단계 경로 입력 검증 (구간별로 이름 규칙 적용) */
export function invalidPathReason(path: string): string | null {
  const segs = path.split("/").map((s) => s.trim()).filter(Boolean);
  if (!segs.length) return t("common.name.empty");
  for (const s of segs) {
    const r = invalidNameReason(s);
    if (r) return r;
  }
  return null;
}

/** 경로 입력 정규화: 구간 trim, 빈 구간(연속/양끝 슬래시) 제거 */
export function normalizePath(path: string): string {
  return path
    .split("/")
    .map((s) => s.trim())
    .filter(Boolean)
    .join("/");
}

/** 트리에서 폴더 경로만 평탄화 (이동/생성 위치 선택용). 루트('') 포함 */
export function flattenDirs(tree: VaultNode[]): string[] {
  const dirs: string[] = [""];
  const walk = (nodes: VaultNode[]) => {
    for (const n of nodes) {
      if (n.isDir) {
        dirs.push(n.path);
        if (n.children) walk(n.children);
      }
    }
  };
  walk(tree);
  return dirs;
}

/** 검색 결과 한 건 */
export interface VaultMatch {
  /** 표시명 (확장자 제거) */
  name: string;
  /** 루트 기준 상대경로 (확장자 포함) */
  path: string;
  /** 본문에서 처음 걸린 줄. 파일명만 일치했으면 null */
  snippet: string | null;
}

const SNIPPET_MAX = 120;

/** 파일명 + 본문 substring 검색 (대소문자 무시). 파일명 일치를 앞에 둔다.
 *  트리·읽기를 주입받는 이유: notes/diagrams 가 이미 각자 루트로 묶은 함수를 내보내고 있어
 *  루트·확장자 설정을 호출부에서 다시 쓰지 않아도 된다.
 *  수십 개 규모라 인덱스 없이 매번 전부 읽는다 — 호출부가 디바운스로 빈도를 줄인다. */
export async function searchFiles(
  src: {
    listTree: () => Promise<VaultNode[]>;
    readFile: (relPath: string) => Promise<string>;
  },
  query: string,
  limit = 20,
): Promise<VaultMatch[]> {
  const q = query.trim().toLowerCase();
  if (!q) return [];

  const files: VaultNode[] = [];
  const walk = (nodes: VaultNode[]) => {
    for (const n of nodes) {
      if (n.isDir) walk(n.children ?? []);
      else files.push(n);
    }
  };
  walk(await src.listTree());

  const byName: VaultMatch[] = [];
  const byBody: VaultMatch[] = [];
  for (const f of files) {
    let snippet: string | null = null;
    try {
      snippet = firstHitLine(await src.readFile(f.path), q);
    } catch {
      // 읽기 실패(삭제 직후·권한)는 결과에서 조용히 빼되 이름 일치는 살린다
    }
    if (f.name.toLowerCase().includes(q))
      byName.push({ name: f.name, path: f.path, snippet });
    else if (snippet) byBody.push({ name: f.name, path: f.path, snippet });
  }
  return [...byName, ...byBody].slice(0, limit);
}

/** 소문자 질의가 처음 걸린 줄. 길면 매칭 지점이 잘리지 않게 그 앞에서 자른다 */
function firstHitLine(body: string, q: string): string | null {
  for (const raw of body.split("\n")) {
    const line = raw.trim();
    const at = line.toLowerCase().indexOf(q);
    if (at === -1) continue;
    if (line.length <= SNIPPET_MAX) return line;
    const start = Math.max(0, at - 30);
    return (start ? "…" : "") + line.slice(start, start + SNIPPET_MAX) + "…";
  }
  return null;
}

export interface VaultTreeConfig {
  /** 루트 폴더. appdata 상대경로('vault/notes') 또는 절대경로('/Users/…').
   *  getter 를 주면 호출 시점마다 해석 — "폴더 열기"로 루트가 바뀌어도 인스턴스 재생성 불필요. */
  root: string | (() => string);
  /** 트리에 보일 파일 확장자(소문자, 점 포함). 첫 항목이 생성 시 기본 확장자 */
  exts: string[];
  /** 새 파일 초기 내용 (파일명 기반) */
  template: (title: string) => string;
}

export function createVaultTree(cfg: VaultTreeConfig) {
  const rootOf = typeof cfg.root === "function" ? cfg.root : () => cfg.root as string;
  const full = (rel: string) => (rel ? `${rootOf()}/${rel}` : rootOf());
  const mainExt = cfg.exts[0];

  const stripExt = (name: string) => {
    const low = name.toLowerCase();
    for (const e of cfg.exts) {
      if (low.endsWith(e)) return name.slice(0, -e.length);
    }
    return name;
  };
  const hasExt = (name: string) => {
    const low = name.toLowerCase();
    return cfg.exts.some((e) => low.endsWith(e));
  };

  async function ensureRoot(): Promise<void> {
    // 기본 보관함(appdata 상대)만 자동 생성. 사용자가 연 절대경로 폴더는 이미 존재한다고 가정
    // (사라졌으면 트리 로드가 에러를 내고 뷰가 안내).
    const root = rootOf();
    if (!root.startsWith("/")) {
      await mkdir(root, { baseDir: BASE, recursive: true });
    }
  }

  function sortNodes(nodes: VaultNode[]): VaultNode[] {
    return nodes.sort((a, b) => {
      if (a.isDir !== b.isDir) return a.isDir ? -1 : 1; // 폴더 먼저
      return a.name.localeCompare(b.name, "ko");
    });
  }

  async function readTree(rel: string): Promise<VaultNode[]> {
    const entries = await readDir(full(rel), { baseDir: BASE });
    const nodes: VaultNode[] = [];
    for (const e of entries) {
      if (e.name.startsWith(".")) continue; // .DS_Store 등
      const childRel = rel ? `${rel}/${e.name}` : e.name;
      if (e.isDirectory) {
        nodes.push({
          name: e.name,
          path: childRel,
          isDir: true,
          children: await readTree(childRel),
        });
      } else if (e.isFile && hasExt(e.name)) {
        nodes.push({ name: stripExt(e.name), path: childRel, isDir: false });
      }
    }
    return sortNodes(nodes);
  }

  /** 전체 트리 (폴더 먼저, 이름순). 루트 폴더가 없으면 만든다 */
  async function listTree(): Promise<VaultNode[]> {
    await ensureRoot();
    return readTree("");
  }

  async function readFile(relPath: string): Promise<string> {
    return readTextFile(full(relPath), { baseDir: BASE });
  }

  async function writeFile(relPath: string, content: string): Promise<void> {
    await writeAtomic(full(relPath), content);
  }

  /** 파일 수정 시각 (UTC ms). 못 읽으면 null */
  async function fileMtime(relPath: string): Promise<number | null> {
    try {
      const info = await stat(full(relPath), { baseDir: BASE });
      return info.mtime ? new Date(info.mtime).getTime() : null;
    } catch {
      return null;
    }
  }

  /** 새 폴더 생성. 다단계 입력이면 중간 폴더까지 함께 생성. 생성된 상대경로 반환 */
  async function createFolder(parentRel: string, nameOrPath: string): Promise<string> {
    const sub = normalizePath(nameOrPath);
    const rel = parentRel ? `${parentRel}/${sub}` : sub;
    if (await exists(full(rel), { baseDir: BASE }))
      throw new Error(t("common.file.dupName"));
    await mkdir(full(rel), { baseDir: BASE, recursive: true });
    return rel;
  }

  /** 새 파일 생성. 경로 입력이면 중간 폴더까지 만든다. 생성된 상대경로 반환 */
  async function createFile(parentRel: string, nameOrPath: string): Promise<string> {
    const sub = normalizePath(nameOrPath);
    const rel = (parentRel ? `${parentRel}/` : "") + `${sub}${mainExt}`;
    if (await exists(full(rel), { baseDir: BASE }))
      throw new Error(t("common.file.dupFile"));
    await mkdir(full(parentOf(rel)), { baseDir: BASE, recursive: true });
    const title = sub.slice(sub.lastIndexOf("/") + 1);
    await writeFile(rel, cfg.template(title));
    return rel;
  }

  /** 이름 변경 (같은 폴더 안에서). 새 상대경로 반환 */
  async function renameEntry(
    relPath: string,
    newName: string,
    isDir: boolean,
  ): Promise<string> {
    const parent = parentOf(relPath);
    const newRel =
      (parent ? `${parent}/` : "") + (isDir ? newName : `${newName}${mainExt}`);
    if (newRel === relPath) return relPath;
    if (await exists(full(newRel), { baseDir: BASE }))
      throw new Error(t("common.file.dupName"));
    await rename(full(relPath), full(newRel), {
      oldPathBaseDir: BASE,
      newPathBaseDir: BASE,
    });
    return newRel;
  }

  /** 다른 폴더로 이동 (드래그 앤 드롭). targetDir='' = 루트. 새 상대경로 반환.
   *  파일명/폴더명은 유지하고 상위 경로만 바꾼다. 자기 자신/하위로의 이동, 같은 폴더 이동은 거부/무시. */
  async function moveEntry(relPath: string, targetDir: string): Promise<string> {
    const dir = normalizePath(targetDir); // '' = 루트
    const curParent = parentOf(relPath);
    if (dir === curParent) return relPath; // 같은 폴더 = no-op
    if (dir === relPath || dir.startsWith(`${relPath}/`))
      throw new Error(t("common.folder.intoSelf"));
    const name = relPath.slice(relPath.lastIndexOf("/") + 1); // 파일은 확장자 포함, 폴더는 폴더명
    const newRel = dir ? `${dir}/${name}` : name;
    if (await exists(full(newRel), { baseDir: BASE }))
      throw new Error(t("common.folder.dupTarget"));
    await rename(full(relPath), full(newRel), {
      oldPathBaseDir: BASE,
      newPathBaseDir: BASE,
    });
    return newRel;
  }

  /** 삭제 (폴더면 하위 전체 포함). 영구 삭제 대신 macOS 휴지통으로 이동 → 복구 가능 */
  async function deleteEntry(relPath: string): Promise<void> {
    await invoke("move_to_trash", { relPath: full(relPath) });
  }

  return {
    listTree,
    readFile,
    writeFile,
    fileMtime,
    createFolder,
    createFile,
    renameEntry,
    moveEntry,
    deleteEntry,
  };
}
