// DB 연결 모달 — 접속 정보(1/2) → 스키마 선택(2/2). 생성·편집 공용, 다이어그램 트리와 설정 양쪽에서 연다.
//
// 비밀번호는 저장 순간 db_secret_set 으로 한 번만 Rust 에 넘긴다 — 프로필(SQLite)엔 절대 들어가지 않고,
// 편집 모달은 값 대신 "키체인에 저장됨"만 보여준다.
// 동기화(스냅샷·ERD 생성)는 여기서 하지 않는다: 프로필과 폴더만 만들고 onSaved 로 넘기면 DiagramsView 가
// 트리 행마다 진행을 보여주며 스키마를 순서대로 읽는다 — 모달이 열린 채 기다리게 하지 않는다.

import { useEffect, useState, type ReactNode } from "react";
import { Checkbox, Modal, Select, Tooltip } from "../ui";
import { Icon } from "../icons";
import { t } from "../lib/i18n";
import { errText } from "../lib/errors";
import { diagramFileExists, ensureDiagramDir } from "../lib/diagrams";
import { rootDisplayName } from "../lib/workspace";
import {
  ENVS,
  createConnection,
  dbSecretSet,
  dbTest,
  deleteConnection,
  envLabel,
  folderNameFor,
  getConnection,
  notifyConnectionsChanged,
  updateConnection,
  type DbConnection,
  type DbEnv,
  type DbSchemaPref,
  type DbTestResult,
  type DbTls,
} from "../lib/dbconn";

type Step = "form" | "schemas";

/** 비밀번호 입력 + 보기/숨기기 토글. 키체인에만 저장되는 값이라 확인할 길이 이 칸밖에 없다 */
export function PasswordField({
  value,
  show,
  onToggle,
  onChange,
  autoFocus,
}: {
  value: string;
  show: boolean;
  onToggle: () => void;
  onChange: (v: string) => void;
  autoFocus?: boolean;
}) {
  return (
    <div className="db-pw-field">
      <input
        className="input"
        type={show ? "text" : "password"}
        value={value}
        autoFocus={autoFocus}
        autoComplete="off"
        autoCapitalize="off"
        spellCheck={false}
        onChange={(e) => onChange(e.target.value)}
      />
      <div className="db-pw-toggle">
        <Tooltip label={show ? t("diagrams.db.password.hide") : t("diagrams.db.password.show")}>
          <button
            type="button"
            className="icon-btn ghost sm"
            aria-label={show ? t("diagrams.db.password.hide") : t("diagrams.db.password.show")}
            aria-pressed={show}
            onClick={onToggle}
          >
            <Icon name={show ? "eye-off" : "eye"} size={14} />
          </button>
        </Tooltip>
      </div>
    </div>
  );
}

const TLS_OPTIONS: { value: DbTls; label: string }[] = [
  { value: "preferred", label: t("diagrams.db.tls.preferred") },
  { value: "required", label: t("diagrams.db.tls.required") },
  { value: "disabled", label: t("diagrams.db.tls.disabled") },
];

// P1 은 MySQL 만. 목록이 하나라도 Select 로 두는 이유: Postgres 가 붙으면 자리만 늘리면 된다
const KIND_OPTIONS = [{ value: "mysql", label: "MySQL" }];

function portOf(s: string): number | null {
  if (!/^\d{1,5}$/.test(s.trim())) return null;
  const n = Number(s);
  return n >= 1 && n <= 65535 ? n : null;
}

