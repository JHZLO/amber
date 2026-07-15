import { useEffect, useRef, useState } from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import { appDataDir } from "@tauri-apps/api/path";
import type { AppConfig } from "../lib/config";
import { MODELS, loadConfig, saveConfig } from "../lib/config";
import { claudeHealth, isClaudeError } from "../lib/claude";
import {
  loadPrompts,
  makePrompt,
  savePrompts,
  type SavedPrompt,
} from "../lib/prompts";
import { getThemePref, setThemePref, type ThemePref } from "../lib/theme";
import { Modal, Select, Spinner } from "../ui";
import { Icon } from "../icons";

const THEMES: { id: ThemePref; label: string }[] = [
  { id: "system", label: "시스템 설정 따름" },
  { id: "light", label: "라이트" },
  { id: "dark", label: "다크" },
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
  const [path, setPath] = useState("");
  const [model, setModel] = useState("");
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; msg: string } | null>(
    null,
  );

  const [theme, setTheme] = useState<ThemePref>("system");

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
        setPath(c.claudePath);
        setModel(c.model);
        setTestResult(null);
      });
      loadPrompts().then(setPrompts);
      setEditing(null);
      setTheme(getThemePref());
    }
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
      const v = await claudeHealth(path);
      if (!alive.current) return;
      setTestResult({ ok: true, msg: `연결 성공 — ${v}` });
    } catch (e) {
      if (!alive.current) return;
      setTestResult({ ok: false, msg: isClaudeError(e) ? e.message : String(e) });
    } finally {
      if (alive.current) setTesting(false);
    }
  }

  async function save() {
    const c: AppConfig = { claudePath: path.trim(), model };
    await saveConfig(c);
    onSaved(c);
    onClose();
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
          <div className="field">
            <label>claude CLI 경로</label>
            <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
              <input
                className="input"
                style={{ flex: 1, minWidth: 0 }}
                value={path}
                onChange={(e) => setPath(e.target.value)}
                placeholder="/opt/homebrew/bin/claude"
              />
              <button
                className="btn"
                style={{ flexShrink: 0 }}
                onClick={test}
                disabled={testing}
              >
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

          <div className="field">
            <label>기본 모델</label>
            <Select
              block
              value={model}
              options={MODELS.map((m) => ({ value: m.id, label: m.label }))}
              onChange={setModel}
            />
          </div>

          <div className="hint">
            정리 호출은 사용 중인 Claude 플랜/크레딧을 소모해요. 비용을 아끼려면
            Sonnet 을 선택하세요.
          </div>

          <div className="field">
            <label>테마</label>
            <Select
              block
              value={theme}
              options={THEMES.map((t) => ({ value: t.id, label: t.label }))}
              onChange={changeTheme}
            />
          </div>

          <div className="field">
            <label style={{ display: "flex", alignItems: "center" }}>
              저장 프롬프트
              <span className="spacer" />
              <button className="btn btn-sm" onClick={startNew}>
                <Icon name="plus" size={13} />새 프롬프트
              </button>
            </label>
            <div className="hint" style={{ marginTop: 0, marginBottom: 8 }}>
              자주 쓰는 지시를 저장해 두면 <b>AI로 노트 작성</b> 모달에서 칩으로
              바로 넣을 수 있어요.
            </div>

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
          </div>
        </>
      )}
    </Modal>
  );
}
