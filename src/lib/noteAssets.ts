// 필기노트에 붙이는 이미지(스크린샷 등). 노트와 같은 폴더의 `_assets/` 에 파일로 두고 본문에는
// `![](_assets/<이름>)` 상대 링크만 남긴다 — vault 가 정본이라 이미지도 노트 옆에 산다.
// 폴더째 옮기거나 이름을 바꿔도 `_assets` 가 폴더 안에 있으니 링크가 그대로 산다.
//
// 이름이 `.assets` 가 아닌 이유: 점으로 시작하는 경로는 fs 스코프 glob(`$HOME/**`)에 걸리지 않는다
// (유닉스 기본 requireLiteralLeadingDot). 그걸 풀면 웹뷰가 `~/.ssh` 같은 숨김 파일까지 읽고 쓸 수
// 있게 된다. 그래서 점 없는 이름을 쓰고 트리에서만 숨긴다(notes.ts 의 hiddenDirs).
//
// 화면에는 asset 프로토콜이 아니라 fs readFile → Blob URL 로 띄운다. 이미 있는 fs 권한으로 끝나서
// 새 프로토콜/스코프/크레이트가 필요 없다.

import {
  BaseDirectory,
  exists,
  mkdir,
  readFile,
  writeFile,
} from "@tauri-apps/plugin-fs";
import { t } from "./i18n";

export const ASSET_DIR = "_assets";
const BASE = BaseDirectory.AppData;

/** 한 장의 상한. 스크린샷은 보통 수 MB 라 넉넉하지만, 영상을 잘못 끌어다 놓는 걸 막는다 */
export const MAX_IMAGE_BYTES = 20 * 1024 * 1024;

// svg 는 받지 않는다 — 스크립트를 품을 수 있는 문서라 이미지로 들이면 안 되고, 노트의 그림은
// ```svg 펜스(sanitizeSvg 를 거친다)가 맡는다.
const MIME_EXT: Record<string, string> = {
  "image/png": "png",
  "image/jpeg": "jpg",
  "image/gif": "gif",
  "image/webp": "webp",
};
const EXT_MIME: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
};

export function extForMime(mime: string): string | null {
  return MIME_EXT[mime.toLowerCase()] ?? null;
}

/** 파일 이름의 확장자로 이미지 종류를 정한다. 이미지가 아니면 null */
export function mimeForName(name: string): string | null {
  const dot = name.lastIndexOf(".");
  if (dot === -1) return null;
  return EXT_MIME[name.slice(dot + 1).toLowerCase()] ?? null;
}

/** `20260924-153012-a1b2.png` — 시각이 앞이라 폴더에서 붙인 순서대로 정렬되고, 꼬리 네 글자가
 *  같은 초에 여러 장을 붙여도 겹치지 않게 한다 */
export function assetFileName(now: Date, ext: string, rand: string): string {
  const p = (n: number) => String(n).padStart(2, "0");
  const day = `${now.getFullYear()}${p(now.getMonth() + 1)}${p(now.getDate())}`;
  const time = `${p(now.getHours())}${p(now.getMinutes())}${p(now.getSeconds())}`;
  return `${day}-${time}-${rand}.${ext}`;
}

function randomSuffix(): string {
  return Math.random().toString(36).slice(2, 6).padEnd(4, "0");
}

/** 본문에 넣을 한 줄 */
export function imageMarkdown(fileName: string): string {
  return `![](${ASSET_DIR}/${fileName})`;
}

/** 커서 자리에 끼울 텍스트 — 그림은 자기 줄에 혼자 서게 앞뒤 줄바꿈을 맞춘다.
 *  앞이 줄 중간이면 줄을 바꿔 시작하고, 뒤에 글이 이어지면 줄을 바꿔 끝낸다. */
export function imageInsertText(
  value: string,
  start: number,
  end: number,
  links: string[],
): string {
  const before = start > 0 ? value[start - 1] : "\n";
  const after = end < value.length ? value[end] : "\n";
  const head = before === "\n" ? "" : "\n";
  const tail = after === "\n" ? "" : "\n";
  return `${head}${links.join("\n")}${tail}`;
}

// ![alt](_assets/이름) / ![](./_assets/이름) / ![](<_assets/이름> "제목") 을 모두 잡는다
const REF_RE = /!\[[^\]]*\]\(\s*<?(?:\.\/)?_assets\/([^)\s>]+)>?(?:\s+"[^"]*")?\s*\)/g;

/** 본문이 같은 폴더의 `_assets` 에서 가리키는 파일 이름들 (중복 제거, 하위 경로/.. 는 제외) */
export function assetRefs(md: string): string[] {
  const out = new Set<string>();
  for (const m of md.matchAll(REF_RE)) {
    const name = m[1];
    if (name.includes("/") || name.includes("\\") || name.startsWith(".")) continue;
    out.add(name);
  }
  return [...out];
}

/** 이미지 src 를 노트 폴더 기준 fs 경로로 푼다. 원격 URL, 스킴, 절대경로, 루트 밖으로 나가는
 *  경로, 이미지가 아닌 확장자는 null — 그런 건 로컬에서 읽지 않는다. */
