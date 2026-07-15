// AI 연결 온보딩 (최초 1회): 로컬에 설치된 AI CLI 를 자동 감지해 카드로 보여주고
// 클릭 한 번으로 연결한다. 감지 실패 시 설치 안내 링크 + "AI 없이 사용" 제공.
// (orca 의 에이전트 온보딩 패턴: 로그인 셸 PATH 감지 → detected 그리드 → 첫 항목 자동 선택)

import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  connectProvider,
  skipAiOnboarding,
  PROVIDER_LABELS,
  type AiProvider,
  type AppConfig,
} from "../lib/config";
import { detectAiClis, type DetectedCli } from "../lib/claude";
import { Modal, Spinner } from "../ui";
import { Icon } from "../icons";

const INSTALL_LINKS: Record<AiProvider, string> = {
  claude: "https://claude.com/claude-code",
  codex: "https://developers.openai.com/codex",
  gemini: "https://github.com/google-gemini/gemini-cli",
};

export function AiOnboarding({
  open,
  onDone,
}: {
  open: boolean;
  /** 연결 또는 건너뛰기 완료 시 최신 config 전달 */
  onDone: (c: AppConfig) => void;
}) {
  const [detecting, setDetecting] = useState(true);
  const [detected, setDetected] = useState<DetectedCli[]>([]);
  const [selected, setSelected] = useState<AiProvider | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setDetecting(true);
    setSelected(null);
    detectAiClis()
      .then((list) => {
        setDetected(list);
        // 첫 감지 항목 자동 선택
        setSelected((list[0]?.id as AiProvider) ?? null);
      })
      .finally(() => setDetecting(false));
  }, [open]);

  async function connect() {
    const cli = detected.find((d) => d.id === selected);
    if (!cli || busy) return;
    setBusy(true);
    try {
      onDone(await connectProvider(cli.id as AiProvider, cli.path));
    } finally {
      setBusy(false);
    }
  }

  async function skip() {
    if (busy) return;
    setBusy(true);
    try {
      onDone(await skipAiOnboarding());
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      open={open}
      title="AI 연결"
      onClose={skip}
      footer={
        <>
          <button className="btn btn-sm" onClick={skip} disabled={busy}>
            AI 없이 사용
          </button>
          <span className="spacer" />
          <button
            className="btn btn-primary"
            onClick={() => void connect()}
            disabled={busy || !selected || detecting}
          >
            <Icon name="sparkles" size={15} />
            {busy ? "연결 중…" : "연결"}
          </button>
        </>
      }
    >
      <p className="onb-lead">
        Amber 의 AI 기능(개념 정리·노트 작성·인라인 질문)은{" "}
        <b>이미 쓰고 있는 AI CLI 의 로그인 세션</b>을 그대로 사용해요.
        API 키를 따로 저장하지 않습니다.
      </p>

      {detecting ? (
        <div className="loading-box">
          <Spinner />
          <div>설치된 AI CLI 를 찾는 중…</div>
          <div className="hint">로그인 셸 PATH 에서 claude · codex · gemini 를 확인해요</div>
        </div>
      ) : detected.length > 0 ? (
        <div className="onb-grid">
          {detected.map((d) => (
            <button
              key={d.id}
              className={`onb-card ${selected === d.id ? "selected" : ""}`}
              onClick={() => setSelected(d.id as AiProvider)}
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
      ) : (
        <div className="onb-empty">
          <p>
            설치된 AI CLI 를 찾지 못했어요.
            <br />
            아래 중 하나를 설치·로그인한 뒤 설정(⚙)에서 다시 감지할 수 있어요.
          </p>
          <div className="onb-links">
            {(Object.keys(INSTALL_LINKS) as AiProvider[]).map((p) => (
              <button
                key={p}
                className="btn btn-sm"
                onClick={() => void openUrl(INSTALL_LINKS[p])}
              >
                {PROVIDER_LABELS[p]} 설치 안내
              </button>
            ))}
          </div>
        </div>
      )}

      <div className="hint" style={{ marginTop: 12 }}>
        나중에 설정(⚙)에서 언제든 다시 연결하거나 바꿀 수 있어요.
      </div>
    </Modal>
  );
}
