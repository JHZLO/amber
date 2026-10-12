// v0.23 — DB 연동 ERD 가 다이어그램 탭에서 ERD 탭으로 옮겨 왔다. 옮길 것은 스냅샷뿐이다: ERD 탭은 스냅샷에서
// 바로 그리므로 다이어그램 보관함에 만들어 두던 mermaid 파일은 필요 없다.
//
// 스냅샷은 말없이 복사한다(원본은 그대로). 옛 자리에 남은 자동 생성 파일을 지우는 것은 사용자가 고른다 —
// 휴지통으로 보내고, 사용자가 직접 넣어 둔 파일은 건드리지 않는다.

import { BaseDirectory, readDir } from "@tauri-apps/plugin-fs";
import { createVaultTree } from "./vaultTree";
import { getRoot } from "./workspace";
import { enabledSchemas, readSnapshot, snapshotPath, writeSnapshot, type DbConnection } from "./dbconn";
import { parseSnapshot } from "./schemaSnapshot";

/** 옛 자리 — 지금 다이어그램 보관함. 읽기와 휴지통은 보관함 트리로, 목록은 아래 listAll 로(확장자를 거르지 않는다) */
const legacy = createVaultTree({
  root: () => getRoot("diagrams"),
  exts: [".mmd", ".mermaid", ".json"],
  template: () => "",
});

/** 자동 생성된 ERD 의 표식 — 앞머리의 `%% amber:db <연결>/<스키마> …` 한 줄 */
export function isGeneratedErd(source: string): boolean {
  return source
    .split("\n", 5)
    .some((line) => /^\s*%% amber:db .+\/[^\s/]+ /.test(line)); // 연결 이름엔 공백이 올 수 있다
}

/** ERD 저장소에 없는 스냅샷을 옛 자리에서 복사해 온다 */
export async function copyLegacySnapshots(list: DbConnection[]): Promise<void> {
  for (const c of list)
    for (const p of enabledSchemas(c)) {
      if (await readSnapshot(c, p.name).catch(() => null)) continue;
      const body = await legacy.readFileIfExists(snapshotPath(c, p.name)).catch(() => null);
      const snap = body ? parseSnapshot(body) : null;
      if (!snap) continue;
      await writeSnapshot(c, p.name, snap);
    }
}

/** 옛 자리에서 지울 것 — 폴더째(자동 생성물뿐일 때) 또는 파일 하나씩(손으로 둔 파일이 섞였을 때) */
export interface LegacyPlan {
  folders: string[];
  files: string[];
  /** 그중 mermaid ERD 파일 수 — 사용자에게 말할 숫자 */
  erds: number;
}

/** 폴더 아래 모든 파일(상대경로) — 보관함 트리와 달리 확장자를 거르지 않는다. 손으로 넣은 .png·.md 도
 *  "사용자 파일"로 세어야 폴더째 지우지 않는다. 점으로 시작하는 것(.DS_Store)만 뺀다 */
async function listAll(rel: string): Promise<string[]> {
  const entries = await readDir(`${getRoot("diagrams")}/${rel}`, { baseDir: BaseDirectory.AppData });
  const out: string[] = [];
  for (const e of entries) {
    if (e.name.startsWith(".")) continue;
    const path = `${rel}/${e.name}`;
    if (e.isDirectory) out.push(...(await listAll(path)));
    else out.push(path);
  }
  return out;
}

/** 연결 폴더마다 옛 자리에 남은 자동 생성물을 찾는다. 없으면 null. 지우기 직전에 다시 부른다 —
 *  배너가 떠 있는 사이 다이어그램 탭에서 그 폴더에 파일을 만들 수 있다 */
export async function findLegacyErds(list: DbConnection[]): Promise<LegacyPlan | null> {
  const plan: LegacyPlan = { folders: [], files: [], erds: 0 };
  for (const c of list) {
    if (!(await legacy.fileExists(c.folder_path).catch(() => false))) continue;
    const files = await listAll(c.folder_path).catch(() => null);
    if (!files) continue;
    // 스냅샷은 이 연결의 스키마 자리에 있고 실제로 스냅샷으로 읽히는 것만 — 이름이 schema.json 이라고 다 아니다
    const snapshots = new Set(c.schemas.map((p) => snapshotPath(c, p.name)));
    const generated: string[] = [];
    let own = false;
    for (const f of files) {
      if (snapshots.has(f)) {
        const body = await legacy.readFile(f).catch(() => "");
        if (parseSnapshot(body)) generated.push(f);
        else own = true;
      } else if (/\.(mmd|mermaid)$/i.test(f) && isGeneratedErd(await legacy.readFile(f).catch(() => ""))) {
        generated.push(f);
        plan.erds++;
      } else own = true;
    }
    if (!generated.length) continue;
    if (own) plan.files.push(...generated);
    else plan.folders.push(c.folder_path);
  }
  return plan.folders.length || plan.files.length ? plan : null;
}

/** 찾은 것을 휴지통으로 — 되돌릴 수 있다 */
export async function trashLegacyErds(plan: LegacyPlan): Promise<void> {
  for (const p of [...plan.folders, ...plan.files]) await legacy.deleteEntry(p);
}
