// 노트 위의 AI 실행 배너 — 모달을 닫은 뒤에도 "쓰는 중 / 초안 준비됨 / 실패" 를 노트 자리에서 보이고 되돌아갈 길을 준다.
// 실행 상태는 lib/noteAiRun 스토어에서 온다(이 컴포넌트는 표시 + 버튼만). 세 상태의 무게가 다르다(DESIGN §2·§3):
// 진행 중은 정보라 무채색, 준비됨·실패는 기다리던 결과라 .ok-note/.error-note 의 색을 그대로 얹는다.

import { useEffect, useState } from "react";
import { AiThinking, DiscardAiModal, timeAgo } from "../ui";
import { Icon } from "../icons";
import { t } from "../lib/i18n";
import { useAiWaitLine } from "../lib/aiWait";
import {
  dismissNoteAi,
  dismissNoteAiError,
  stopNoteAi,
  type NoteAiRun,
} from "../lib/noteAiRun";

export function NoteAiBanner({ run, onOpen }: { run: NoteAiRun; onOpen: () => void }) {
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const running = run.phase === "running";
  const waitLine = useAiWaitLine({
    running,
    startedAt: run.startedAt,
    activity: run.activity,
    activityAt: run.activityAt,
    hasRefDirs: run.refDirs.length > 0,
  });
  // "3분 전 완료" 가 굳지 않게 1분마다 다시 그린다
  const [, tick] = useState(0);
  useEffect(() => {
    if (running) return;
    const id = setInterval(() => tick((n) => n + 1), 60_000);
    return () => clearInterval(id);
  }, [running]);

  if (running) {
    return (
      <div className="ai-bg-bar" role="status">
        <AiThinking
          compact
          label={run.continuing ? t("notes.ai.continuing") : t("notes.ai.bg.running")}
          activity={waitLine ?? undefined}
        />
        <span className="ai-bg-actions">
          <button className="btn btn-sm" onClick={onOpen}>
            <Icon name="eye" size={13} />
            {t("notes.ai.bg.open")}
          </button>
          {/* 중단은 멈출 대상과 같은 줄 오른쪽 끝(§3) */}
          <button className="btn btn-sm btn-danger-ghost" onClick={() => stopNoteAi(run.path)}>
            <Icon name="x" size={13} />
            {t("notes.ai.stop")}
          </button>
        </span>
      </div>
    );
  }

  if (run.phase === "error") {
    return (
      <div className="error-note ai-bg-bar" role="alert">
        <span className="ai-bg-text">
          <b>{t("notes.ai.bg.failed")}</b> {run.error}
        </span>
        <span className="ai-bg-actions">
          <button className="btn btn-sm" onClick={onOpen}>
            <Icon name="refresh" size={13} />
            {t("notes.ai.bg.retry")}
          </button>
          <button className="btn btn-sm" onClick={() => dismissNoteAiError(run.path)}>
            {t("common.close")}
          </button>
        </span>
      </div>
    );
  }

  return (
    <>
      <div className="ok-note ai-bg-bar" role="status">
        <span className="ai-bg-text">
          <Icon name="sparkles" size={14} />
          <b>{t("notes.ai.bg.ready")}</b>
          {run.finishedAt && (
            <span className="ai-bg-when">
              {t("notes.ai.bg.readyAt", { when: timeAgo(run.finishedAt) })}
            </span>
          )}
        </span>
        <span className="ai-bg-actions">
          <button className="btn btn-sm btn-primary" onClick={onOpen}>
            {t("notes.ai.bg.review")}
          </button>
          <button className="btn btn-sm btn-danger-ghost" onClick={() => setConfirmDiscard(true)}>
            {t("notes.ai.discard")}
          </button>
        </span>
      </div>
      <DiscardAiModal
        open={confirmDiscard}
        mode="discard"
        onKeep={() => setConfirmDiscard(false)}
        onDiscard={() => {
          dismissNoteAi(run.path);
          setConfirmDiscard(false);
        }}
      />
    </>
  );
}
