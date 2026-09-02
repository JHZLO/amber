import { useCallback, useEffect, useRef, useState } from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { appDataDir } from "@tauri-apps/api/path";
import { emitTo } from "@tauri-apps/api/event";
import { createBackup } from "../lib/backup";
import type { AiProvider, AppConfig } from "../lib/config";
import {
  PROVIDER_LABELS,
  PROVIDER_MODELS,
  connectProvider,
  loadConfig,
  saveConfig,
} from "../lib/config";
import { aiHealth, detectAiClis, type DetectedCli } from "../lib/ai";
import { aiAuthStatus, type AuthStatus } from "../lib/auth";
import {
  loadPrompts,
  makePrompt,
  savePrompts,
  type SavedPrompt,
} from "../lib/prompts";
import { getThemePref, setThemePref, type ThemePref } from "../lib/theme";
import {
  getLang,
  setLang,
  t,
  LANG_CHANGED_EVENT,
  type AiLang,
  type Lang,
} from "../lib/i18n";
import { errText } from "../lib/errors";
import { Modal, Select, Spinner, Tooltip } from "../ui";
import { Icon } from "../icons";
import { ReportSettings } from "./ReportSettings";
import { AiAuthModal } from "./AiAuthModal";
import { DbSettings } from "./DbSettings";
import { DbConnectionModal, PasswordField } from "./DbConnectionModal";
import {
  dbSecretSet,
  deleteConnection,
  notifyConnectionsChanged,
  type DbConnection,
} from "../lib/dbconn";

const THEMES: { id: ThemePref; label: string }[] = [
  { id: "system", label: t("settings.theme.system") },
  { id: "light", label: t("settings.theme.light") },
  { id: "dark", label: t("settings.theme.dark") },
];

// 언어 선택지 — 라벨은 각 언어의 자기 표기(autonym) 그대로. 절대 번역하지 않는다.
const LANGS: { id: Lang; label: string }[] = [
  { id: "ko", label: "한국어" },
  { id: "en", label: "English" },
];

// 설정 카테고리 탭 — 한 화면에 다 쌓지 않고 갈래로 나눈다
type SetTab = "ai" | "prompts" | "report" | "databases" | "appearance";
const SETTING_TABS: { id: SetTab; label: string }[] = [
  { id: "ai", label: t("settings.tab.ai") },
  { id: "prompts", label: t("settings.tab.prompts") },
  { id: "report", label: t("settings.tab.report") },
  { id: "databases", label: t("settings.tab.databases") },
  { id: "appearance", label: t("settings.tab.appearance") },
];

