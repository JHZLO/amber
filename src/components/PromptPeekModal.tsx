// 저장 프롬프트 내용 보기 — AI 작성 모달 위에 겹쳐 뜨는 작은 모달. 칩 이름만으로는 무슨 지시가 들어가는지
// 알 수 없어서 칩의 [내용 보기] 버튼이 여기로 온다. 읽은 자리에서 바로 포함/제외를 정하고 닫힌다.
// 편집은 하지 않는다 — 정본은 설정 › 프롬프트(lib/prompts.ts). 부모 모달의 형제로 렌더한다(Modal 은 portal 이 아니다).

import type { SavedPrompt } from "../lib/prompts";
import { Modal } from "../ui";
import { Icon } from "../icons";
import { t } from "../lib/i18n";

export function PromptPeekModal({
  prompt,
  included,
  onToggle,
  onClose,
}: {
  /** null 이면 닫힘 */
  prompt: SavedPrompt | null;
  included: boolean;
  onToggle: () => void;
  onClose: () => void;
}) {
  const name = prompt ? prompt.label.trim() || prompt.text.slice(0, 20) : "";
  return (
    <Modal
      open={prompt !== null}
      title={name}
      onClose={onClose}
      footer={
        <>
          <button className="btn btn-sm" onClick={onClose}>
            {t("common.close")}
          </button>
          <button
            className={`btn ${included ? "" : "btn-primary"}`}
            onClick={() => {
              onToggle();
              onClose();
            }}
          >
            <Icon name={included ? "minus" : "check"} size={15} />
            {included ? t("notes.ai.promptPeek.exclude") : t("notes.ai.promptPeek.include")}
          </button>
        </>
      }
    >
      <div className="field">
        <label>{t("notes.ai.promptPeek.eyebrow")}</label>
        <pre className="prompt-peek-text">{prompt?.text}</pre>
        <div className="hint">{t("notes.ai.promptPeek.editHint")}</div>
      </div>
    </Modal>
  );
}
