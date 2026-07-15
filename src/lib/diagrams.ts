// 다이어그램: $APPDATA/vault/diagrams/ 아래 실제 디렉토리 + .mmd(mermaid) 파일이 정본.
// 필기노트와 같은 vaultTree 계층을 mermaid 설정으로 감싼 것.

import { createVaultTree } from "./vaultTree";

export {
  parentOf,
  invalidNameReason,
  invalidPathReason,
  normalizePath,
  flattenDirs,
} from "./vaultTree";
export type { VaultNode as DiagramNode } from "./vaultTree";

// 주 용도가 ERD 라 스타터도 erDiagram (지우고 다른 다이어그램 써도 됨)
const STARTER = `erDiagram
    USER ||--o{ ORDER : places
    ORDER ||--|{ ORDER_ITEM : contains

    USER {
        bigint id PK
        varchar email
    }
    ORDER {
        bigint id PK
        bigint user_id FK
    }
`;

const tree = createVaultTree({
  root: "vault/diagrams",
  exts: [".mmd", ".mermaid"],
  template: () => STARTER,
});

export const listDiagramTree = tree.listTree;
export const readDiagramFile = tree.readFile;
export const writeDiagramFile = tree.writeFile;
export const diagramMtime = tree.fileMtime;
export const createFolder = tree.createFolder;
export const createDiagram = tree.createFile;
export const renameEntry = tree.renameEntry;
export const deleteEntry = tree.deleteEntry;
