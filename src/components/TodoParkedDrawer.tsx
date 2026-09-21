// '언젠가' 서랍 — 달력에서 내려놓은 할 일이 사는 곳. 오늘 목록 오른쪽에 선다.
//
// 왜 목록 위아래가 아니라 옆인가: 위에 쌓으면 오늘 목록이 아래로 밀리고 내려놓은 것이 목록의
// 일부처럼 보인다. 옆에 두면 경계가 공간으로 분명해진다 — 왼쪽은 오늘 하기로 한 것,
// 오른쪽은 아직 아닌 것. 끌어오는 방향(오른쪽→왼쪽)이 곧 '오늘로 올린다'는 뜻이 된다.
//
// **체크박스가 없다.** 여기 있는 것의 성공 상태는 '완료'가 아니라 '오늘로 올라감'이다.
// 체크박스를 두면 이 서랍이 두 번째 죄책감 목록이 되고, 날짜 목록이 썩던 문제가 장소만 바꿔
// 그대로 반복된다. 그래서 카드 자체가 동작이다 — 누르면 오늘로 간다.

import { useState } from "react";
import { Icon } from "../icons";
import { Tooltip } from "../ui";
import { t } from "../lib/i18n";
import type { Todo } from "../types";

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

export function TodoParkedDrawer({
  rows,
  open,
  onToggle,
  onPull,
  onDelete,
}: {
  rows: Todo[];
  open: boolean;
  onToggle: () => void;
  /** 오늘로 올린다 (서브트리째) */
  onPull: (todo: Todo) => void;
  onDelete: (todo: Todo) => void;
}) {
  const now = Date.now();
  const roots = parkedRoots(rows);
  const kidCount = (id: number) => rows.filter((r) => r.parent_id === id).length;
  // 손잡이는 **언제나 렌더한다.** 열려 있을 때는 CSS 가 숨기는데, 창이 좁아지면 서랍 대신
  // 손잡이가 다시 나온다(아래 미디어 쿼리) — 좁은 창에서 서랍이 사라지면 들어갈 길이 없어진다.
  const handle = (
    <button
      className={`parked-handle ${open ? "only-narrow" : ""}`}
      onClick={onToggle}
      title={t("todos.parked.open")}
    >
      <Icon name="chevron-left" size={14} />
      <span className="parked-handle-label">{t("todos.parked.title")}</span>
      {rows.length > 0 && <span className="parked-handle-cnt">{rows.length}</span>}
    </button>
  );
  if (!open) return handle;
  return (
    <>
      {handle}
    <aside className="parked">
      <div className="parked-head">
        <b>{t("todos.parked.title")}</b>
        <span className="parked-cnt">{rows.length}</span>
        <span className="spacer" />
        <Tooltip label={t("todos.parked.close")}>
          <button className="icon-btn sm" aria-label={t("todos.parked.close")} onClick={onToggle}>
            <Icon name="chevron-right" size={14} />
          </button>
        </Tooltip>
      </div>

      <div className="parked-body">
        {roots.length === 0 ? (
          <p className="parked-empty">{t("todos.parked.empty")}</p>
        ) : (
          roots.map((r) => (
            <ParkedCard
              key={r.id}
              todo={r}
              kids={kidCount(r.id)}
              days={r.parked_at == null ? 0 : parkedDays(r.parked_at, now)}
              onPull={() => onPull(r)}
              onDelete={() => onDelete(r)}
            />
          ))
        )}
      </div>

      {roots.length > 0 && <p className="parked-note">{t("todos.parked.hint")}</p>}
    </aside>
    </>
  );
}

/** 카드 하나 = 동작 하나. 평소엔 표시가 없고, 올라가면 왼쪽 여백에 화살표가 떠오르며
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
      <span className="parked-card-actions" onClick={(e) => e.stopPropagation()}>
        {confirming ? (
          <>
            <button className="btn btn-sm btn-danger-ghost" onClick={onDelete}>
              {t("common.delete")}
            </button>
            <button className="btn btn-sm" onClick={() => setConfirming(false)}>
              {t("common.cancel")}
            </button>
          </>
        ) : (
          <Tooltip label={t("common.delete")}>
            <button
              className="icon-btn sm danger"
              aria-label={t("common.delete")}
              onClick={() => setConfirming(true)}
            >
              <Icon name="trash" size={13} />
            </button>
          </Tooltip>
        )}
      </span>
    </div>
  );
}
