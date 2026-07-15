// vault 하위 디렉터리 트리(실제 폴더 + 파일)를 다루는 공용 계층.
// 필기노트(vault/notes, .md)와 다이어그램(vault/diagrams, .mmd)이
// 루트/확장자/새 파일 템플릿만 달리해 같은 구현을 공유한다. DB 없음 — 파일시스템이 정본.

import {
  BaseDirectory,
  exists,
  mkdir,
  readDir,
  readTextFile,
  rename,
  stat,
  writeTextFile,
} from "@tauri-apps/plugin-fs";
import { invoke } from "@tauri-apps/api/core";

const BASE = BaseDirectory.AppData;

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
  if (!n) return "이름을 입력하세요.";
  if (/[/\\:]/.test(n)) return "이름에 / \\ : 는 쓸 수 없어요.";
  if (n.startsWith(".")) return "이름은 . 으로 시작할 수 없어요.";
  if (n.length > 80) return "이름이 너무 길어요 (80자 이내).";
  return null;
}

/** 'CS/네트워크' 같은 다단계 경로 입력 검증 (구간별로 이름 규칙 적용) */
export function invalidPathReason(path: string): string | null {
  const segs = path.split("/").map((s) => s.trim()).filter(Boolean);
  if (!segs.length) return "이름을 입력하세요.";
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

export interface VaultTreeConfig {
  /** appDataDir 기준 루트 (예: 'vault/notes') */
  root: string;
  /** 트리에 보일 파일 확장자(소문자, 점 포함). 첫 항목이 생성 시 기본 확장자 */
  exts: string[];
  /** 새 파일 초기 내용 (파일명 기반) */
  template: (title: string) => string;
}

export function createVaultTree(cfg: VaultTreeConfig) {
  const full = (rel: string) => (rel ? `${cfg.root}/${rel}` : cfg.root);
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
    await mkdir(cfg.root, { baseDir: BASE, recursive: true });
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
    await writeTextFile(full(relPath), content, { baseDir: BASE });
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
      throw new Error("같은 이름이 이미 있어요.");
    await mkdir(full(rel), { baseDir: BASE, recursive: true });
    return rel;
  }

  /** 새 파일 생성. 경로 입력이면 중간 폴더까지 만든다. 생성된 상대경로 반환 */
  async function createFile(parentRel: string, nameOrPath: string): Promise<string> {
    const sub = normalizePath(nameOrPath);
    const rel = (parentRel ? `${parentRel}/` : "") + `${sub}${mainExt}`;
    if (await exists(full(rel), { baseDir: BASE }))
      throw new Error("같은 이름의 파일이 이미 있어요.");
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
      throw new Error("같은 이름이 이미 있어요.");
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
    deleteEntry,
  };
}
