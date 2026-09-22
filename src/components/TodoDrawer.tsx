// 오늘 목록 오른쪽의 서랍 — 탭 둘이다.
//   오늘 후보 — AI 가 내 기록을 훑어 고른 것. 아직 할 일이 아니다(비우는 게 목표)
//   언젠가   — 달력에서 내려놓은 할 일이 사는 곳
//
// 왜 목록 위아래가 아니라 옆인가: 위에 쌓으면 오늘 목록이 아래로 밀리고 내려놓은 것이 목록의
// 일부처럼 보인다. 옆에 두면 경계가 공간으로 분명해진다 — 왼쪽은 오늘 하기로 한 것,
// 오른쪽은 아직 아닌 것. 끌어오는 방향(오른쪽→왼쪽)이 곧 '오늘로 올린다'는 뜻이 된다.
//
// **체크박스가 없다.** 여기 있는 것의 성공 상태는 '완료'가 아니라 '오늘로 올라감'이다.
// 체크박스를 두면 이 서랍이 두 번째 죄책감 목록이 되고, 날짜 목록이 썩던 문제가 장소만 바꿔
// 그대로 반복된다. 그래서 카드 자체가 동작이다 — 누르면 오늘로 간다.

import { useState } from "react";
import { AiThinking, ConfirmDelete } from "../ui";
import type { SuggestState } from "../lib/todoSuggest";
import { Icon } from "../icons";
import { Tooltip } from "../ui";
import { t } from "../lib/i18n";
import type { Todo, TodoAncestor } from "../types";
import { ancestorPath } from "../lib/todoTree";

const DAY_MS = 86_400_000;

/** 내려놓은 지 며칠 됐나 — 그날 안이면 0 */
export function parkedDays(parkedAt: number, now: number): number {
  return Math.max(0, Math.floor((now - parkedAt) / DAY_MS));
}

/** 내려놓은 것들 중 **맨 위 줄**만. 하위는 부모와 함께 내려가므로(parkSubtree) 따로 카드를
 *  세우면 같은 덩어리가 두 번 보인다. 부모가 목록에 없으면 그 줄이 곧 맨 위다. */
export function parkedRoots(rows: Todo[]): Todo[] {
  const ids = new Set(rows.map((r) => r.id));
  return rows.filter((r) => r.parent_id == null || !ids.has(r.parent_id));
}

/** 같은 묶음에서 내려온 것끼리 묶는다 — 줄마다 "Devops" 를 반복하면 그게 서로 남남처럼 보인다.
 *  이름은 머리글 한 번에 올리고 카드는 본문만 남긴다(오늘 목록의 묶음 머리글과 같은 문법).
 *
 *  머리글 순서는 **그 묶음의 첫 카드가 나온 순서**다. 목록 자체가 오래 묵은 것부터라
 *  (listParked 의 parked_at ASC) 따로 정렬하면 "17일째 안 건드렸네" 가 위에서 밀려난다.
 *  묶음이 없던 것(원래 최상위)은 머리글 없이 그 자리에 그대로 선다. */
export interface ParkedGroup {
  /** 머리글 텍스트. 빈 문자열이면 머리글 없는 묶음(원래 최상위였던 것들) */
  key: string;
  items: Todo[];
}
export function groupParked(roots: Todo[], ancestors: TodoAncestor[]): ParkedGroup[] {
  const out: ParkedGroup[] = [];
  for (const r of roots) {
    const key = ancestorPath(ancestors, r.parent_id).join(" › ");
    // 같은 이름이라도 **떨어져 있으면 합치지 않는다** — 목록 순서가 곧 나이라
    // 멀리 있는 것을 끌어올리면 위아래가 뜻을 잃는다
    const last = out[out.length - 1];
    if (last && last.key === key) last.items.push(r);
    else out.push({ key, items: [r] });
  }
  return out;
}