export function DbConnectionModal({
  open,
  connection,
  onClose,
  onSaved,
}: {
  open: boolean;
  /** 편집 대상. null = 새 연결 */
  connection: DbConnection | null;
  onClose: () => void;
  /** 프로필이 저장된 뒤(새 연결이면 생성된 행). 호출부가 트리를 다시 읽고 동기화를 시작한다 */
  onSaved: (c: DbConnection) => void;
}) {
  const editing = connection !== null;
  const [step, setStep] = useState<Step>("form");
  const [name, setName] = useState("");
  const [env, setEnv] = useState<DbEnv>("dev");
  const [host, setHost] = useState("127.0.0.1");
  const [port, setPort] = useState("3306");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [tls, setTls] = useState<DbTls>("preferred");
  // 편집 모달에서 비밀번호 칸을 열었는가 — 기본은 닫힘("키체인에 저장됨")
  const [changePw, setChangePw] = useState(false);
  // 입력한 비밀번호를 평문으로 확인 — 열 때마다 숨김으로 돌아간다
  const [showPw, setShowPw] = useState(false);
  const [testing, setTesting] = useState(false);
  const [test, setTest] = useState<DbTestResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [prefs, setPrefs] = useState<DbSchemaPref[]>([]);
  const [busy, setBusy] = useState(false);
  const [prodConfirm, setProdConfirm] = useState(false);

  useEffect(() => {
    if (!open) return;
    setStep("form");
    setName(connection?.name ?? "");
    setEnv(connection?.env ?? "dev");
    setHost(connection?.host ?? "127.0.0.1");
    setPort(String(connection?.port ?? 3306));
    setUsername(connection?.username ?? "");
    setPassword("");
    setTls(connection?.tls ?? "preferred");
    setChangePw(false);
    setShowPw(false);
    setTesting(false);
    setTest(null);
    setError(null);
    setPrefs(connection?.schemas ?? []);
    setBusy(false);
    setProdConfirm(false);
  }, [open, connection]);

  // 접속 정보를 고치면 직전 테스트 결과는 다른 서버 얘기다 — 다시 테스트하게 한다
  function touch<T>(set: (v: T) => void) {
    return (v: T) => {
      set(v);
      setTest(null);
    };
  }

  const portNum = portOf(port);
  const fieldsValid =
    name.trim().length > 0 &&
    host.trim().length > 0 &&
    portNum !== null &&
    username.trim().length > 0 &&
    (editing && !changePw ? true : password.length > 0);
  const enabledCount = prefs.filter((p) => p.enabled).length;

  async function runTest() {
    if (!fieldsValid || testing) return;
    setTesting(true);
    setError(null);
    try {
      const r = await dbTest(
        {
          ulid: connection?.ulid ?? "new",
          kind: "mysql",
          host: host.trim(),
          port: portNum ?? 3306,
          username: username.trim(),
          tls,
        },
        // 편집 중 비밀번호를 안 바꿨으면 Rust 가 키체인에서 읽는다
        editing && !changePw ? null : password,
      );
      setTest(r);
      // 이미 있던 설정(표시명·체크)은 지키고, 새로 보인 스키마만 기본값으로 — 테이블이 있으면 켜 둔다
      setPrefs((prev) =>
        r.schemas.map((s) => prev.find((p) => p.name === s.name) ?? {
          name: s.name,
          label: "",
          enabled: s.tables > 0,
        }),
      );
    } catch (e) {
      setError(errText(e));
    } finally {
      setTesting(false);
    }
  }

  function goSchemas() {
    if (!test) return;
    if (env === "prod" && !editing) {
      setProdConfirm(true);
      return;
    }
    setStep("schemas");
  }

  function togglePref(name: string) {
    setPrefs((prev) => prev.map((p) => (p.name === name ? { ...p, enabled: !p.enabled } : p)));
  }

  function setLabel(name: string, label: string) {
    setPrefs((prev) => prev.map((p) => (p.name === name ? { ...p, label } : p)));
  }

  /** 새 연결 폴더 이름 — 같은 이름의 폴더가 있으면 -2, -3… 을 붙인다 */
  async function pickFolder(): Promise<string> {
    const base = folderNameFor(name);
    let folder = base;
    for (let i = 2; await diagramFileExists(folder); i++) folder = `${base}-${i}`;
    return folder;
  }

  async function save() {
    if (busy || !fieldsValid || portNum === null) return;
    if (!editing && enabledCount === 0) {
      setError(t("diagrams.db.schemas.none"));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      let saved: DbConnection;
      if (connection) {
        await updateConnection(connection.id, {
          name,
          env,
          host,
          port: portNum,
          username,
          tls,
          // 테스트를 안 했으면 스키마 목록은 손대지 않는다(연결 없이 이름만 고치는 경로)
          ...(test ? { schemas: prefs } : {}),
        });
        if (changePw && password) await dbSecretSet(connection.ulid, password);
        const fresh = await getConnection(connection.id);
        if (!fresh) throw new Error("connection vanished");
        saved = fresh;
      } else {
        const folder = await pickFolder();
        saved = await createConnection({
          name,
          kind: "mysql",
          env,
          host,
          port: portNum,
          username,
          tls,
          folder_path: folder,
          schemas: prefs,
        });
        try {
          await dbSecretSet(saved.ulid, password);
        } catch (e) {
          // 비밀번호 없는 프로필은 쓸모가 없다 — 행을 되돌려 반쪽 상태를 남기지 않는다
          await deleteConnection(saved).catch(() => {});
          throw e;
        }
      }
      await ensureDiagramDir(saved.folder_path);
      for (const p of saved.schemas.filter((p) => p.enabled)) {
        await ensureDiagramDir(`${saved.folder_path}/${p.name}`);
      }
      notifyConnectionsChanged();
      onSaved(saved);
      onClose();
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  }

  let footer: ReactNode;
  if (step === "form") {
    footer = (
      <>
        <button className="btn btn-sm" onClick={() => void runTest()} disabled={!fieldsValid || testing || busy}>
          {testing ? t("diagrams.db.testing") : t("diagrams.db.test")}
        </button>
        <span className="spacer" />
        <button className="btn btn-sm" onClick={onClose} disabled={busy}>
          {t("common.cancel")}
        </button>
        {editing ? (
          <>
            <button className="btn btn-sm" onClick={goSchemas} disabled={!test || busy}>
              {t("diagrams.db.next")}
              <Icon name="chevron-right" size={13} />
            </button>
            <button className="btn btn-primary btn-sm" onClick={() => void save()} disabled={!fieldsValid || busy}>
              {busy ? t("diagrams.working") : t("diagrams.db.saveChanges")}
            </button>
          </>
        ) : (
          <button className="btn btn-primary btn-sm" onClick={goSchemas} disabled={!test || busy}>
            {t("diagrams.db.next")}
            <Icon name="chevron-right" size={13} />
          </button>
        )}
      </>
    );
  } else {
    footer = (
      <>
        <button className="btn btn-sm" onClick={() => setStep("form")} disabled={busy}>
          <Icon name="chevron-left" size={13} />
          {t("diagrams.db.back")}
        </button>
        <span className="spacer" />
        <button className="btn btn-sm" onClick={onClose} disabled={busy}>
          {t("common.cancel")}
        </button>
        <button
          className="btn btn-primary btn-sm"
          onClick={() => void save()}
          disabled={busy || (!editing && enabledCount === 0)}
        >
          {busy
            ? t("diagrams.working")
            : editing
              ? t("diagrams.db.saveChanges")
              : t("diagrams.db.connectAndSync", { n: enabledCount })}
        </button>
      </>
    );
  }

  const envOptions = ENVS.map((e) => ({ value: e, label: envLabel(e) }));
  const folderPreview = connection?.folder_path ?? folderNameFor(name || "db");

  return (
    <>
      <Modal
        open={open}
        title={editing ? t("diagrams.db.modal.editTitle") : t("diagrams.db.modal.title")}
        onClose={onClose}
        footer={footer}
        fixedHeight
      >
        <div className="db-step-eyebrow">
          <span className="set-eyebrow">
            {step === "form" ? t("diagrams.db.modal.step1") : t("diagrams.db.modal.step2")}
          </span>
        </div>

        {error && (
          <div className="error-note" style={{ marginBottom: 12 }}>
            {error}
          </div>
        )}

        {step === "form" && (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void runTest();
            }}
          >
            <div className="db-grid">
              <div className="field">
                <label>{t("diagrams.db.field.name")}</label>
                <input
                  className="input"
                  autoFocus={!editing}
                  value={name}
                  placeholder={t("diagrams.db.field.namePh")}
                  onChange={(e) => setName(e.target.value)}
                />
              </div>
              <div className="field">
                <label>{t("diagrams.db.field.env")}</label>
                <Select value={env} options={envOptions} onChange={setEnv} block />
              </div>
            </div>
            <div className="db-grid">
              <div className="field">
                <label>{t("diagrams.db.field.host")}</label>
                <input
                  className="input"
                  value={host}
                  spellCheck={false}
                  onChange={(e) => touch(setHost)(e.target.value)}
                />
                <div className="hint">{t("diagrams.db.hint.host")}</div>
              </div>
              <div className="field">
                <label>{t("diagrams.db.field.port")}</label>
                <input
                  className="input"
                  value={port}
                  inputMode="numeric"
                  onChange={(e) => touch(setPort)(e.target.value)}
                />
              </div>
            </div>
            <div className="db-grid half">
              <div className="field">
                <label>{t("diagrams.db.field.user")}</label>
                <input
                  className="input"
                  value={username}
                  spellCheck={false}
                  autoCapitalize="off"
                  onChange={(e) => touch(setUsername)(e.target.value)}
                />
              </div>
              <div className="field">
                <label>{t("diagrams.db.field.password")}</label>
                {editing && !changePw ? (
                  <div className="db-pw-stored">
                    <Icon name="key" size={14} />
                    <span>{t("diagrams.db.password.stored")}</span>
                    <span className="spacer" />
                    <button type="button" className="btn btn-sm" onClick={() => setChangePw(true)}>
                      {t("diagrams.db.password.change")}
                    </button>
                  </div>
                ) : (
                  <PasswordField
                    value={password}
                    show={showPw}
                    onToggle={() => setShowPw((v) => !v)}
                    onChange={touch(setPassword)}
                  />
                )}
                <div className="hint">{t("diagrams.db.hint.password")}</div>
              </div>
            </div>
            <div className="db-grid half">
              <div className="field">
                <label>{t("diagrams.db.field.kind")}</label>
                <Select value="mysql" options={KIND_OPTIONS} onChange={() => {}} block />
              </div>
              <div className="field">
                <label>{t("diagrams.db.field.tls")}</label>
                <Select value={tls} options={TLS_OPTIONS} onChange={touch(setTls)} block />
              </div>
            </div>
            <div className="hint" style={{ marginBottom: 12 }}>
              {t("diagrams.db.hint.readonly")}
            </div>
            {test && (
              <div className="ok-note">
                {t("diagrams.db.testOk", {
                  server: test.server,
                  n: test.schemas.length,
                  ms: test.latency_ms,
                })}
              </div>
            )}
            {/* Enter 로 테스트가 돌게 하는 보이지 않는 submit */}
            <button type="submit" hidden />
          </form>
        )}

        {step === "schemas" && test && (
          <>
            <div className="db-kv" style={{ marginBottom: 12 }}>
              <span>
                {t("diagrams.db.schemas.summary", {
                  name: name.trim(),
                  host: host.trim(),
                  port,
                  user: username.trim(),
                })}
              </span>
              <span>·</span>
              <span>{t("diagrams.db.schemas.readable", { n: test.schemas.length })}</span>
            </div>
            <div className="db-panel">
              <div className="db-panel-head db-schema-cols">
                <span>{t("diagrams.db.schemas.col.schema")}</span>
                <span className="spacer" />
                <span>{t("diagrams.db.schemas.col.label")}</span>
                <span className="db-num-head">{t("diagrams.db.schemas.col.tables")}</span>
              </div>
              <div className="db-list">
                {test.schemas.map((s) => {
                  const p = prefs.find((x) => x.name === s.name);
                  const on = p?.enabled ?? false;
                  return (
                    <div key={s.name} className={`db-schema-row ${on ? "" : "off"}`}>
                      <Checkbox checked={on} onChange={() => togglePref(s.name)} label={s.name} />
                      <span className="db-row-name" onClick={() => togglePref(s.name)}>
                        {s.name}
                      </span>
                      <input
                        className="input db-label-input"
                        value={p?.label ?? ""}
                        placeholder={t("diagrams.db.schemas.labelPh")}
                        disabled={!on}
                        onChange={(e) => setLabel(s.name, e.target.value)}
                      />
                      <span className="db-num">{s.tables}</span>
                    </div>
                  );
                })}
              </div>
            </div>
            <div className="field" style={{ marginTop: 14 }}>
              <label>{t("diagrams.db.folder.label")}</label>
              <div className="db-folder">
                <Icon name="folder" size={14} />
                <span>
                  {rootDisplayName("diagrams")} / <b>{folderPreview}</b>
                </span>
                <span className="spacer" />
                <span className="set-eyebrow">{t("diagrams.db.folder.perSchema")}</span>
              </div>
              <div className="hint">{t("diagrams.db.folder.hint")}</div>
            </div>
          </>
        )}
      </Modal>

      {/* 운영 연결 첫 추가 — 한 번만 묻는다 */}
      <Modal
        open={prodConfirm}
        title={t("diagrams.db.prod.confirmTitle")}
        onClose={() => setProdConfirm(false)}
        narrow
        footer={
          <>
            <button className="btn btn-sm" onClick={() => setProdConfirm(false)}>
              {t("common.cancel")}
            </button>
            <button
              className="btn btn-primary btn-sm"
              onClick={() => {
                setProdConfirm(false);
                setStep("schemas");
              }}
            >
              {t("diagrams.db.prod.confirmOk")}
            </button>
          </>
        }
      >
        <p style={{ margin: 0 }}>{t("diagrams.db.prod.confirmBody")}</p>
      </Modal>
    </>
  );
}
