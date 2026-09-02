// 설정 › 데이터베이스 — 연결 목록·상태. 추가/편집/삭제/비밀번호 입력 모달은 부모(SettingsModal)가
// 설정 모달의 형제로 띄운다(Modal 은 portal 이 아니라 모달 안에 모달을 넣으면 overflow 에 잘린다).

import { useCallback, useEffect, useState } from "react";
import { Spinner, Tooltip, timeAgo } from "../ui";
import { Icon } from "../icons";
import { t } from "../lib/i18n";
import {
  DB_CONNECTIONS_EVENT,
  connStatus,
  connStatusDot,
  connStatusLabel,
  envLabel,
  listConnections,
  type DbConnection,
} from "../lib/dbconn";

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

  const reload = useCallback(async () => {
    setList(await listConnections());
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
            const st = connStatus(c);
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
                    {/* 점이 색으로 상태를 말하고 바로 옆 글자가 같은 것을 단어로 말한다 — 색만으로
                        정보를 전하지 않는다(DESIGN §2 결과 색) */}
                    <span className={`db-dot ${connStatusDot(st)}`} />
                    {connStatusLabel(st)}
                    {/* 마지막 시도 시각은 성공이든 실패든 '언제 기준의 상태인가'를 말해 준다 */}
                    {(st === "connected" || st === "failed") &&
                      c.last_sync_at &&
                      ` · ${t("diagrams.db.syncAgo", { ago: timeAgo(c.last_sync_at) })}`}
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
