// 기존 노트 ↔ AI 결과의 라인 단위 unified diff (git diff 스타일).
// 변경 없는 긴 구간은 접어서(context fold) 바뀐 곳에 집중하게 한다.

import { useMemo } from "react";
import { diffLines } from "diff";
import { t } from "../lib/i18n";

type Row =
  | { type: "add" | "del" | "ctx"; text: string }
  | { type: "fold"; count: number };

const CONTEXT = 3; // 변경 주변에 보여줄 문맥 줄 수
const FOLD_MIN = 8; // 이보다 긴 무변경 구간만 접음

function buildRows(oldText: string, newText: string): { rows: Row[]; add: number; del: number } {
  const parts = diffLines(oldText, newText);
  const raw: { type: "add" | "del" | "ctx"; text: string }[] = [];
  let add = 0;
  let del = 0;
  for (const p of parts) {
    const type = p.added ? "add" : p.removed ? "del" : "ctx";
    const lines = p.value.replace(/\n$/, "").split("\n");
    for (const ln of lines) {
      raw.push({ type, text: ln });
      if (type === "add") add++;
      else if (type === "del") del++;
    }
  }

  // 긴 ctx 런 접기: 파일 맨 앞/뒤는 CONTEXT 만, 변경 사이는 양쪽 CONTEXT 만 남김
  const rows: Row[] = [];
  let i = 0;
  while (i < raw.length) {
    if (raw[i].type !== "ctx") {
      rows.push(raw[i]);
      i++;
      continue;
    }
    let j = i;
    while (j < raw.length && raw[j].type === "ctx") j++;
    const run = raw.slice(i, j);
    const atStart = i === 0;
    const atEnd = j === raw.length;
    if (run.length > FOLD_MIN) {
      const head = atStart ? 0 : CONTEXT; // 시작이면 앞 문맥 없이 접기
      const tail = atEnd ? 0 : CONTEXT;
      if (!atStart) for (let k = 0; k < head; k++) rows.push(run[k]);
      const folded = run.length - head - tail;
      if (folded > 0) rows.push({ type: "fold", count: folded });
      if (!atEnd) for (let k = run.length - tail; k < run.length; k++) rows.push(run[k]);
    } else {
      rows.push(...run);
    }
    i = j;
  }
  return { rows, add, del };
}

export function DiffView({
  oldText,
  newText,
}: {
  oldText: string;
  newText: string;
}) {
  const { rows, add, del } = useMemo(
    () => buildRows(oldText, newText),
    [oldText, newText],
  );

  const unchanged = add === 0 && del === 0;

  return (
    <div className="diff-wrap">
      <div className="diff-summary">
        {unchanged ? (
          <span className="diff-none">{t("diagrams.diff.none")}</span>
        ) : (
          <>
            <span className="diff-stat add">+{add}</span>
            <span className="diff-stat del">−{del}</span>
            <span className="diff-stat-label">{t("diagrams.diff.lines")}</span>
          </>
        )}
      </div>
      <div className="diff-view">
        {rows.map((r, idx) =>
          r.type === "fold" ? (
            <div key={idx} className="diff-line fold">
              <span className="diff-gutter" />
              <span className="diff-text">
                {t("diagrams.diff.fold", { n: r.count })}
              </span>
            </div>
          ) : (
            <div key={idx} className={`diff-line ${r.type}`}>
              <span className="diff-gutter">
                {r.type === "add" ? "+" : r.type === "del" ? "−" : ""}
              </span>
              <span className="diff-text">{r.text || " "}</span>
            </div>
          ),
        )}
      </div>
    </div>
  );
}
