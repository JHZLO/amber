// 파일 쪽 대역 — 지금 앱이 쓰는 tauri-plugin-fs 와 move_to_trash 명령을 하네스 폴더 위의 node:fs 로 바꾼다.
// 엔진 계약: 앱 데이터 폴더(BaseDirectory.AppData) = DB 파일이 있는 폴더. Rust 엔진도 같은 폴더를 쓴다.
// 휴지통은 진짜 휴지통 대신 `.trash/<n>/<이름>`(n 은 연결을 열 때마다 1 부터) — 지운 것도 상태에 남아 비교된다.

import * as fs from "node:fs";
import { basename, isAbsolute, join } from "node:path";

export const fsRoot = { dir: "", trashed: 0 };

const at = (p: string) => (isAbsolute(p) ? p : join(fsRoot.dir, p));

type Opts = { recursive?: boolean };

export const pluginFs = {
  BaseDirectory: { AppData: 14 },
  exists: async (p: string) => fs.existsSync(at(p)),
  mkdir: async (p: string, o?: Opts) => {
    fs.mkdirSync(at(p), { recursive: !!o?.recursive });
  },
  readDir: async (p: string) =>
    fs.readdirSync(at(p), { withFileTypes: true }).map((e) => ({
      name: e.name,
      isDirectory: e.isDirectory(),
      isFile: e.isFile(),
      isSymlink: e.isSymbolicLink(),
    })),
  readTextFile: async (p: string) => fs.readFileSync(at(p), "utf8"),
  writeTextFile: async (p: string, content: string) => fs.writeFileSync(at(p), content),
  readFile: async (p: string) => new Uint8Array(fs.readFileSync(at(p))),
  writeFile: async (p: string, data: Uint8Array) => fs.writeFileSync(at(p), data),
  rename: async (from: string, to: string) => fs.renameSync(at(from), at(to)),
  remove: async (p: string, o?: Opts) => fs.rmSync(at(p), { recursive: !!o?.recursive }),
  stat: async (p: string) => {
    const s = fs.statSync(at(p));
    return { mtime: s.mtime, isFile: s.isFile(), isDirectory: s.isDirectory(), size: s.size };
  },
};

/** src-tauri 의 move_to_trash 대역. 없으면 그대로 성공(앱과 같다) */
export function moveToTrash(relPath: string): void {
  const from = at(relPath);
  if (!fs.existsSync(from)) return;
  const dir = join(fsRoot.dir, ".trash", String(++fsRoot.trashed));
  fs.mkdirSync(dir, { recursive: true });
  fs.renameSync(from, join(dir, basename(from)));
}

const DB_FILES = /^amber\.db(-journal|-wal|-shm)?$/;

/** 앱 데이터 폴더의 파일과 폴더 전부(DB 파일 빼고). 경로는 '/' 로 잇는다 */
export function listFiles(dir: string): { path: string; kind: "dir" | "file"; text?: string; hex?: string }[] {
  const out: { path: string; kind: "dir" | "file"; text?: string; hex?: string }[] = [];
  const walk = (rel: string) => {
    for (const e of fs.readdirSync(join(dir, rel), { withFileTypes: true })) {
      if (!rel && DB_FILES.test(e.name)) continue;
      const path = rel ? `${rel}/${e.name}` : e.name;
      if (e.isDirectory()) {
        out.push({ path, kind: "dir" });
        walk(path);
      } else {
        const bytes = fs.readFileSync(join(dir, path));
        const text = bytes.toString("utf8");
        out.push(
          Buffer.from(text, "utf8").equals(bytes)
            ? { path, kind: "file", text }
            : { path, kind: "file", hex: bytes.toString("hex") },
        );
      }
    }
  };
  walk("");
  return out;
}
