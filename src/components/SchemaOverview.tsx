// 스키마 개요 — ERD 탭에서 스키마의 [개요] 보기. 머리줄(이름, 동기화)은 ERD 보기와 함께 쓰므로 ErdView 가 갖는다.
// 스냅샷(schema.json)으로 그린다 — 연결이 끊긴 채로도 마지막 구조가 보인다.

import { useMemo, useState } from "react";
import { Spinner } from "../ui";
import { Icon } from "../icons";
import { t } from "../lib/i18n";
import { formatHeaderTime, type SchemaSnapshot } from "../lib/schemaSnapshot";
import { inferReferences } from "../lib/erdGen";

const isAudit = (name: string) => name.endsWith("_aud");

/** 행 수 추정치 — 1.2M · 86K 처럼 짧게 */
function compact(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(n >= 10_000_000 ? 0 : 1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(n >= 10_000 ? 0 : 1)}K`;
  return String(n);
}

export function SchemaOverview({
  snapshot,
  syncing,
  onPickTable,
}: {
  snapshot: SchemaSnapshot | null;
  syncing: boolean;
  /** 테이블 행을 고르면 ERD 보기에서 그 표를 연다 */
  onPickTable: (table: string) => void;
}) {
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
    <div className="db-ov">
      {!snapshot ? (
        <div className="db-ov-empty">
          {syncing ? <Spinner /> : <Icon name="table" size={22} />}
          <p>{syncing ? t("erd.db.syncingShort") : t("erd.db.overview.noSnapshot")}</p>
        </div>
      ) : (
        <>
          <div className="db-kv">
            <span>{t("erd.db.overview.stats", { tables: stats!.tables, columns: stats!.columns })}</span>
            <span>·</span>
            <span>{t("erd.db.overview.audit", { n: stats!.audit })}</span>
            <span>·</span>
            <span>
              {t("erd.db.overview.snapshot", {
                time: formatHeaderTime(new Date(snapshot.synced_at)),
                server: snapshot.server,
              })}
            </span>
          </div>

          <div className="db-ov-grid">
            <div className="db-panel">
              <div className="db-panel-head">
                <span className="set-eyebrow">{t("erd.db.overview.tableList")}</span>
                <span className="spacer" />
                <div className="db-search">
                  <Icon name="search" size={12} />
                  <input
                    className="db-search-input"
                    value={query}
                    placeholder={t("erd.db.overview.searchPh")}
                    onChange={(e) => setQuery(e.target.value)}
                  />
                </div>
              </div>
              <div className="db-list scroll">
                {rows.length === 0 && <div className="db-row-empty">{t("erd.db.overview.empty")}</div>}
                {rows.map((tb) => (
                  <div
                    key={tb.name}
                    className={`db-row db-row-pick ${isAudit(tb.name) ? "audit" : ""}`}
                    onClick={() => onPickTable(tb.name)}
                  >
                    <div className="db-row-main">
                      <div className="db-row-name">{tb.name}</div>
                      <div className="db-row-sub">
                        {tb.comment || t("erd.db.overview.noComment")}
                      </div>
                    </div>
                    <span className="db-num">{t("erd.db.overview.cols", { n: tb.columns.length })}</span>
                    <span className="db-num">
                      {tb.rows_estimate !== null
                        ? t("erd.db.overview.rows", { n: compact(tb.rows_estimate) })
                        : ""}
                    </span>
                  </div>
                ))}
              </div>
            </div>

            <div className="db-ov-side">
              <div className="db-stats">
                <div className="db-stat">
                  <div className="db-stat-l">{t("erd.db.stat.physical")}</div>
                  <div className="db-stat-v">{stats!.physical}</div>
                </div>
                <div className="db-stat">
                  <div className="db-stat-l">{t("erd.db.stat.logical")}</div>
                  <div className="db-stat-v">{stats!.logical}</div>
                </div>
                <div className="db-stat">
                  <div className="db-stat-l">{t("erd.db.stat.unresolved")}</div>
                  <div className="db-stat-v">{stats!.unresolved}</div>
                </div>
              </div>
              {refs && refs.unresolved.length > 0 && (
                <div className="db-panel">
                  <div className="db-panel-head">
                    <span className="set-eyebrow">{t("erd.db.stat.unresolvedList")}</span>
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
                  <div className="hint db-panel-foot">{t("erd.db.stat.unresolvedHint")}</div>
                </div>
              )}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
