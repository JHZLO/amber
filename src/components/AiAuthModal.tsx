// AI CLI 로그인 모달 — 만료된 인증을 앱을 벗어나지 않고 다시 맺는다.
//
// 왜 필요한가: CLI 토큰은 몇 주에 한 번 조용히 만료되고, 그때 AI 기능이 전부 같이 멈춘다.
// 예전엔 화면에 "터미널에서 로그인하세요" 한 줄만 남아서, 멈춘 자리와 고치는 자리가 달랐다.
// 이제 AI_AUTH 에러가 나면 이 모달이 뜨고(App), 설정 › AI 에서도 직접 열 수 있다.
//
// 흐름: [로그인 시작] → CLI 가 브라우저를 열고 stdout 으로 인증 URL 을 준다 → 사용자가
// 브라우저에서 받은 코드를 붙여넣으면 CLI stdin 으로 전달 → CLI 가 끝나면 상태를 다시 확인한다.
// 코드는 어디에도 저장하지 않는다(자식 프로세스 stdin 으로 흘려보낼 뿐).

import { useCallback, useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  aiAuthCancel,
  aiAuthCode,
  aiAuthLogin,
  aiAuthStatus,
  asksForCode,
  extractAuthUrl,
  type AuthStatus,
} from "../lib/auth";
import { PROVIDER_LABELS, type AiProvider } from "../lib/config";
import { errText } from "../lib/errors";
import { t } from "../lib/i18n";
import { Modal, Spinner } from "../ui";
import { Icon } from "../icons";

/** 실패 시 보여 줄 CLI 출력 꼬리 — 전문을 쏟으면 읽히지 않는다 */
const TAIL_CHARS = 400;

type Phase = "checking" | "idle" | "running" | "done";

export function AiAuthModal({
  open,
  provider,
  cliPath,
  onClose,
  onLoggedIn,
}: {
  open: boolean;
  provider: AiProvider | null;
  cliPath: string;
  onClose: () => void;
  /** 로그인이 확인되면 호출 — 부모가 상태 표시를 갱신한다 */
  onLoggedIn?: () => void;
}) {
  const [phase, setPhase] = useState<Phase>("checking");
  const [status, setStatus] = useState<AuthStatus | null>(null);
  const [output, setOutput] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  useEffect(() => {
    if (!open) return;
    let alive = true;
    setPhase("checking");
    setStatus(null);
    setOutput("");
    setCode("");
    setErr("");
    aiAuthStatus(provider, cliPath)
      .then((s) => {
        if (!alive) return;
        setStatus(s);
        setPhase("idle");
      })
      .catch((e) => {
        if (!alive) return;
        setErr(errText(e));
        setPhase("idle");
      });
    return () => {
      alive = false;
    };
  }, [open, provider, cliPath]);

  const close = useCallback(() => {
    void aiAuthCancel();
    onClose();
  }, [onClose]);

  async function start() {
    setOutput("");
    setCode("");
    setErr("");
    setPhase("running");
    try {
      await aiAuthLogin(provider, cliPath, (e) => {
        if (e.kind === "output") {
          setOutput((prev) => prev + e.text);
          return;
        }
        setStatus(e.status);
        setPhase("done");
        if (e.status.loggedIn) onLoggedIn?.();
      });
    } catch (e) {
      setErr(errText(e));
      setPhase("idle");
    }
  }

  async function submit() {
    if (!code.trim() || busy) return;
    setBusy(true);
    setErr("");
    try {
      await aiAuthCode(code);
      setCode("");
    } catch (e) {
      setErr(errText(e));
    } finally {
      setBusy(false);
    }
  }

  const name = provider ? PROVIDER_LABELS[provider] : "";
  const url = extractAuthUrl(output);
  const wantsCode = asksForCode(output);
  const loggedIn = status?.loggedIn === true;
  const unsupported = status?.supported === false;
  // 진행 상황은 CLI 가 쓴 마지막 문장 그대로 — 우리가 다시 쓰면 CLI 가 바뀔 때마다 어긋난다
  const lastLine = output.trim().split("\n").filter(Boolean).pop() ?? "";

  return (
    <Modal
      open={open}
      narrow
      title={t("settings.auth.title")}
      onClose={close}
      footer={
        <>
          <button className="btn btn-sm" onClick={close}>
            {t(phase === "done" && loggedIn ? "common.done" : "common.close")}
          </button>
          <span className="spacer" />
          {!unsupported && !(phase === "done" && loggedIn) && (
            <>
              {wantsCode && phase === "running" ? (
                <button
                  className="btn btn-primary"
                  onClick={() => void submit()}
                  disabled={busy || !code.trim()}
                >
                  {busy ? <Spinner /> : t("settings.auth.submit")}
                </button>
              ) : (
                <button
                  className="btn btn-primary"
                  onClick={() => void start()}
                  disabled={phase === "checking" || phase === "running"}
                >
                  {t(phase === "idle" && !loggedIn ? "settings.auth.start" : "settings.auth.again")}
                </button>
              )}
            </>
          )}
        </>
      }
    >
      {phase === "checking" ? (
        <div className="loading-box" style={{ padding: "22px 0" }}>
          <Spinner />
          <div>{t("settings.auth.checking")}</div>
        </div>
      ) : unsupported ? (
        <p className="set-desc">{t("settings.auth.unsupported", { name })}</p>
      ) : (
        <>
          <p className="set-desc">
            {phase === "done" && loggedIn
              ? t("settings.auth.done")
              : loggedIn && phase === "idle"
                ? t("settings.auth.alreadyOk", { name })
                : t("settings.auth.lead", { name })}
          </p>

          {url && phase !== "done" && (
            <div className="field">
              <label>{t("settings.auth.urlLabel")}</label>
              <div className="set-inline">
                <input className="input" readOnly value={url} />
                <button className="btn" onClick={() => void openUrl(url)}>
                  {t("settings.auth.openBrowser")}
                </button>
              </div>
              <div className="hint" style={{ marginTop: 6 }}>
                {t("settings.auth.urlHint")}
              </div>
            </div>
          )}

          {phase === "running" && wantsCode && (
            <div className="field" style={{ marginBottom: 0 }}>
              <label>{t("settings.auth.codeLabel")}</label>
              <input
                className="input"
                autoFocus
                value={code}
                placeholder={t("settings.auth.codePlaceholder")}
                onChange={(e) => setCode(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void submit();
                }}
              />
              <div className="hint" style={{ marginTop: 6 }}>
                {t("settings.auth.codeSafety")}
              </div>
            </div>
          )}

          {phase === "running" && !wantsCode && (
            <div className="loading-box" style={{ padding: "18px 0" }}>
              <Spinner />
              <div>{lastLine || t("settings.auth.waiting")}</div>
            </div>
          )}

          {phase === "done" &&
            (loggedIn ? (
              <div className="ok-note">
                <Icon name="check" size={14} /> {t("settings.auth.okNote", { name })}
              </div>
            ) : (
              <div className="error-note">
                {t("settings.auth.failed")}
                {output.trim() ? `\n\n${output.trim().slice(-TAIL_CHARS)}` : ""}
              </div>
            ))}

          {err && (
            <div className="error-note" style={{ marginTop: 10 }}>
              {err}
            </div>
          )}
        </>
      )}
    </Modal>
  );
}
