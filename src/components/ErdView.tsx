// ERD 탭 — 연결한 DB 의 스키마를 그린다. 손으로 그리는 ERD 는 다이어그램 탭(mermaid)의 일이고, 여기는 DB 가 정본이다.
//
// 좌 = 연결 → 스키마 → 테이블 트리, 우 = 고른 스키마의 ERD(또는 개요). 그림은 파일이 아니라 스냅샷(schema.json)에서
// 그때그때 만든다(lib/erdGen.ts → resin) — 동기화하면 바로 새 구조가 그려지고, 덮어쓸 파일도 낡을 파일도 없다.
// 그리기와 조작은 resin 플레이그라운드의 것을 그대로 쓴다(ErdCanvas).

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { compile } from "resin";
import {
  DB_CONNECTIONS_EVENT,
  connStatus,
  connStatusDot,
  connStatusLabel,
  deleteConnection,
  enabledSchemas,
  envLabel,
  isStale,
  listConnections,
  notifyConnectionsChanged,
  prefAudit,
  readSnapshot,
  schemaFolder,
  setSchemaStarred,
  syncSchema,
  type DbConnection,
  type DbSchemaPref,
} from "../lib/dbconn";
import { diffIsEmpty, formatHeaderTime, type SchemaDiff, type SchemaSnapshot } from "../lib/schemaSnapshot";
import { generateErd } from "../lib/erdGen";
import { ERD_ROOT } from "../lib/erdStore";
import {
  copyLegacySnapshots,
  findLegacyErds,
  trashLegacyErds,
  type LegacyPlan,
} from "../lib/erdMigrate";
import { usePaneResize } from "../lib/usePaneResize";
import { t } from "../lib/i18n";
import { errText } from "../lib/errors";
import { Icon } from "../icons";
import { Modal, Spinner, Tooltip, timeAgo } from "../ui";
import { TREE_ROW_ATTR, TreeFindBar, TreeLabel, useTreeFind } from "./TreeFind";
import { DbConnectionModal } from "./DbConnectionModal";
import { SchemaOverview } from "./SchemaOverview";
import { ErdCanvas } from "./ErdCanvas";
import { Button } from "@/components/ui/button";

/** 트리 노드 — 경로는 `c<연결 id>/<스키마>/<테이블>`. 찾기(⌘F)가 이 모양을 그대로 거른다 */
interface ErdNode {
  path: string;
  name: string;
  isDir: boolean;
  children?: ErdNode[];
}

const SELECTED_KEY = "amber.erd.selected";
const LEGACY_DISMISSED_KEY = "amber.erd.legacy-dismissed";
/** 한 연결에서 동시에 읽는 스키마 수 — 터널 너머 왕복이 병목이라 셋이면 체감이 다르다(Rust 풀은 4) */
const SYNC_WORKERS = 3;

const connPath = (c: DbConnection) => `c${c.id}`;
const schemaPath = (c: DbConnection, schema: string) => `c${c.id}/${schema}`;
const isAudit = (name: string) => name.endsWith("_aud") || name === "revinfo";
/** 로케일 무관 정렬 — 트리 순서가 언어 설정에 따라 흔들리지 않게 */
const cmp = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);