export function TodoDrawer({
  rows,
  ancestors,
  suggest,
  open,
  onClose,
  onRun,
  onAccept,
  onPull,
  onDelete,
}: {
  rows: Todo[];
  /** 서랍 밖(달력)에 남아 있는 조상 행 — 카드에 "DEVOPS ›" 를 달아 문맥을 되돌려 준다 */
  ancestors: TodoAncestor[];
  suggest: SuggestState;
  open: boolean;
  onClose: () => void;
  /** 다시 훑기 */
  onRun: () => void;
  /** 후보를 오늘 목록으로 받아들인다 */
  onAccept: (index: number) => void;
  /** 언젠가에서 오늘로 올린다 (서브트리째) */
  onPull: (todo: Todo) => void;
  onDelete: (todo: Todo) => void;
}) {
  const [tab, setTab] = useState<"suggest" | "parked">("suggest");
  const now = Date.now();
  const roots = parkedRoots(rows);
  const kidCount = (id: number) => rows.filter((r) => r.parent_id === id).length;
  if (!open) return null;
  const busy = suggest.phase === "running";
  return (
    <aside className="parked">
      <div className="parked-head">
        <div className="parked-tabs" role="tablist">
          <button
            role="tab"
            aria-selected={tab === "suggest"}
            className="tab"
            onClick={() => setTab("suggest")}
          >
            {t("todos.suggest.title")}
            {suggest.items.length > 0 && <span className="parked-cnt">{suggest.items.length}</span>}
          </button>
          <button
            role="tab"
            aria-selected={tab === "parked"}
            className="tab"
            onClick={() => setTab("parked")}
          >
            {t("todos.parked.title")}
            {/* 카드 수다(행 수가 아니라). 서브트리는 카드 하나로 접히고 그 안의 수는 카드가
                "2 sub" 로 말하므로, 행을 세면 "3 인데 하나만 보인다" 가 되어 삼켜진 줄 안다 */}
            {roots.length > 0 && <span className="parked-cnt">{roots.length}</span>}
          </button>
        </div>
        <span className="spacer" />
        {tab === "suggest" && (
          <Tooltip label={t("todos.suggest.run")}>
            <button
              className="icon-btn sm"
              aria-label={t("todos.suggest.run")}
              onClick={onRun}
              disabled={busy}
            >
              <Icon name="refresh" size={13} />
            </button>
          </Tooltip>
        )}
        <Tooltip label={t("todos.parked.close")}>
          <button className="icon-btn sm" aria-label={t("todos.parked.close")} onClick={onClose}>
            <Icon name="chevron-right" size={14} />
          </button>
        </Tooltip>
      </div>

      {tab === "suggest" ? (
        <>
          <div className="parked-body">
            {suggest.error && <div className="error-note">{suggest.error}</div>}
            {busy && (
              <AiThinking
                compact
                label={
                  suggest.step === "collect"
                    ? t("todos.suggest.collecting")
                    : t("todos.suggest.running")
                }
              />
            )}
            {!busy &&
              suggest.items.map((it, i) => (
                <SuggestCard key={`${it.text}-${i}`} item={it} onAccept={() => onAccept(i)} />
              ))}
            {!busy && !suggest.error && suggest.items.length === 0 && (
              <p className="parked-empty">
                {suggest.phase === "empty"
                  ? t("todos.suggest.nothingToRead")
                  : suggest.phase === "done"
                    ? t("todos.suggest.none")
                    : t("todos.suggest.idle")}
              </p>
            )}
          </div>
          {!busy && suggest.items.length > 0 && (
            <p className="parked-note">{t("todos.suggest.hint")}</p>
          )}
        </>
      ) : (
        <>
          <div className="parked-body">
            {roots.length === 0 ? (
              <p className="parked-empty">{t("todos.parked.empty")}</p>
            ) : (
              groupParked(roots, ancestors).map((g, gi) => (
                <div className="parked-group" key={`${g.key}-${gi}`}>
                  {g.key && (
                    <div className="parked-group-head">
                      <span className="parked-group-name" title={g.key}>
                        {g.key}
                      </span>
                      <span className="parked-group-rule" />
                    </div>
                  )}
                  {g.items.map((r) => (
                    <ParkedCard
                      key={r.id}
                      todo={r}
                      kids={kidCount(r.id)}
                      days={r.parked_at == null ? 0 : parkedDays(r.parked_at, now)}
                      onPull={() => onPull(r)}
                      onDelete={() => onDelete(r)}
                    />
                  ))}
                </div>
              ))
            )}
          </div>
          {roots.length > 0 && <p className="parked-note">{t("todos.parked.hint")}</p>}
        </>
      )}
    </aside>
  );
}

