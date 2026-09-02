// 스키마 개요 — 다이어그램 트리에서 연결의 스키마 폴더를 클릭했을 때 우측 pane.
// 스냅샷(schema.json)으로 그린다 — 연결이 끊긴 채로도 마지막 구조가 보인다.
// 주 행동은 하나: [전체 ERD 생성](없을 때) 또는 [ERD 열기](있을 때). 재생성은 ERD 화면의 변경 배너에서 한다.

import { useMemo, useState } from "react";
import { Checkbox, Spinner, Tooltip, timeAgo } from "../ui";
import { Icon } from "../icons";
import { t } from "../lib/i18n";
import { rootDisplayName } from "../lib/workspace";
import { envLabel, prefAudit, schemaFolder, type DbConnection, type DbSchemaPref } from "../lib/dbconn";
import { SNAPSHOT_FILE, formatHeaderTime, type SchemaSnapshot } from "../lib/schemaSnapshot";
import { inferReferences } from "../lib/erdGen";

const isAudit = (name: string) => name.endsWith("_aud");

/** 행 수 추정치 — 1.2M · 86K 처럼 짧게 */
function compact(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(n >= 10_000_000 ? 0 : 1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(n >= 10_000 ? 0 : 1)}K`;
  return String(n);
}

export function SchemaOverview({
  conn,
  pref,
  snapshot,
  syncing,
  error,
  hasFullErd,
  onSync,
  onGenerate,
  onOpenFull,
  onToggleAudit,
}: {
  conn: DbConnection;
  pref: DbSchemaPref;
  snapshot: SchemaSnapshot | null;
  syncing: boolean;
  error: string | null;
  hasFullErd: boolean;
  onSync: () => void;
  onGenerate: () => void;
  onOpenFull: () => void;
  /** 감사 테이블 포함 토글 — 값은 스키마 설정에 저장되고 생성·재생성이 따른다 */
  onToggleAudit: (audit: boolean) => void;
}) {
  const audit = prefAudit(pref);
  const [query, setQuery] = useState("");

  const refs = useMemo(() => (snapshot ? inferReferences(snapshot) : null), [snapshot]);
  const stats = useMemo(() => {
    if (!snapshot) return null;
    const tables = snapshot.tables.length;
    const columns = snapshot.tables.reduce((n, tb) => n + tb.columns.length, 0);
    const audit = snapshot.tables.filter((tb) => isAudit(tb.name)).length;
    // Envers 의 rev → revinfo 물리 FK 는 도메인 관계가 아니다 — 다이어그램에서도 선으로 그리지 않으니 통계에서도 뺀다
    const domain = (refs?.edges ?? []).filter((e) => !isAudit(e.from) && e.to !== "revinfo");
    const physical = domain.filter((e) => e.physical).length;
    const logical = domain.length - physical;
    return { tables, columns, audit, physical, logical, unresolved: refs?.unresolved.length ?? 0 };
  }, [snapshot, refs]);

  const rows = useMemo(() => {
    if (!snapshot) return [];
    const q = query.trim().toLowerCase();
    // 감사 테이블은 목록 뒤로 — 도메인 테이블을 먼저 훑게
    const sorted = [...snapshot.tables].sort((a, b) => {
      const aa = isAudit(a.name) ? 1 : 0;
      const bb = isAudit(b.name) ? 1 : 0;
      return aa - bb || a.name.localeCompare(b.name);
    });
    if (!q) return sorted;
    return sorted.filter(
      (tb) =>
        tb.name.toLowerCase().includes(q) ||
        tb.comment.toLowerCase().includes(q) ||
        tb.columns.some((c) => c.name.toLowerCase().includes(q) || c.comment.toLowerCase().includes(q)),
    );
  }, [snapshot, query]);

  return (
    <div className="notes-detail editing db-ov-wrap">
      <div className="dgm-head">
        <div className="dgm-head-info">
          <div className="note-crumb">
            <Icon name="database" size={12} />
            {[t("diagrams.title"), conn.name].join(" › ")}
          </div>
          <h1 className="dgm-title db-title">
            {pref.name}
            {pref.label && <span className="chip chip-sm">{pref.label}</span>}
            <span className={`chip chip-sm ${conn.env === "prod" ? "chip-fill" : ""}`}>{envLabel(conn.env)}</span>
          </h1>
        </div>
        <span className="spacer" />
        <Tooltip label={t("diagrams.db.audit.hint")}>
          <span className={`db-audit-toggle ${syncing ? "disabled" : ""}`}>
            <Checkbox
              checked={audit}
              onChange={() => !syncing && onToggleAudit(!audit)}
              label={t("diagrams.db.audit.include")}
            />
            <span onClick={() => !syncing && onToggleAudit(!audit)}>{t("diagrams.db.audit.include")}</span>
          </span>
        </Tooltip>
        <button className="btn btn-sm" onClick={onSync} disabled={syncing}>
          <Icon name="refresh" size={14} />
          {syncing
            ? t("diagrams.db.syncingShort")
            : snapshot
              ? t("diagrams.db.syncAgo", { ago: timeAgo(snapshot.synced_at) })
              : t("diagrams.db.sync")}
        </button>
        <button
          className="btn btn-primary btn-sm"
          onClick={hasFullErd ? onOpenFull : onGenerate}
          disabled={!snapshot || syncing}
        >
          <Icon name="workflow" size={14} />
          {hasFullErd ? t("diagrams.db.openFull") : t("diagrams.db.generateFull")}
        </button>
      </div>

      <div className="db-ov">
        {error && <div className="error-note">{error}</div>}

        {!snapshot ? (
          <div className="db-ov-empty">
            {syncing ? <Spinner /> : <Icon name="table" size={22} />}
            <p>{syncing ? t("diagrams.db.syncingShort") : t("diagrams.db.overview.noSnapshot")}</p>
          </div>
        ) : (
          <>
            <div className="db-kv">
              <span>{t("diagrams.db.overview.stats", { tables: stats!.tables, columns: stats!.columns })}</span>
              <span>·</span>
              <span>{t("diagrams.db.overview.audit", { n: stats!.audit })}</span>
              <span>·</span>
              <span>
                {t("diagrams.db.overview.snapshot", {
                  time: formatHeaderTime(new Date(snapshot.synced_at)),
                  server: snapshot.server,
                })}
              </span>
            </div>

            <div className="db-ov-grid">
              <div className="db-panel">
                <div className="db-panel-head">
                  <span className="set-eyebrow">{t("diagrams.db.overview.tableList")}</span>
                  <span className="spacer" />
                  <div className="db-search">
                    <Icon name="search" size={12} />
                    <input
                      className="db-search-input"
                      value={query}
                      placeholder={t("diagrams.db.overview.searchPh")}
                      onChange={(e) => setQuery(e.target.value)}
                    />
                  </div>
                </div>
                <div className="db-list scroll">
                  {rows.length === 0 && <div className="db-row-empty">{t("diagrams.db.overview.empty")}</div>}
                  {rows.map((tb) => (
                    <div key={tb.name} className={`db-row ${isAudit(tb.name) ? "audit" : ""}`}>
                      <div className="db-row-main">
                        <div className="db-row-name">{tb.name}</div>
                        <div className="db-row-sub">
                          {tb.comment || t("diagrams.db.overview.noComment")}
                        </div>
                      </div>
                      <span className="db-num">{t("diagrams.db.overview.cols", { n: tb.columns.length })}</span>
                      <span className="db-num">
                        {tb.rows_estimate !== null
                          ? t("diagrams.db.overview.rows", { n: compact(tb.rows_estimate) })
                          : ""}
                      </span>
                    </div>
                  ))}
                </div>
              </div>

              <div className="db-ov-side">
                <div className="db-stats">
                  <div className="db-stat">
                    <div className="db-stat-l">{t("diagrams.db.stat.physical")}</div>
                    <div className="db-stat-v">{stats!.physical}</div>
                  </div>
                  <div className="db-stat">
                    <div className="db-stat-l">{t("diagrams.db.stat.logical")}</div>
                    <div className="db-stat-v">{stats!.logical}</div>
                  </div>
                  <div className="db-stat">
                    <div className="db-stat-l">{t("diagrams.db.stat.unresolved")}</div>
                    <div className="db-stat-v">{stats!.unresolved}</div>
                  </div>
                </div>
                {refs && refs.unresolved.length > 0 && (
                  <div className="db-panel">
                    <div className="db-panel-head">
                      <span className="set-eyebrow">{t("diagrams.db.stat.unresolvedList")}</span>
                    </div>
                    <div className="db-list scroll">
                      {refs.unresolved.map((u) => (
                        <div key={`${u.table}.${u.column}`} className="db-row db-row-tight">
                          <div className="db-row-main">
                            <div className="db-row-name">
                              {u.table}
                              <span className="db-row-col">.{u.column}</span>
                            </div>
                          </div>
                        </div>
                      ))}
                    </div>
                    <div className="hint db-panel-foot">{t("diagrams.db.stat.unresolvedHint")}</div>
                  </div>
                )}
              </div>
            </div>
          </>
        )}
      </div>

      <div className="detail-meta db-ov-meta">
        {rootDisplayName("diagrams")}/{schemaFolder(conn, pref.name)}
        {snapshot && (
          <>
            {" · "}
            {SNAPSHOT_FILE} {formatHeaderTime(new Date(snapshot.synced_at))}
          </>
        )}
      </div>
    </div>
  );
}
