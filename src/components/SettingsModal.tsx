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
import { getLang, setLang, t, LANG_CHANGED_EVENT, type Lang } from "../lib/i18n";
import { errText } from "../lib/errors";
import { Modal, Select, Spinner, Tooltip } from "../ui";
import { Icon } from "../icons";
import { ReportSettings } from "./ReportSettings";
import { AiAuthModal } from "./AiAuthModal";

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
type SetTab = "ai" | "prompts" | "report" | "appearance";
const SETTING_TABS: { id: SetTab; label: string }[] = [
  { id: "ai", label: t("settings.tab.ai") },
  { id: "prompts", label: t("settings.tab.prompts") },
  { id: "report", label: t("settings.tab.report") },
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

  const alive = useRef(true);
  useEffect(
    () => () => {
      alive.current = false;
    },
    [],
  );

  useEffect(() => {
    if (open) {
      loadConfig().then((c) => {
        setProvider(c.provider);
        setPath(c.cliPath);
        setModel(c.model);
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
    const st = await aiAuthStatus(provider, path);
    if (alive.current) setAuth(st);
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

                <div className="field">
                  <label>{t("settings.auth.row")}</label>
                  <div className="set-inline">
                    <span className="hint">
                      {auth === null
                        ? t("settings.auth.checking")
                        : auth.supported === false
                          ? t("settings.auth.rowUnknown")
                          : auth.loggedIn === true
                            ? t("settings.auth.rowOk")
                            : auth.loggedIn === false
                              ? t("settings.auth.rowExpired")
                              : t("settings.auth.rowUnknown")}
                    </span>
                    <span className="spacer" />
                    <button
                      className="btn"
                      onClick={() => setAuthOpen(true)}
                      disabled={auth?.supported === false}
                    >
                      {t(
                        auth?.loggedIn === true
                          ? "settings.auth.again"
                          : "settings.auth.rowAction",
                      )}
                    </button>
                  </div>
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
              </div>
            )}
          </section>
          )}
          {tab === "report" && <ReportSettings />}
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
    </>
  );
}