/** 후보 카드 — 언젠가 카드와 같은 문법이다(누르면 왼쪽으로 간다). 다른 건 출처와 '왜 오늘인가'
 *  가 붙는 것뿐: 그게 없으면 이 서랍은 알림 쓰레기통이 된다. */
function SuggestCard({
  item,
  onAccept,
}: {
  item: { text: string; source: string; why: string };
  onAccept: () => void;
}) {
  return (
    <div
      className="parked-card suggest-card"
      role="button"
      tabIndex={0}
      title={t("todos.suggest.acceptTip")}
      onClick={onAccept}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onAccept();
        }
      }}
    >
      <span className="suggest-main">
        <span className="suggest-meta">
          <span className="suggest-src">{t(sourceKey(item.source))}</span>
          {item.why && <span className="suggest-why">{item.why}</span>}
        </span>
        <span className="parked-card-text">{item.text}</span>
      </span>
    </div>
  );
}

/** 출처 딱지 — 모르는 값이 와도 빈 칸을 만들지 않는다 */
function sourceKey(source: string) {
  if (source === "overdue") return "todos.suggest.src.overdue" as const;
  if (source === "anytime") return "todos.suggest.src.anytime" as const;
  if (source === "activity") return "todos.suggest.src.activity" as const;
  if (source === "note") return "todos.suggest.src.note" as const;
  return "todos.suggest.src.other" as const;
}

/** 카드 하나 = 동작 하나./** 카드 하나 = 동작 하나. 평소엔 표시가 없고, 올라가면 왼쪽 여백에 화살표가 떠오르며
 *  카드가 2px 왼쪽으로 물러난다 — 움직이는 방향이 곧 가는 곳이다(오늘 목록은 왼쪽에 있다). */
function ParkedCard({
  todo,
  kids,
  days,
  onPull,
  onDelete,
}: {
  todo: Todo;
  kids: number;
  days: number;
  onPull: () => void;
  onDelete: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  return (
    <div
      className="parked-card"
      role="button"
      tabIndex={0}
      title={t("todos.parked.pullTip")}
      onClick={onPull}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onPull();
        }
      }}
    >
      <span className="parked-card-text">{todo.content}</span>
      {kids > 0 && <span className="parked-card-kids">{t("todos.parked.kids", { n: kids })}</span>}
      <span className="parked-card-age">
        {days === 0 ? t("todos.parked.age.zero") : t("todos.parked.age", { n: days })}
      </span>
      {/* 확인은 카드 안 두 버튼이 아니라 **모달**이다(§3) — 자리마다 묻는 방식이 다르면
          어떤 화면에서 무엇을 기대해야 하는지 매번 다시 배우게 된다 */}
      <span className="parked-card-actions" onClick={(e) => e.stopPropagation()}>
        <Tooltip label={t("common.delete")}>
          <button
            className="icon-btn sm danger"
            aria-label={t("common.delete")}
            onClick={() => setConfirming(true)}
          >
            <Icon name="trash" size={13} />
          </button>
        </Tooltip>
      </span>
      <ConfirmDelete
        open={confirming}
        title={t("todos.parked.deleteTitle")}
        name={todo.content}
        body={t("todos.parked.deleteConfirm", { name: "{name}" })}
        onCancel={() => setConfirming(false)}
        onConfirm={() => {
          setConfirming(false);
          onDelete();
        }}
      />
    </div>
  );
}