export function SettingsModal({
  open,
  onClose,
  onSaved,
}: {
  open: boolean;
  onClose: () => void;
  onSaved: (c: AppConfig) => void;
}) {
  const [provider, setProvider] = useState<AiProvider | null>(null);
  const [path, setPath] = useState("");
  const [model, setModel] = useState("");
  // AI 응답 언어 — 'auto' 는 UI 언어를 따른다(lib/i18n.aiOutputLang)
  const [aiLang, setAiLang] = useState<AiLang>("auto");
  const [detected, setDetected] = useState<DetectedCli[] | null>(null);
  const [detecting, setDetecting] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; msg: string } | null>(
    null,
  );

  // CLI 로그인 상태 — 만료를 AI 기능이 멈추고 나서야 알게 되지 않도록 여기서 먼저 보여 준다
  const [auth, setAuth] = useState<AuthStatus | null>(null);
  const [authOpen, setAuthOpen] = useState(false);

  const [theme, setTheme] = useState<ThemePref>("system");
  const [tab, setTab] = useState<SetTab>("ai");

  // 데이터베이스 탭 — 추가/편집/삭제/비밀번호 모달은 설정 모달의 형제로 띄운다(Modal 은 portal 이 아니다)
  const [dbModal, setDbModal] = useState<{ open: boolean; connection: DbConnection | null }>({
    open: false,
    connection: null,
  });
  const [dbDelete, setDbDelete] = useState<DbConnection | null>(null);
  const [dbPw, setDbPw] = useState<DbConnection | null>(null);
  const [dbPwValue, setDbPwValue] = useState("");
  const [dbPwShow, setDbPwShow] = useState(false);
  const [dbPwBusy, setDbPwBusy] = useState(false);
  const [dbPwError, setDbPwError] = useState<string | null>(null);
  const [dbError, setDbError] = useState<string | null>(null);
  const [dbRefresh, setDbRefresh] = useState(0);

  async function removeDbConnection() {
    const c = dbDelete;
    if (!c) return;
    try {
      await deleteConnection(c);
      notifyConnectionsChanged();
      setDbRefresh((n) => n + 1);
      setDbError(null);
    } catch (e) {
      setDbError(errText(e));
    } finally {
      setDbDelete(null);
    }
  }

  async function saveDbPassword() {
    if (!dbPw || !dbPwValue || dbPwBusy) return;
    setDbPwBusy(true);
    setDbPwError(null);
    try {
      await dbSecretSet(dbPw.ulid, dbPwValue);
      notifyConnectionsChanged();
      setDbRefresh((n) => n + 1);
      setDbPw(null);
    } catch (e) {
      setDbPwError(errText(e));
    } finally {
      setDbPwBusy(false);
    }
  }

  // 언어는 페이지 로드 시 고정(i18n) — 선택 즉시 적용하지 않고 확인 모달을 거쳐
  // setLang 저장 후 창을 다시 불러온다. null = 확인 대기 없음
  const [langPending, setLangPending] = useState<Lang | null>(null);

  // 백업: 폴더 선택 → Rust 가 vault 사본 + DB 스냅샷 생성. 결과는 탭과 무관하게 본문 맨 위에 남긴다
  const [backingUp, setBackingUp] = useState(false);
  const [backupResult, setBackupResult] = useState<{
    ok: boolean;
    msg: string;
  } | null>(null);
  // 폴더 선택 다이얼로그는 앱을 막지 않아 그 사이에도 버튼을 또 누를 수 있다 — 상태보다 먼저 잠근다
  const backupLock = useRef(false);
  const backupNote = useRef<HTMLDivElement>(null);

  // 저장 프롬프트: 목록 + 포커스 에디터(한 번에 하나만 편집)
  const [prompts, setPrompts] = useState<SavedPrompt[]>([]);
  // editing !== null 이면 에디터 화면. isNew 는 취소 시 목록에 안 남기기 위함
  const [editing, setEditing] = useState<SavedPrompt | null>(null);
  const [isNew, setIsNew] = useState(false);
  const [editLabel, setEditLabel] = useState("");
  const [editText, setEditText] = useState("");

  // 언마운트 뒤 setState 를 막는 가드. **본문에서 true 로 되돌리는 줄이 반드시 있어야 한다** —
  // StrictMode(dev)는 마운트 직후 effect → cleanup → effect 를 한 번 더 돌리는데, cleanup 이
  // 내려둔 false 를 아무도 올리지 않으면 그 뒤 모든 비동기 결과가 조용히 버려진다
  // (CLI 감지·로그인 확인·연결 테스트·백업이 영원히 "…중" 에서 멈췄다).
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  useEffect(() => {
    if (open) {
      loadConfig().then((c) => {
        setProvider(c.provider);
        setPath(c.cliPath);
        setModel(c.model);
        setAiLang(c.aiLang);
        setTestResult(null);
      });
      setDetected(null);
      void redetect(); // 열자마자 설치된 CLI 를 감지해 카드로 보여준다
      loadPrompts().then(setPrompts);
      setEditing(null);
      setBackupResult(null);
      setTab("ai");
      setTheme(getThemePref());
      setLangPending(null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  function changeTheme(pref: ThemePref) {
    setTheme(pref);
    setThemePref(pref); // 즉시 적용
  }

  // 언어 적용 = 저장 → 위젯 창에 통지 → 이 창 리로드 (i18n 은 로드 시 고정이라 리로드가 적용)
  async function applyLang() {
    if (!langPending) return;
    setLang(langPending);
    // 위젯 창도 스스로 리로드하게 알린다 — 위젯이 없거나 실패해도 본창 적용엔 지장 없음
    await emitTo("widget", LANG_CHANGED_EVENT, {}).catch(() => {});
    window.location.reload();
  }

  // 목록 확정 = 상태 + 영속 동시에 (명시적 저장/삭제 시점에만)
  function commitPrompts(next: SavedPrompt[]) {
    setPrompts(next);
    void savePrompts(next);
  }
  function startNew() {
    setEditing(makePrompt());
    setIsNew(true);
    setEditLabel("");
    setEditText("");
  }
  function startEdit(p: SavedPrompt) {
    setEditing(p);
    setIsNew(false);
    setEditLabel(p.label);
    setEditText(p.text);
  }
  function cancelEdit() {
    setEditing(null);
  }
  function saveEdit() {
    if (!editing || !editText.trim()) return;
    const label = editLabel.trim() || editText.trim().slice(0, 24);
    const updated: SavedPrompt = { ...editing, label, text: editText.trim() };
    const next = isNew
      ? [...prompts, updated]
      : prompts.map((p) => (p.id === updated.id ? updated : p));
    commitPrompts(next);
    setEditing(null);
  }
  function removePrompt(id: string) {
    commitPrompts(prompts.filter((p) => p.id !== id));
  }

  const refreshAuth = useCallback(async () => {
    if (!provider) return;
    setAuth(null);
    try {
      const st = await aiAuthStatus(provider, path);
      if (alive.current) setAuth(st);
    } catch {
      // '알아내지 못했다'도 결과다 — null 로 남기면 확인 중 스피너가 영원히 돈다
      if (alive.current) setAuth({ supported: true, loggedIn: null, detail: "" });
    }
  }, [provider, path]);

  // 설정을 열 때·프로바이더를 바꿀 때 한 번. 경로 입력 중에는 다시 묻지 않는다(타이핑마다 프로세스를 띄우게 된다)
  useEffect(() => {
    if (!open || !provider) return;
    void refreshAuth();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, provider]);

  async function test() {
    setTesting(true);
    setTestResult(null);
    try {
      const v = await aiHealth(path);
      if (!alive.current) return;
      setTestResult({ ok: true, msg: t("settings.ai.testOk", { version: v }) });
    } catch (e) {
      if (!alive.current) return;
      setTestResult({ ok: false, msg: errText(e) });
    } finally {
      if (alive.current) setTesting(false);
    }
  }

  async function save() {
    // 응답 언어는 연결 여부와 무관하게 저장한다(saveConfig 가 provider 없어도 처리)
    await saveConfig({ provider, onboarded: true, cliPath: path.trim(), model, aiLang });
    if (provider) {
      const c = await connectProvider(provider, path.trim(), model);
      onSaved(c);
    }
    onClose();
  }

  // AI CLI 재감지 → 카드 목록 표시. 카드 클릭 = 해당 프로바이더로 전환
  async function redetect() {
    setDetecting(true);
    try {
      setDetected(await detectAiClis());
    } finally {
      if (alive.current) setDetecting(false);
    }
  }

  function pickDetected(d: DetectedCli) {
    setProvider(d.id as AiProvider);
    setPath(d.path);
    setModel(PROVIDER_MODELS[d.id as AiProvider][0]?.id ?? "");
    setTestResult(null);
  }

  async function backup() {
    if (backupLock.current) return;
    backupLock.current = true;
    try {
      const dir = await openDialog({
        directory: true,
        multiple: false,
        title: t("settings.backup.pickTitle"),
      });
      if (typeof dir !== "string" || !dir) return;
      setBackingUp(true);
      setBackupResult(null);
      const out = await createBackup(dir);
      if (!alive.current) return;
      setBackupResult({ ok: true, msg: t("settings.backup.done", { path: out }) });
    } catch (e) {
      // 노트가 탭 위에 따로 뜨므로 무엇이 실패했는지부터 밝힌다 (사유는 errText 가 번역)
      if (alive.current)
        setBackupResult({
          ok: false,
          msg: t("settings.backup.fail", { err: errText(e) }),
        });
    } finally {
      backupLock.current = false;
      if (alive.current) setBackingUp(false);
    }
  }

  // 탭 내용이 길어 아래로 스크롤돼 있으면 맨 위의 결과 노트를 못 보고 지나친다
  useEffect(() => {
    if (backupResult) backupNote.current?.scrollIntoView({ block: "nearest" });
  }, [backupResult]);

  async function openFolder() {
    try {
      await openPath(await appDataDir());
    } catch (e) {
      setTestResult({
        ok: false,
        msg: t("settings.openFolderFail", { err: errText(e) }),
      });
    }
  }

  // 에디터 화면일 땐 X/ESC/바깥클릭이 편집만 취소(한 단계 뒤로), 아니면 설정 닫기.
  // 언어 확인 모달이 위에 떠 있으면 그것만 닫는다 — Esc 는 두 모달 리스너에 모두 닿으므로
  // 여기서 가드하지 않으면 설정까지 한 번에 닫혀버린다.
  const handleClose = () =>
    langPending ? setLangPending(null) : editing ? cancelEdit() : onClose();

  const footer = editing ? (
    <>
      <button className="btn btn-sm" onClick={cancelEdit}>
        {t("common.cancel")}
      </button>
      <span className="spacer" />
      <button
        className="btn btn-primary"
        onClick={saveEdit}
        disabled={!editText.trim()}
      >
        {t("common.save")}
      </button>
    </>
  ) : (
    <>
      <button className="btn btn-sm" onClick={openFolder}>
        {t("settings.openDataFolder")}
      </button>
      <button
        className="btn btn-sm"
        onClick={() => void backup()}
        disabled={backingUp}
      >
        {backingUp ? t("settings.backup.busy") : t("settings.backup")}
      </button>
      <span className="spacer" />
      <button className="btn btn-sm" onClick={onClose}>
        {t("common.close")}
      </button>
      <button className="btn btn-primary" onClick={save} disabled={testing}>
        {t("common.save")}
      </button>
    </>
  );

  return (
    <>
    <Modal
      open={open}
      title={
        editing
          ? isNew
            ? t("settings.prompt.new")
            : t("settings.prompt.editTitle")
          : t("settings.title")
      }
      onClose={handleClose}
      footer={footer}
      fixedHeight
    >
      {editing ? (
        // 포커스 에디터 — 이름 + 큰 textarea 하나만
        <>
          <div className="field">
            <label>{t("settings.prompt.nameLabel")}</label>
            <input
              className="input"
              autoFocus
              placeholder={t("settings.prompt.namePlaceholder")}
              value={editLabel}
              onChange={(e) => setEditLabel(e.target.value)}
            />
          </div>
          <div className="field">
            <label>{t("settings.prompt.textLabel")}</label>
            <textarea
              className="textarea"
              style={{ fontFamily: "var(--font)" }}
              rows={10}
              placeholder={t("settings.prompt.textPlaceholder")}
              value={editText}
              onChange={(e) => setEditText(e.target.value)}
            />
            <div className="hint">{t("settings.prompt.nameHint")}</div>
          </div>
        </>
      ) : (
        <>
          {backupResult && (
            <div
              ref={backupNote}
              className={backupResult.ok ? "ok-note" : "error-note"}
              style={{ marginBottom: 12 }}
            >
              {backupResult.msg}
            </div>
          )}
          <div className="set-tabs">
            {SETTING_TABS.map((st) => (
              <button
                key={st.id}
                className={`set-tab ${tab === st.id ? "active" : ""}`}
                onClick={() => setTab(st.id)}
              >
                {st.label}
              </button>
            ))}
          </div>
          <div className="set-tab-content">
          {tab === "ai" && (
          <section className="set-section">
            <div className="set-head">
              <span className="set-eyebrow">{t("settings.ai.title")}</span>
              <span className="spacer" />
              <button
                className="btn btn-sm"
                onClick={() => void redetect()}
                disabled={detecting}
              >
                <Icon name="refresh" size={13} />
                {detecting ? t("settings.ai.detecting") : t("settings.ai.redetect")}
              </button>
            </div>
            <p className="set-desc">
              {provider
                ? t("settings.ai.connected", { name: PROVIDER_LABELS[provider] })
                : t("settings.ai.none")}
            </p>

            {/* 로그인 상태 — 위 문장이 "CLI 의 로그인 세션을 그대로 쓴다"고 말하는 그 세션의
                현재 상태다. 별도 'Sign-in' 필드로 떼어 두면 같은 얘기가 두 군데로 갈리고,
                버튼이 상태와 무관하게 늘 'Sign in' 이라 이미 로그인한 사람에게도 로그인을
                권하는 모양이 된다. **행동이 필요할 때만 버튼을 낸다**:
                  · 확인 중       → 상태만 (버튼 없음 — 아직 뭘 해야 할지 모른다)
                  · 로그인됨      → 체크 + 표준 버튼 '다시 로그인'(계정 교체용 탈출구)
                  · 만료됨        → 경고 + primary 버튼 (여기서만 눌러야 할 이유가 있다)
                  · 지원 안 함    → 터미널에서 로그인하라는 안내만 */}
            {provider && (
              <div className="set-auth">
                {auth === null ? (
                  <span className="set-auth-state">
                    <Spinner />
                    {t("settings.auth.checking")}
                  </span>
                ) : auth.supported === false ? (
                  <span className="set-auth-state">
                    {t("settings.auth.unsupported", {
                      name: PROVIDER_LABELS[provider],
                    })}
                  </span>
                ) : auth.loggedIn === true ? (
                  <>
                    <span className="set-auth-state ok">
                      <Icon name="check" size={13} />
                      {t("settings.auth.rowOk")}
                    </span>
                    <button
                      className="btn btn-sm"
                      onClick={() => setAuthOpen(true)}
                    >
                      {t("settings.auth.again")}
                    </button>
                  </>
                ) : auth.loggedIn === false ? (
                  <>
                    <span className="set-auth-state warn">
                      <Icon name="alert-triangle" size={13} />
                      {t("settings.auth.rowExpired")}
                    </span>
                    <button
                      className="btn btn-sm btn-primary"
                      onClick={() => setAuthOpen(true)}
                    >
                      {t("settings.auth.rowAction")}
                    </button>
                  </>
                ) : (
                  <span className="set-auth-state">
                    {t("settings.auth.rowUnknown")}
                  </span>
                )}
              </div>
            )}

            {detecting && detected === null ? (
              <div className="loading-box" style={{ padding: "22px 0" }}>
                <Spinner />
                <div className="hint">{t("settings.ai.searching")}</div>
              </div>
            ) : detected !== null && detected.length === 0 ? (
              <div className="error-note">{t("settings.ai.notFound")}</div>
            ) : (
              <div className="onb-grid">
                {(detected ?? []).map((d) => (
                  <button
                    key={d.id}
                    className={`onb-card ${provider === d.id ? "selected" : ""}`}
                    onClick={() => pickDetected(d)}
                  >
                    <span className="onb-dot" />
                    <span className="onb-name">{d.name}</span>
                    <span className="onb-version">{d.version}</span>
                    <span className="onb-path" title={d.path}>
                      {d.path}
                    </span>
                  </button>
                ))}
              </div>
            )}

            {provider && (
              <div className="set-provider">
                <div className="field">
                  <label>
                    {t("settings.ai.pathLabel", { name: PROVIDER_LABELS[provider] })}
                  </label>
                  <div className="set-inline">
                    <input
                      className="input"
                      value={path}
                      onChange={(e) => setPath(e.target.value)}
                      placeholder={`/opt/homebrew/bin/${provider}`}
                    />
                    <button className="btn" onClick={test} disabled={testing}>
                      {testing ? <Spinner /> : t("settings.ai.test")}
                    </button>
                  </div>
                  {testResult && (
                    <div
                      className={testResult.ok ? "ok-note" : "error-note"}
                      style={{ marginTop: 8 }}
                    >
                      {testResult.msg}
                    </div>
                  )}
                </div>

                <div className="field" style={{ marginBottom: 0 }}>
                  <label>{t("settings.ai.modelLabel")}</label>
                  <Select
                    block
                    value={model}
                    options={PROVIDER_MODELS[provider].map((m) => ({
                      value: m.id,
                      label: m.label,
                    }))}
                    onChange={setModel}
                  />
                  <div className="hint" style={{ marginTop: 6 }}>
                    {t("settings.ai.creditHint")}
                  </div>
                </div>

                {/* 응답 언어 — UI 언어와 따로 둔다. 기술 노트는 본문이 영어 식별자로
                    가득해서 예전엔 모델이 '입력은 영어'로 판단하고 한국어 UI 에서도 영어로
                    답했다. 이제 이 값이 절대 지시로 프롬프트에 박힌다(ai.rs lang_directive). */}
                <div className="field" style={{ marginBottom: 0, marginTop: 14 }}>
                  <label>{t("settings.ai.langLabel")}</label>
                  <Select<AiLang>
                    block
                    value={aiLang}
                    options={[
                      { value: "auto", label: t("settings.ai.langAuto") },
                      { value: "ko", label: "한국어" },
                      { value: "en", label: "English" },
                    ]}
                    onChange={setAiLang}
                  />
                  <div className="hint" style={{ marginTop: 6 }}>
                    {t("settings.ai.langHint")}
                  </div>
                </div>
              </div>
            )}
          </section>
          )}
          {tab === "report" && <ReportSettings />}
          {tab === "databases" && (
            <DbSettings
              onAdd={() => setDbModal({ open: true, connection: null })}
              onEdit={(c) => setDbModal({ open: true, connection: c })}
              onDelete={setDbDelete}
              onEnterPassword={(c) => {
                setDbPw(c);
                setDbPwValue("");
                setDbPwShow(false);
                setDbPwError(null);
              }}
              refreshKey={dbRefresh}
              error={dbError}
            />
          )}
          {tab === "appearance" && (
          <section className="set-section">
            <div className="set-head">
              <span className="set-eyebrow">{t("settings.tab.appearance")}</span>
            </div>
            <div className="field">
              <label>{t("settings.theme.label")}</label>
              <Select
                block
                value={theme}
                options={THEMES.map((th) => ({ value: th.id, label: th.label }))}
                onChange={changeTheme}
              />
            </div>
            <div className="field" style={{ marginBottom: 0 }}>
              <label>{t("settings.lang.label")}</label>
              {/* 테마와 달리 즉시 적용하지 않는다 — 리로드가 필요해 확인 모달을 거친다 */}
              <Select
                block
                value={getLang()}
                options={LANGS.map((l) => ({ value: l.id, label: l.label }))}
                onChange={(v) => {
                  if (v !== getLang()) setLangPending(v);
                }}
              />
            </div>
          </section>
          )}
          {tab === "prompts" && (
          <section className="set-section">
            <div className="set-head">
              <span className="set-eyebrow">{t("settings.prompts.title")}</span>
              <span className="spacer" />
              <button className="btn btn-sm" onClick={startNew}>
                <Icon name="plus" size={13} />
                {t("settings.prompt.new")}
              </button>
            </div>
            <p className="set-desc">
              {t("settings.prompts.desc.pre")}
              <b>{t("settings.prompts.desc.bold")}</b>
              {t("settings.prompts.desc.post")}
            </p>

            {prompts.length === 0 ? (
              <div className="prompt-empty">{t("settings.prompts.empty")}</div>
            ) : (
              <div className="prompt-list">
                {prompts.map((p) => (
                  <div className="prompt-row" key={p.id}>
                    <button
                      className="prompt-row-open"
                      onClick={() => startEdit(p)}
                      title={p.text}
                    >
                      {p.label.trim() || p.text.slice(0, 24)}
                    </button>
                    <Tooltip label={t("common.delete")}>
                      <button
                        aria-label={t("common.delete")}
                        className="icon-btn ghost sm danger prompt-del"
                        onClick={() => removePrompt(p.id)}
                      >
                        <Icon name="trash" size={14} />
                      </button>
                    </Tooltip>
                  </div>
                ))}
              </div>
            )}
          </section>
          )}
          </div>
        </>
      )}
    </Modal>

    {/* 언어 변경 확인 — 적용 = 리로드라서 즉시 바꾸지 않고 한 번 묻는다.
        설정 모달 위에 겹쳐 뜨는 좁은 확인 모달(나중에 마운트 = 위에 그려짐). */}
    <Modal
      open={langPending !== null}
      title={t("settings.lang.confirmTitle")}
      onClose={() => setLangPending(null)}
      narrow
      footer={
        <>
          <button className="btn btn-sm" onClick={() => setLangPending(null)}>
            {t("common.cancel")}
          </button>
          <button className="btn btn-primary btn-sm" onClick={() => void applyLang()}>
            {t("settings.lang.apply")}
          </button>
        </>
      }
    >
      <p style={{ margin: 0 }}>{t("settings.lang.confirmBody")}</p>
    </Modal>

    {/* CLI 로그인 — 설정 위에 겹쳐 뜬다(나중에 마운트 = 위에 그려짐) */}
    <AiAuthModal
      open={authOpen}
      provider={provider}
      cliPath={path}
      onClose={() => setAuthOpen(false)}
      onLoggedIn={() => void refreshAuth()}
    />

    {/* DB 연결 추가/편집 — 다이어그램 탭이 DB_CONNECTIONS_EVENT 로 알아채 동기화를 시작한다 */}
    <DbConnectionModal
      open={dbModal.open}
      connection={dbModal.connection}
      onClose={() => setDbModal({ open: false, connection: null })}
      onSaved={() => setDbRefresh((n) => n + 1)}
    />

    {/* 연결 삭제 — 프로필 + 키체인만. 파일은 남는다 */}
    <Modal
      open={dbDelete !== null}
      title={t("settings.db.deleteTitle")}
      onClose={() => setDbDelete(null)}
      narrow
      footer={
        <>
          <button className="btn btn-sm" onClick={() => setDbDelete(null)}>
            {t("common.cancel")}
          </button>
          <button className="btn btn-sm btn-danger-ghost" onClick={() => void removeDbConnection()}>
            {t("common.delete")}
          </button>
        </>
      }
    >
      <p style={{ margin: 0 }}>
        <b>{dbDelete?.name}</b> — {t("settings.db.deleteBody")}
      </p>
    </Modal>

    {/* 비밀번호 다시 입력 — 백업을 다른 기기에 복원해 키체인 항목이 없을 때 */}
    <Modal
      open={dbPw !== null}
      title={t("settings.db.passwordTitle", { name: dbPw?.name ?? "" })}
      onClose={() => setDbPw(null)}
      narrow
      footer={
        <>
          <button className="btn btn-sm" onClick={() => setDbPw(null)} disabled={dbPwBusy}>
            {t("common.cancel")}
          </button>
          <button
            className="btn btn-primary btn-sm"
            onClick={() => void saveDbPassword()}
            disabled={!dbPwValue || dbPwBusy}
          >
            {t("common.save")}
          </button>
        </>
      }
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void saveDbPassword();
        }}
      >
        <div className="field">
          <label>{t("diagrams.db.field.password")}</label>
          <PasswordField
            value={dbPwValue}
            show={dbPwShow}
            onToggle={() => setDbPwShow((v) => !v)}
            onChange={setDbPwValue}
            autoFocus
          />
          <div className="hint">{t("diagrams.db.hint.password")}</div>
        </div>
        {dbPwError && <div className="error-note">{dbPwError}</div>}
      </form>
    </Modal>
    </>
  );
}
