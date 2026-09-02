// 설정 › 데이터베이스 — 연결 목록·상태. 추가/편집/삭제/비밀번호 입력 모달은 부모(SettingsModal)가
// 설정 모달의 형제로 띄운다(Modal 은 portal 이 아니라 모달 안에 모달을 넣으면 overflow 에 잘린다).

import { useCallback, useEffect, useState } from "react";
import { Spinner, Tooltip, timeAgo } from "../ui";
import { Icon } from "../icons";
import { t } from "../lib/i18n";
import {
  DB_CONNECTIONS_EVENT,
  dbSecretExists,
  envLabel,
  listConnections,
  type DbConnection,
} from "../lib/dbconn";

type Status = "connected" | "failed" | "needsPassword" | "never";

function statusOf(c: DbConnection, hasSecret: boolean): Status {
  if (!hasSecret) return "needsPassword";
  if (c.last_error) return "failed";
  if (c.last_sync_at) return "connected";
  return "never";
}

export function DbSettings({
  onAdd,
  onEdit,
  onDelete,
  onEnterPassword,
  refreshKey,
  error,
}: {
  onAdd: () => void;
  onEdit: (c: DbConnection) => void;
  onDelete: (c: DbConnection) => void;
  onEnterPassword: (c: DbConnection) => void;
  /** 부모가 바꾸면 다시 읽는다(모달에서 저장한 뒤) */
  refreshKey: number;
  /** 부모 모달(삭제)에서 난 오류 */
  error?: string | null;
}) {
  const [list, setList] = useState<DbConnection[] | null>(null);
  const [secrets, setSecrets] = useState<Record<string, boolean>>({});

  const reload = useCallback(async () => {
    const rows = await listConnections();
    setList(rows);
    const entries = await Promise.all(
      rows.map(async (c) => [c.ulid, await dbSecretExists(c.ulid).catch(() => true)] as const),
    );
    setSecrets(Object.fromEntries(entries));
  }, []);

  useEffect(() => {
    void reload();
  }, [reload, refreshKey]);

  useEffect(() => {
    const h = () => void reload();
    window.addEventListener(DB_CONNECTIONS_EVENT, h);
    return () => window.removeEventListener(DB_CONNECTIONS_EVENT, h);
  }, [reload]);

  return (
    <section className="set-section">
      <div className="set-head">
        <span className="set-eyebrow">{t("settings.db.title")}</span>
        <span className="spacer" />
        <button className="btn btn-sm" onClick={onAdd}>
          <Icon name="plus" size={13} />
          {t("settings.db.add")}
        </button>
      </div>

      {error && (
        <div className="error-note" style={{ marginBottom: 12 }}>
          {error}
        </div>
      )}
      {list === null && <Spinner />}
      {list && list.length === 0 && <p className="set-desc">{t("settings.db.none")}</p>}

      {list && list.length > 0 && (
        <div className="db-conn-list">
          {list.map((c) => {
            const st = statusOf(c, secrets[c.ulid] ?? true);
            return (
              <div key={c.id} className={`db-conn-row ${st === "needsPassword" ? "needs-pw" : ""}`}>
                <Icon name={st === "needsPassword" ? "key" : "database"} size={16} className="db-conn-ico" />
                <div className="db-conn-main">
                  <div className="db-conn-name">
                    {c.name}
                    <span className={`chip chip-sm ${c.env === "prod" ? "chip-fill" : ""}`}>{envLabel(c.env)}</span>
                  </div>
                  <div className="db-conn-sub">
                    {c.kind} · {c.host}:{c.port} · {c.username} ·{" "}
                    {t("settings.db.schemas", { n: c.schemas.filter((s) => s.enabled).length })} ·{" "}
                    {t("settings.db.folder", { path: c.folder_path })}
                  </div>
                </div>
                <div className="db-conn-side">
                  <div className="db-conn-status">
                    {st !== "needsPassword" && (
                      <span className={`db-dot ${st === "connected" ? "on" : ""}`} />
                    )}
                    {st === "connected" && (
                      <>
                        {t("diagrams.db.status.connected")}
                        {c.last_sync_at && ` · ${t("diagrams.db.syncAgo", { ago: timeAgo(c.last_sync_at) })}`}
                      </>
                    )}
                    {st === "failed" && (
                      <>
                        {t("diagrams.db.status.failed")}
                        {c.last_sync_at && ` · ${t("diagrams.db.syncAgo", { ago: timeAgo(c.last_sync_at) })}`}
                      </>
                    )}
                    {st === "never" && t("diagrams.db.status.never")}
                    {st === "needsPassword" && t("diagrams.db.status.needsPassword")}
                  </div>
                  <div className="db-conn-actions">
                    {st === "needsPassword" && (
                      <button className="btn btn-sm" onClick={() => onEnterPassword(c)}>
                        {t("settings.db.enterPassword")}
                      </button>
                    )}
                    <button className="btn btn-sm" onClick={() => onEdit(c)}>
                      {t("settings.db.edit")}
                    </button>
                    <Tooltip label={t("settings.db.deleteTitle")}>
                      <button
                        className="btn btn-sm btn-danger-ghost"
                        aria-label={t("settings.db.deleteTitle")}
                        onClick={() => onDelete(c)}
                      >
                        <Icon name="trash" size={13} />
                      </button>
                    </Tooltip>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}

      <p className="hint" style={{ marginTop: 14, lineHeight: 1.6 }}>
        {t("settings.db.hint")}
        <br />
        {t("diagrams.db.hint.readonly")}
      </p>
    </section>
  );
}