export function ErdView({ active }: { active: boolean }) {
  const [connections, setConnections] = useState<DbConnection[] | null>(null);
  // 스키마 폴더 경로 → 스냅샷 (null = 아직 없음)
  const [snaps, setSnaps] = useState<Map<string, SchemaSnapshot | null>>(new Map());
  const [syncing, setSyncing] = useState<Set<string>>(new Set());
  const [queued, setQueued] = useState<Set<string>>(new Set());
  const syncingRef = useRef<Set<string>>(new Set());
  // 이 세션에서 동기화로 알게 된 구조 변화 — 배너가 무엇이 바뀌었는지 말한다
  const [diffs, setDiffs] = useState<Map<string, SchemaDiff>>(new Map());
  const [dbError, setDbError] = useState<string | null>(null);
  const [selected, setSelected] = useState<{ ulid: string; schema: string } | null>(() => {
    try {
      const v = JSON.parse(localStorage.getItem(SELECTED_KEY) ?? "null");
      return v && typeof v.ulid === "string" && typeof v.schema === "string" ? v : null;
    } catch {
      return null;
    }
  });
  const [mode, setMode] = useState<"erd" | "overview">("erd");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [reveal, setReveal] = useState<{ table: string; seq: number } | null>(null);
  // 늘 늘어나는 번호 — 스키마를 바꾸며 reveal 을 비운 같은 순간에 새로 골라도 번호가 겹치지 않게
  const revealSeq = useRef(0);
  const [dbModal, setDbModal] = useState<{ open: boolean; connection: DbConnection | null }>({
    open: false,
    connection: null,
  });
  const [confirmDisconnect, setConfirmDisconnect] = useState<DbConnection | null>(null);
  const [busy, setBusy] = useState(false);
  const [legacy, setLegacy] = useState<LegacyPlan | null>(null);
  const [confirmLegacy, setConfirmLegacy] = useState(false);
  // 세션당 한 번만 자동 동기화를 시도한 스키마 — 실패한 연결이 이벤트마다 다시 두드리지 않게
  const attempted = useRef<Set<string>>(new Set());
  const migrated = useRef(false);

  useEffect(() => {
    try {
      localStorage.setItem(SELECTED_KEY, JSON.stringify(selected));
    } catch {
      /* 이번 실행 동안만 */
    }
  }, [selected]);

  /** 연결 목록 + 각 스키마의 스냅샷을 다시 읽는다 */
  const loadConnections = useCallback(async () => {
    try {
      const list = await listConnections();
      // 다이어그램 탭 시절의 스냅샷을 한 번 옮겨 온다(복사) — 다시 읽지 않아도 지난 구조가 바로 보이게
      if (!migrated.current) {
        migrated.current = true;
        await copyLegacySnapshots(list).catch(() => {});
        let dismissed = false;
        try {
          dismissed = localStorage.getItem(LEGACY_DISMISSED_KEY) === "1";
        } catch {
          /* 묻는다 */
        }
        if (!dismissed) void findLegacyErds(list).then(setLegacy);
      }
      const next = new Map<string, SchemaSnapshot | null>();
      for (const c of list)
        for (const p of enabledSchemas(c))
          next.set(schemaFolder(c, p.name), await readSnapshot(c, p.name).catch(() => null));
      setConnections(list);
      setSnaps(next);
      return { list, snaps: next };
    } catch (e) {
      setDbError(errText(e));
      setConnections((cur) => cur ?? []);
      return null;
    }
  }, []);

  /** 행(last_sync_at, last_error)만 다시 읽는다 — 스냅샷은 동기화가 이미 상태에 넣었다 */
  const refreshConnectionRows = useCallback(async () => {
    try {
      setConnections(await listConnections());
    } catch {
      /* 다음 로드에서 잡힌다 */
    }
  }, []);

  /** 스키마 하나를 DB 에서 다시 읽는다. 같은 스키마가 이미 도는 중이면 겹쳐 돌리지 않는다 */
  async function syncOne(conn: DbConnection, pref: DbSchemaPref): Promise<SchemaSnapshot | null> {
    const folder = schemaFolder(conn, pref.name);
    if (syncingRef.current.has(folder)) return snaps.get(folder) ?? null;
    syncingRef.current.add(folder);
    setSyncing((prev) => new Set(prev).add(folder));
    setDbError(null);
    try {
      const r = await syncSchema(conn, pref.name);
      setSnaps((m) => new Map(m).set(folder, r.snapshot));
      if (r.diff && !diffIsEmpty(r.diff)) {
        const d = r.diff;
        setDiffs((m) => new Map(m).set(folder, d));
      }
      return r.snapshot;
    } catch (e) {
      setDbError(t("erd.db.syncFail", { msg: errText(e) }));
      return null;
    } finally {
      syncingRef.current.delete(folder);
      setSyncing((prev) => {
        const n = new Set(prev);
        n.delete(folder);
        return n;
      });
      void refreshConnectionRows();
    }
  }

  /** 연결 하나의 스키마들을 워커 셋으로 읽는다. 접속, 키체인 실패는 연결 단위의 사고라 남은 스키마는 건너뛴다
   *  (실측: 키체인 확인창이 스키마 수만큼 떴다) */
  async function syncMany(conn: DbConnection, todo: DbSchemaPref[]) {
    if (!todo.length) return;
    const folders = todo.map((p) => schemaFolder(conn, p.name));
    setQueued((q) => new Set([...q, ...folders]));
    setExpanded((e) => new Set(e).add(connPath(conn)));
    let failed = false;
    const queue = [...todo];
    const worker = async () => {
      while (queue.length && !failed) {
        const pref = queue.shift()!;
        const folder = schemaFolder(conn, pref.name);
        setQueued((q) => {
          const n = new Set(q);
          n.delete(folder);
          return n;
        });
        // 사용자가 먼저 눌러 이미 도는 중이다 — 그 결과를 기다리지 않고 넘어간다(실패로 세지 않는다)
        if (syncingRef.current.has(folder)) continue;
        if (!(await syncOne(conn, pref))) failed = true;
      }
    };
    await Promise.all(Array.from({ length: SYNC_WORKERS }, worker));
    setQueued((q) => {
      const n = new Set(q);
      for (const f of folders) n.delete(f);
      return n;
    });
  }

  /** 스냅샷이 없는 스키마를 세션당 한 번 채운다 — 새 연결(설정에서 만든 것 포함)의 첫 동기화 */
  async function autoSyncMissing(list: DbConnection[], have: Map<string, SchemaSnapshot | null>) {
    for (const conn of list) {
      const todo = enabledSchemas(conn).filter((p) => {
        const folder = schemaFolder(conn, p.name);
        return !have.get(folder) && !attempted.current.has(folder);
      });
      for (const p of todo) attempted.current.add(schemaFolder(conn, p.name));
      await syncMany(conn, todo);
    }
  }
  const autoSyncRef = useRef(autoSyncMissing);
  autoSyncRef.current = autoSyncMissing;

  useEffect(() => {
    if (!active) return;
    void loadConnections().then((r) => r && autoSyncRef.current(r.list, r.snaps));
  }, [active, loadConnections]);

  useEffect(() => {
    const h = () => void loadConnections().then((r) => r && autoSyncRef.current(r.list, r.snaps));
    window.addEventListener(DB_CONNECTIONS_EVENT, h);
    return () => window.removeEventListener(DB_CONNECTIONS_EVENT, h);
  }, [loadConnections]);

  // 고른 스키마 — 프로필이 바뀌거나 지워지면 따라간다
  const current = useMemo(() => {
    if (!selected || !connections) return null;
    const conn = connections.find((c) => c.ulid === selected.ulid);
    const pref = conn?.schemas.find((p) => p.name === selected.schema && p.enabled);
    return conn && pref ? { conn, pref, folder: schemaFolder(conn, pref.name) } : null;
  }, [selected, connections]);
  const snapshot = current ? snaps.get(current.folder) ?? null : null;

  // 고른 스키마의 연결은 펼쳐 둔다 — 다시 열었을 때(선택은 기억된다) 트리에서 어디인지 보이게
  useEffect(() => {
    if (!current) return;
    const path = connPath(current.conn);
    setExpanded((e) => (e.has(path) ? e : new Set(e).add(path)));
  }, [current?.conn.id]);

  /** 스냅샷 → resin → 모델. 같은 스냅샷이면 다시 만들지 않는다 */
  const erd = useMemo(() => {
    if (!snapshot || !current) return null;
    try {
      const { source } = generateErd(snapshot, { audit: prefAudit(current.pref) });
      const r = compile(source);
      const err = r.diagnostics.find((d) => d.severity === "error");
      return { model: r.model, error: err ? `${err.span.line}:${err.span.col} ${err.message}` : null };
    } catch (e) {
      return { model: null, error: errText(e) };
    }
  }, [snapshot, current?.pref.audit]);

  function openSchema(conn: DbConnection, pref: DbSchemaPref) {
    setSelected({ ulid: conn.ulid, schema: pref.name });
    setReveal(null); // 다른 스키마에서 고른 표를 이 스키마에 다시 열지 않게
    setExpanded((e) => new Set(e).add(connPath(conn)));
    // 열 때 한 번, 스냅샷이 오래됐으면(10분) 다시 읽는다 — 폴링하지 않는다
    if (isStale(snaps.get(schemaFolder(conn, pref.name)) ?? null)) void syncOne(conn, pref);
  }

  function openTable(conn: DbConnection, pref: DbSchemaPref, table: string) {
    if (!current || current.conn.id !== conn.id || current.pref.name !== pref.name) openSchema(conn, pref);
    setMode("erd");
    setReveal({ table, seq: ++revealSeq.current });
  }

  async function toggleStar(conn: DbConnection, pref: DbSchemaPref) {
    const starred = !pref.starred;
    // 트리 순서가 바로 바뀌도록 화면 상태를 먼저 고친다
    setConnections((prev) =>
      (prev ?? []).map((c) =>
        c.id !== conn.id ? c : { ...c, schemas: c.schemas.map((p) => (p.name === pref.name ? { ...p, starred } : p)) },
      ),
    );
    try {
      await setSchemaStarred(conn, pref.name, starred);
    } catch (e) {
      setDbError(errText(e));
    }
    await refreshConnectionRows();
  }

  /** 연결 끊기 — 프로필, 키체인 비밀번호, 이 연결의 스냅샷(휴지통)이 지워진다. DB 에는 아무 일도 없다 */
  async function doDisconnect() {
    const c = confirmDisconnect;
    if (!c || busy) return;
    setBusy(true);
    try {
      await deleteConnection(c);
      if (current?.conn.id === c.id) setSelected(null);
      // 같은 이름으로 다시 연결하면 같은 폴더를 쓴다 — 지난 시도와 변화 기록을 비워 첫 동기화가 다시 돌게
      const prefix = `${c.folder_path}/`;
      for (const f of [...attempted.current]) if (f.startsWith(prefix)) attempted.current.delete(f);
      setDiffs((m) => new Map([...m].filter(([f]) => !f.startsWith(prefix))));
      notifyConnectionsChanged();
      setConfirmDisconnect(null);
      await loadConnections();
    } catch (e) {
      setDbError(errText(e));
      setConfirmDisconnect(null);
    } finally {
      setBusy(false);
    }
  }

  async function doTrashLegacy() {
    if (!legacy || busy) return;
    setBusy(true);
    try {
      // 확인을 누른 지금 다시 본다 — 배너가 떠 있는 사이 그 폴더에 파일을 만들었을 수 있다
      const plan = await findLegacyErds(connections ?? []);
      if (plan) await trashLegacyErds(plan);
      setLegacy(null);
      setConfirmLegacy(false);
    } catch (e) {
      setDbError(errText(e));
      setConfirmLegacy(false);
    } finally {
      setBusy(false);
    }
  }

  function keepLegacy() {
    try {
      localStorage.setItem(LEGACY_DISMISSED_KEY, "1");
    } catch {
      /* 이번 실행 동안만 */
    }
    setLegacy(null);
  }

  // ---- 트리 ----

  const tree = useMemo<ErdNode[]>(() => {
    if (!connections) return [];
    return connections.map((c) => ({
      path: connPath(c),
      name: c.name,
      isDir: true,
      // 즐겨찾기 스키마는 따로 모으지 않고 제 연결 안에서 맨 위로 — 한 스키마가 두 줄이 되지 않게
      children: [...enabledSchemas(c)]
        .sort((a, b) => Number(!!b.starred) - Number(!!a.starred) || cmp(a.name, b.name))
        .map((p) => {
          const snap = snaps.get(schemaFolder(c, p.name));
          return {
            path: schemaPath(c, p.name),
            name: p.name,
            isDir: true,
            // 도메인 테이블 먼저, 감사 테이블은 뒤로
            children: (snap?.tables ?? [])
              .map((tb) => tb.name)
              .sort((a, b) => Number(isAudit(a)) - Number(isAudit(b)) || cmp(a, b))
              .map((name) => ({ path: `${schemaPath(c, p.name)}/${name}`, name, isDir: false })),
          };
        }),
    }));
  }, [connections, snaps]);

  const byPath = useMemo(() => {
    const m = new Map<string, { conn: DbConnection; pref?: DbSchemaPref; table?: string }>();
    for (const c of connections ?? []) {
      m.set(connPath(c), { conn: c });
      for (const p of enabledSchemas(c)) {
        m.set(schemaPath(c, p.name), { conn: c, pref: p });
        for (const tb of snaps.get(schemaFolder(c, p.name))?.tables ?? [])
          m.set(`${schemaPath(c, p.name)}/${tb.name}`, { conn: c, pref: p, table: tb.name });
      }
    }
    return m;
  }, [connections, snaps]);

  const toggle = (path: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  const find = useTreeFind(tree, (path, isDir) => {
    const hit = byPath.get(path);
    if (!hit) return;
    if (isDir) {
      setExpanded((e) => new Set(e).add(path));
      if (hit.pref) openSchema(hit.conn, hit.pref);
      return;
    }
    if (hit.pref && hit.table) openTable(hit.conn, hit.pref, hit.table);
    find.close();
  });

  const pane = usePaneResize({ storageKey: "amber.erd.list-width", active });

  function renderConnection(n: ErdNode) {
    const conn = byPath.get(n.path)?.conn;
    if (!conn) return null;
    const isOpen = expanded.has(n.path) || find.autoExpand.has(n.path);
    const folders = enabledSchemas(conn).map((p) => schemaFolder(conn, p.name));
    const running = folders.filter((f) => syncing.has(f) || queued.has(f)).length;
    const progress = running ? { done: folders.length - running, total: folders.length } : null;
    return (
      <div key={n.path} className="tree-branch">
        <div
          {...{ [TREE_ROW_ATTR]: n.path }}
          className={`tree-row dir ${find.current?.path === n.path ? "find-current" : ""}`}
          style={{ paddingLeft: 8 }}
          onClick={() => toggle(n.path)}
        >
          <span className={`caret ${isOpen ? "open" : ""}`}>
            <Icon name="chevron-right" size={13} />
          </span>
          <Icon name="database" size={14} className="tree-ico" />
          <span className="label" title={conn.name}>
            <TreeLabel name={n.name} query={find.query} />
          </span>
          <span className="tree-count">
            {progress ? (
              // 동기화가 도는 동안은 상태 점 대신 진행 — 기다리는 이유가 여기 보인다
              t("erd.db.tree.progress", progress)
            ) : (
              <>
                {/* 상태 점이 유일한 신호라 툴팁이 같은 상태를 단어로 준다(DESIGN §2 결과 색) */}
                <Tooltip label={connStatusLabel(connStatus(conn))}>
                  <span className={`db-dot ${connStatusDot(connStatus(conn))}`} />
                </Tooltip>
                {envLabel(conn.env)}
              </>
            )}
          </span>
          <span className="row-actions" onClick={(e) => e.stopPropagation()}>
            <Tooltip label={t("erd.db.tree.syncAll")}>
              <Button
                aria-label={t("erd.db.tree.syncAll")}
                size="icon-sm"
                disabled={!!progress}
                onClick={() => void syncMany(conn, enabledSchemas(conn))}
              >
                <Icon name="refresh" size={13} />
              </Button>
            </Tooltip>
            <Tooltip label={t("erd.db.tree.edit")}>
              <Button
                aria-label={t("erd.db.tree.edit")}
                size="icon-sm"
                onClick={() => setDbModal({ open: true, connection: conn })}
              >
                <Icon name="settings" size={13} />
              </Button>
            </Tooltip>
            <Tooltip label={t("erd.db.tree.disconnect")}>
              <Button
                aria-label={t("erd.db.tree.disconnect")}
                variant="danger"
                size="icon-sm"
                onClick={() => setConfirmDisconnect(conn)}
              >
                <Icon name="x" size={13} />
              </Button>
            </Tooltip>
          </span>
        </div>
        {isOpen && n.children && n.children.length > 0 && (
          <div className="tree-children open">
            <div className="tree-children-inner">{n.children.map(renderSchema)}</div>
          </div>
        )}
      </div>
    );
  }

  function renderSchema(n: ErdNode) {
    const hit = byPath.get(n.path);
    if (!hit?.pref) return null;
    const { conn, pref } = hit;
    const folder = schemaFolder(conn, pref.name);
    const isOpen = expanded.has(n.path) || find.autoExpand.has(n.path);
    const isSyncing = syncing.has(folder);
    const isQueued = !isSyncing && queued.has(folder);
    const snap = snaps.get(folder);
    const isSelected = current?.folder === folder && current.conn.id === conn.id;
    return (
      <div key={n.path} className="tree-branch">
        <div
          {...{ [TREE_ROW_ATTR]: n.path }}
          className={`tree-row dir ${isSelected ? "selected" : ""} ${isSyncing ? "db-syncing" : ""} ${
            find.current?.path === n.path ? "find-current" : ""
          }`}
          style={{ paddingLeft: 22 }}
          onClick={() => {
            // 스키마는 클릭 = ERD, 펼침은 셰브론이 맡는다(처음 클릭엔 함께 펼친다)
            openSchema(conn, pref);
            if (!expanded.has(n.path)) toggle(n.path);
          }}
        >
          <span
            className={`caret ${isOpen ? "open" : ""}`}
            onClick={(e) => {
              e.stopPropagation();
              toggle(n.path);
            }}
          >
            <Icon name="chevron-right" size={13} />
          </span>
          <Icon name={isSyncing ? "refresh" : "table"} size={14} className="tree-ico" />
          <span className="label" title={pref.name}>
            <TreeLabel name={n.name} query={find.query} />
          </span>
          {pref.label && <span className="tree-sub">{pref.label}</span>}
          {isSyncing && <span className="tree-count">{t("erd.db.tree.syncing")}</span>}
          {isQueued && <span className="tree-count">{t("erd.db.tree.queued")}</span>}
          {!isSyncing && !isQueued && (snap || pref.starred) && (
            <span className="tree-count">
              {pref.starred && <Icon name="star" size={11} className="icon-fill tree-star" />}
              {snap?.tables.length}
            </span>
          )}
          <span className="row-actions" onClick={(e) => e.stopPropagation()}>
            <Tooltip label={t(pref.starred ? "erd.db.tree.unstar" : "erd.db.tree.star")}>
              <Button
                aria-label={t(pref.starred ? "erd.db.tree.unstar" : "erd.db.tree.star")}
                aria-pressed={!!pref.starred}
                size="icon-sm"
                onClick={() => void toggleStar(conn, pref)}
              >
                <Icon name="star" size={13} className={pref.starred ? "icon-fill" : undefined} />
              </Button>
            </Tooltip>
            <Tooltip label={t("erd.db.tree.syncHere")}>
              <Button
                aria-label={t("erd.db.tree.syncHere")}
                size="icon-sm"
                disabled={isSyncing}
                onClick={() => void syncOne(conn, pref)}
              >
                <Icon name="refresh" size={13} />
              </Button>
            </Tooltip>
          </span>
        </div>
        {isOpen && n.children && n.children.length > 0 && (
          <div className="tree-children open">
            <div className="tree-children-inner">
              {n.children.map((leaf) => {
                const comment = snap?.tables.find((tb) => tb.name === leaf.name)?.comment;
                return (
                  <div key={leaf.path} className="tree-branch">
                    <div
                      {...{ [TREE_ROW_ATTR]: leaf.path }}
                      className={`tree-row erd-leaf ${find.current?.path === leaf.path ? "find-current" : ""}`}
                      style={{ paddingLeft: 36 }}
                      onClick={() => openTable(conn, pref, leaf.name)}
                    >
                      <span className="caret leaf" />
                      <span className="label" title={leaf.name}>
                        <TreeLabel name={leaf.name} query={find.query} />
                      </span>
                      {comment && <span className="tree-sub">{comment}</span>}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}
      </div>
    );
  }

  const diff = current ? diffs.get(current.folder) : undefined;

  return (
    <div className="body" {...pane.bodyProps}>
      <aside className="list" {...find.paneProps}>
        <div className="notes-tree-head">
          <span className="erd-tree-title">{t("erd.tree.title")}</span>
          <span className="spacer" />
          <Tooltip label={t("common.find.treeTip")}>
            <Button size="icon-sm" aria-label={t("common.find.treeTip")} onClick={find.start}>
              <Icon name="search" size={14} />
            </Button>
          </Tooltip>
          <Tooltip label={t("erd.db.tooltip.add")}>
            <Button
              size="icon-sm"
              aria-label={t("erd.db.addConnection")}
              onClick={() => setDbModal({ open: true, connection: null })}
            >
              <Icon name="plus" size={14} />
            </Button>
          </Tooltip>
        </div>
        <TreeFindBar find={find} />

        {legacy && (
          <div className="erd-legacy">
            <p>{t("erd.legacy.body", { n: legacy.erds })}</p>
            <div className="erd-legacy-actions">
              <Button size="sm" onClick={keepLegacy}>
                {t("erd.legacy.keep")}
              </Button>
              <Button variant="danger" size="sm" onClick={() => setConfirmLegacy(true)}>
                {t("erd.legacy.trash")}
              </Button>
            </div>
          </div>
        )}

        {connections === null ? (
          <Spinner />
        ) : connections.length === 0 ? (
          <div className="tree-empty">
            <p>{t("erd.empty.noConnection")}</p>
            <Button variant="primary" size="sm" onClick={() => setDbModal({ open: true, connection: null })}>
              <Icon name="database" size={14} />
              {t("erd.empty.connect")}
            </Button>
          </div>
        ) : (
          <div className="tree">
            {find.open && find.nodes.length === 0 ? (
              <p className="tree-find-empty">{t("common.find.treeEmpty")}</p>
            ) : (
              find.nodes.map(renderConnection)
            )}
          </div>
        )}
      </aside>

      <div {...pane.resizerProps} />

      <section className="detail dgm">
        {current ? (
          <div className="notes-detail editing erd-detail">
            <div className="dgm-head">
              <div className="dgm-head-info">
                <div className="note-crumb">
                  <Icon name="database" size={12} />
                  {current.conn.name}
                </div>
                <h1 className="dgm-title db-title">
                  {current.pref.name}
                  {current.pref.label && <span className="chip chip-sm">{current.pref.label}</span>}
                  <span className={`chip chip-sm ${current.conn.env === "prod" ? "chip-fill" : ""}`}>
                    {envLabel(current.conn.env)}
                  </span>
                </h1>
              </div>
              <span className="spacer" />
              <div className="segmented erd-mode">
                <button className={`tab ${mode === "erd" ? "active" : ""}`} onClick={() => setMode("erd")}>
                  {t("erd.mode.erd")}
                </button>
                <button className={`tab ${mode === "overview" ? "active" : ""}`} onClick={() => setMode("overview")}>
                  {t("erd.mode.overview")}
                </button>
              </div>
              <Button
                size="sm"
                onClick={() => void syncOne(current.conn, current.pref)}
                disabled={syncing.has(current.folder)}
              >
                <Icon name="refresh" size={14} />
                {syncing.has(current.folder)
                  ? t("erd.db.syncingShort")
                  : snapshot
                    ? t("erd.db.syncAgo", { ago: timeAgo(snapshot.synced_at) })
                    : t("erd.db.sync")}
              </Button>
            </div>

            {dbError && <div className="error-note">{dbError}</div>}
            {erd?.error && <div className="error-note">{t("erd.compileError", { msg: erd.error })}</div>}
            {diff && (
              <div className="db-banner">
                <Icon name="database" size={14} />
                <span>
                  <b>{t("erd.db.banner.changed")}</b> {t("erd.db.banner.detail", {
                    ta: diff.tablesAdded.length,
                    tr: diff.tablesRemoved.length,
                    ca: diff.columnsAdded.length,
                    cr: diff.columnsRemoved.length,
                    cc: diff.columnsChanged.length + diff.constraintTables.length,
                  })}
                </span>
                <span className="spacer" />
                <Button
                  size="sm"
                  onClick={() =>
                    setDiffs((m) => {
                      const n = new Map(m);
                      n.delete(current.folder);
                      return n;
                    })
                  }
                >
                  {t("common.close")}
                </Button>
              </div>
            )}

            {/* 캔버스는 개요를 보는 동안에도 남겨 둔다 — 유리(WebGL)와 시점을 잃지 않게 */}
            <div className={`erd-body ${mode === "erd" ? "" : "hidden"}`}>
              {snapshot ? (
                <ErdCanvas
                  model={erd?.model ?? null}
                  drawingId={`${current.conn.ulid}/${current.pref.name}/${snapshot.fingerprint}/${prefAudit(current.pref)}`}
                  active={active && mode === "erd"}
                  reveal={reveal}
                />
              ) : (
                <div className="db-ov-empty erd-empty">
                  {syncing.has(current.folder) ? <Spinner /> : <Icon name="table" size={22} />}
                  <p>{syncing.has(current.folder) ? t("erd.db.syncingShort") : t("erd.noSnapshot")}</p>
                </div>
              )}
            </div>
            {mode === "overview" && (
              <div className="erd-body erd-overview">
                <SchemaOverview
                  snapshot={snapshot}
                  syncing={syncing.has(current.folder)}
                  onPickTable={(table) => openTable(current.conn, current.pref, table)}
                />
              </div>
            )}

            <div className="detail-meta">
              {ERD_ROOT}/{current.folder}
              {snapshot && (
                <>
                  {" "}
                  {t("erd.db.overview.snapshot", {
                    time: formatHeaderTime(new Date(snapshot.synced_at)),
                    server: snapshot.server,
                  })}
                </>
              )}
            </div>
          </div>
        ) : (
          <div className="notes-empty">
            <div className="notes-empty-icon">
              <Icon name="database" size={30} />
            </div>
            <h2 className="notes-empty-title">{t("erd.title")}</h2>
            <p className="notes-empty-sub">
              {connections && connections.length === 0 ? t("erd.empty.noConnection") : t("erd.empty.pick")}
            </p>
            {connections && connections.length === 0 && (
              <div className="notes-empty-actions">
                <Button variant="primary" size="sm" onClick={() => setDbModal({ open: true, connection: null })}>
                  <Icon name="database" size={14} />
                  {t("erd.empty.connect")}
                </Button>
              </div>
            )}
          </div>
        )}
      </section>

      {/* 연결 끊기 — 지워지는 것(프로필, 비밀번호, 스냅샷)을 그 자리에서 말한다 */}
      <Modal
        open={!!confirmDisconnect}
        title={t("erd.db.disconnect.title")}
        narrow
        onClose={() => setConfirmDisconnect(null)}
        footer={
          <>
            <Button size="sm" onClick={() => setConfirmDisconnect(null)} disabled={busy}>
              {t("common.cancel")}
            </Button>
            <Button variant="danger" size="sm" onClick={() => void doDisconnect()} disabled={busy}>
              {busy ? t("erd.db.disconnect.working") : t("erd.db.disconnect.confirm")}
            </Button>
          </>
        }
      >
        <p style={{ margin: 0 }}>
          <b>{confirmDisconnect?.name}</b> {t("erd.db.disconnect.body")}
        </p>
      </Modal>

      {/* 예전 ERD 파일 정리 — 휴지통으로, 되돌릴 수 있다 */}
      <Modal
        open={confirmLegacy}
        title={t("erd.legacy.confirmTitle")}
        narrow
        onClose={() => setConfirmLegacy(false)}
        footer={
          <>
            <Button size="sm" onClick={() => setConfirmLegacy(false)} disabled={busy}>
              {t("common.cancel")}
            </Button>
            <Button variant="danger" size="sm" onClick={() => void doTrashLegacy()} disabled={busy}>
              {busy ? t("erd.legacy.working") : t("erd.legacy.confirm")}
            </Button>
          </>
        }
      >
        <p style={{ margin: 0 }}>{t("erd.legacy.confirmBody", { n: legacy?.erds ?? 0 })}</p>
      </Modal>

      {/* DB 연결 추가/편집 — 저장되면 스냅샷이 없는 스키마를 채운다 */}
      <DbConnectionModal
        open={dbModal.open}
        connection={dbModal.connection}
        onClose={() => setDbModal({ open: false, connection: null })}
        onSaved={(c) => {
          setExpanded((e) => new Set(e).add(connPath(c)));
          void loadConnections().then((r) => r && autoSyncRef.current(r.list, r.snaps));
        }}
      />
    </div>
  );
}