export function resolveLocalSrc(baseDir: string, src: string): string | null {
  const raw = src.trim();
  if (!raw || /^[a-z][a-z0-9+.-]*:/i.test(raw) || raw.startsWith("/") || raw.startsWith("//")) {
    return null;
  }
  let rel = raw.split(/[?#]/)[0];
  try {
    rel = decodeURIComponent(rel);
  } catch {
    return null;
  }
  if (!mimeForName(rel)) return null;
  const stack = baseDir.split("/").filter(Boolean);
  for (const s of rel.split("/")) {
    if (!s || s === ".") continue;
    if (s === "..") {
      if (!stack.length) return null;
      stack.pop();
      continue;
    }
    stack.push(s);
  }
  return (baseDir.startsWith("/") ? "/" : "") + stack.join("/");
}

/** 붙여넣은/끌어다 놓은 이미지를 `<dirPath>/_assets/` 에 쓰고 파일 이름을 돌려준다.
 *  dirPath 는 워크스페이스 루트를 포함한 노트 폴더 경로(appdata 상대 또는 절대). */
export async function saveNoteImage(
  dirPath: string,
  data: Uint8Array,
  mime: string,
  now = new Date(),
): Promise<string> {
  const ext = extForMime(mime);
  if (!ext) throw new Error(t("notes.image.unsupported"));
  if (data.byteLength > MAX_IMAGE_BYTES) throw new Error(t("notes.image.tooLarge"));
  const dir = `${dirPath}/${ASSET_DIR}`;
  await mkdir(dir, { baseDir: BASE, recursive: true });
  const name = assetFileName(now, ext, randomSuffix());
  await writeFile(`${dir}/${name}`, data, { baseDir: BASE });
  return name;
}

/** Finder 에서 끌어다 놓은 파일(절대경로)을 노트 폴더의 `_assets/` 로 복사한다 */
export async function importImageFile(dirPath: string, absPath: string): Promise<string> {
  const name = absPath.slice(absPath.lastIndexOf("/") + 1);
  const mime = mimeForName(name);
  if (!mime) throw new Error(t("notes.image.unsupported"));
  const data = await readFile(absPath);
  return saveNoteImage(dirPath, data, mime);
}

/** 노트를 다른 폴더로 옮길 때, 본문이 가리키는 이미지를 새 폴더의 `_assets/` 로 복사한다.
 *  옮기지 않고 복사하는 이유: 같은 폴더의 다른 노트가 본문을 붙여 넣어 같은 파일을 가리킬 수 있다.
 *  옮기면 그쪽이 깨진다. 남는 원본은 숨은 폴더라 보이지 않고 디스크만 조금 쓴다. */
export async function copyAssetsForMove(
  md: string,
  fromDir: string,
  toDir: string,
): Promise<void> {
  const names = assetRefs(md);
  if (!names.length || fromDir === toDir) return;
  const src = `${fromDir}/${ASSET_DIR}`;
  const dst = `${toDir}/${ASSET_DIR}`;
  let made = false;
  for (const name of names) {
    const from = `${src}/${name}`;
    const to = `${dst}/${name}`;
    if (!(await exists(from, { baseDir: BASE }))) continue;
    if (await exists(to, { baseDir: BASE })) continue; // 이름이 시각+난수라 같으면 같은 파일이다
    if (!made) {
      await mkdir(dst, { baseDir: BASE, recursive: true });
      made = true;
    }
    await writeFile(to, await readFile(from, { baseDir: BASE }), { baseDir: BASE });
  }
}

// ── 화면용 Blob URL 캐시 ──────────────────────────────────────────────
// 노트를 오갈 때마다 같은 파일을 다시 읽지 않게 경로별로 한 번만 읽는다. 파일 이름이 시각+난수라
// 같은 경로의 내용이 바뀌지 않으므로 무효화가 필요 없다. 오래 쓰면 쌓이니 개수로 자른다.
const CACHE_LIMIT = 40;
const cache = new Map<string, Promise<string>>();

export function loadImageUrl(path: string): Promise<string> {
  const hit = cache.get(path);
  if (hit) {
    cache.delete(path); // 최근 쓴 것을 뒤로 — Map 순서가 곧 LRU 순서
    cache.set(path, hit);
    return hit;
  }
  const mime = mimeForName(path) ?? "application/octet-stream";
  const p = readFile(path, { baseDir: BASE }).then((bytes) =>
    URL.createObjectURL(new Blob([bytes], { type: mime })),
  );
  // 실패한 읽기는 캐시에 남기지 않는다 — 파일을 나중에 넣으면 다시 시도해야 한다
  p.catch(() => cache.delete(path));
  cache.set(path, p);
  while (cache.size > CACHE_LIMIT) {
    const [oldKey, oldUrl] = cache.entries().next().value as [string, Promise<string>];
    cache.delete(oldKey);
    oldUrl.then((u) => URL.revokeObjectURL(u)).catch(() => {});
  }
  return p;
}
