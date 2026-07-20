import { useEffect, useRef, useState } from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import { appDataDir } from "@tauri-apps/api/path";
import type { AiProvider, AppConfig } from "../lib/config";
import {
  PROVIDER_LABELS,
  PROVIDER_MODELS,
  connectProvider,
  loadConfig,
} from "../lib/config";
import {
  aiHealth,
  detectAiClis,
  isAiError,
  type DetectedCli,
} from "../lib/ai";
import {
  loadPrompts,
  makePrompt,
  savePrompts,
  type SavedPrompt,
} from "../lib/prompts";
import { getThemePref, setThemePref, type ThemePref } from "../lib/theme";
import { Modal, Select, Spinner } from "../ui";
import { Icon } from "../icons";
import { ReportSettings } from "./ReportSettings";

const THEMES: { id: ThemePref; label: string }[] = [
  { id: "system", label: "시스템 설정 따름" },
  { id: "light", label: "라이트" },
  { id: "dark", label: "다크" },
];

// 설정 카테고리 탭 — 한 화면에 다 쌓지 않고 갈래로 나눈다
type SetTab = "ai" | "prompts" | "report" | "appearance";
const SETTING_TABS: { id: SetTab; label: string }[] = [
  { id: "ai", label: "AI" },
  { id: "prompts", label: "프롬프트" },
  { id: "report", label: "데일리 리포트" },
  { id: "appearance", label: "모양" },
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

  const [theme, setTheme] = useState<ThemePref>("system");
  const [tab, setTab] = useState<SetTab>("ai");

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
      setTab("ai");
      setTheme(getThemePref());
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  function changeTheme(t: ThemePref) {
    setTheme(t);
    setThemePref(t); // 즉시 적용
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

  async function test() {
    setTesting(true);
    setTestResult(null);
    try {
      const v = await aiHealth(path);
      if (!alive.current) return;
      setTestResult({ ok: true, msg: `연결 성공 — ${v}` });
    } catch (e) {
      if (!alive.current) return;
      setTestResult({ ok: false, msg: isAiError(e) ? e.message : String(e) });
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

  async function openFolder() {
    try {
      await openPath(await appDataDir());
    } catch (e) {
      setTestResult({ ok: false, msg: "폴더 열기 실패: " + String(e) });
    }
  }

  // 에디터 화면일 땐 X/ESC/바깥클릭이 편집만 취소(한 단계 뒤로), 아니면 설정 닫기
  const handleClose = () => (editing ? cancelEdit() : onClose());

  const footer = editing ? (
    <>
      <button className="btn btn-sm" onClick={cancelEdit}>
        취소
      </button>
      <span className="spacer" />
      <button
        className="btn btn-primary"
        onClick={saveEdit}
        disabled={!editText.trim()}
      >
        저장
      </button>
    </>
  ) : (
    <>
      <button className="btn btn-sm" onClick={openFolder}>
        데이터 폴더 열기
      </button>
      <span className="spacer" />
      <button className="btn btn-sm" onClick={onClose}>
        닫기
      </button>
      <button className="btn btn-primary" onClick={save} disabled={testing}>
        저장
      </button>
    </>
  );

  return (
    <Modal
      open={open}
      title={editing ? (isNew ? "새 프롬프트" : "프롬프트 편집") : "설정"}
      onClose={handleClose}
      footer={footer}
    >
      {editing ? (
        // 포커스 에디터 — 이름 + 큰 textarea 하나만
        <>
          <div className="field">
            <label>이름</label>
            <input
              className="input"
              autoFocus
              placeholder="예: 개념노트 보강"
              value={editLabel}
              onChange={(e) => setEditLabel(e.target.value)}
            />
          </div>
          <div className="field">
            <label>프롬프트 (Claude에게 줄 지시)</label>
            <textarea
              className="textarea"
              style={{ fontFamily: "var(--font)" }}
              rows={10}
              placeholder="예: 진짜 개념노트처럼 대/중/소제목으로 분류하고 예시 코드와 표를 넣어 상세히 보강해줘"
              value={editText}
              onChange={(e) => setEditText(e.target.value)}
            />
            <div className="hint">
              이름을 비우면 지시문 앞부분이 이름으로 쓰여요.
            </div>
          </div>
        </>
      ) : (
        <>
          <div className="set-tabs">
            {SETTING_TABS.map((t) => (
              <button
                key={t.id}
                className={`set-tab ${tab === t.id ? "active" : ""}`}
                onClick={() => setTab(t.id)}
              >
                {t.label}
              </button>
            ))}
          </div>
          <div className="set-tab-content">
          {tab === "ai" && (
          <section className="set-section">
            <div className="set-head">
              <span className="set-eyebrow">AI 연결</span>
              <span className="spacer" />
              <button
                className="btn btn-sm"
                onClick={() => void redetect()}
                disabled={detecting}
              >
                <Icon name="refresh" size={13} />
                {detecting ? "감지 중…" : "다시 감지"}
              </button>
            </div>
            <p className="set-desc">
              {provider
                ? `현재 ${PROVIDER_LABELS[provider]} 에 연결돼 있어요. 로컬 CLI 의 로그인 세션을 그대로 사용합니다.`
                : "연결된 AI 가 없어요. 설치된 CLI 를 감지해 연결하세요."}
            </p>

            {detecting && detected === null ? (
              <div className="loading-box" style={{ padding: "22px 0" }}>
                <Spinner />
                <div className="hint">설치된 CLI 를 찾는 중…</div>
              </div>
            ) : detected !== null && detected.length === 0 ? (
              <div className="error-note">
                설치된 AI CLI 를 찾지 못했어요. claude · codex · gemini 중 하나를
                설치하고 로그인한 뒤 다시 감지하세요.
              </div>
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
                  <label>{PROVIDER_LABELS[provider]} 경로</label>
                  <div className="set-inline">
                    <input
                      className="input"
                      value={path}
                      onChange={(e) => setPath(e.target.value)}
                      placeholder={`/opt/homebrew/bin/${provider}`}
                    />
                    <button className="btn" onClick={test} disabled={testing}>
                      {testing ? <Spinner /> : "연결 테스트"}
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
                  <label>모델</label>
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
                    AI 호출은 연결된 CLI 의 플랜/크레딧을 소모해요.
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
              <span className="set-eyebrow">모양</span>
            </div>
            <div className="field" style={{ marginBottom: 0 }}>
              <label>테마</label>
              <Select
                block
                value={theme}
                options={THEMES.map((t) => ({ value: t.id, label: t.label }))}
                onChange={changeTheme}
              />
            </div>
          </section>
          )}
          {tab === "prompts" && (
          <section className="set-section">
            <div className="set-head">
              <span className="set-eyebrow">저장 프롬프트</span>
              <span className="spacer" />
              <button className="btn btn-sm" onClick={startNew}>
                <Icon name="plus" size={13} />새 프롬프트
              </button>
            </div>
            <p className="set-desc">
              자주 쓰는 지시를 저장해 두면 <b>AI로 노트 작성</b> 모달에서 칩으로
              바로 넣을 수 있어요.
            </p>

            {prompts.length === 0 ? (
              <div className="prompt-empty">
                저장된 프롬프트가 없어요. “새 프롬프트”로 추가하세요.
              </div>
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
                    <button
                      className="icon-btn ghost sm danger prompt-del"
                      title="삭제"
                      onClick={() => removePrompt(p.id)}
                    >
                      <Icon name="trash" size={14} />
                    </button>
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
  );
}
