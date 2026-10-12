// ERD 탭의 저장소 — `$APPDATA/vault/erd/<연결 폴더>/<스키마>/schema.json`. 스냅샷만 둔다.
// 그림은 스냅샷에서 그때그때 그린다(lib/erdGen.ts → resin) — 파일이 없으니 낡을 일도, 덮어쓸 일도 없다.
// 사용자가 고르는 작업 폴더가 아니라 앱이 관리하는 자리라 루트는 고정이다("폴더 열기" 없음).

import { createVaultTree } from "./vaultTree";

export const ERD_ROOT = "vault/erd";

const tree = createVaultTree({ root: ERD_ROOT, exts: [".json"], template: () => "" });

export const readErdFileIfExists = tree.readFileIfExists;
export const writeErdFile = tree.writeFile;
export const ensureErdDir = tree.ensureDir;
export const erdFileExists = tree.fileExists;
export const deleteErdEntry = tree.deleteEntry;
