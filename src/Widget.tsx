import { useCallback, useEffect, useRef, useState } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { emitTo, listen } from "@tauri-apps/api/event";
import type { ConceptWithTags, Confidence } from "./types";
import { adjustConfidence, learningQueue, markSeen, setStatus } from "./lib/db";
import { ConfidenceDots } from "./ui";
import { Icon } from "./icons";
import { LANG_CHANGED_EVENT, t } from "./lib/i18n";

async function showMain() {
  const main = await WebviewWindow.getByLabel("main");
  if (main) {
    await main.show();
    await main.unminimize();
    await main.setFocus();
  }
}

export function Widget() {
  const [queue, setQueue] = useState<ConceptWithTags[]>([]);
  const [idx, setIdx] = useState(0);
  const [ready, setReady] = useState(false);
  const dirty = useRef(false);
  const curIdRef = useRef<number | null>(null);

  const snapshot = useCallback(async (keepId?: number | null) => {
    const q = await learningQueue();
    setQueue(q);
    setReady(true);
    dirty.current = false;
    if (keepId != null) {
      const i = q.findIndex((c) => c.id === keepId);
      setIdx(i >= 0 ? i : 0);
    } else {
      setIdx((prev) => (prev < q.length ? prev : 0));
    }
    return q;
  }, []);

  useEffect(() => {
    snapshot();
  }, [snapshot]);

  // 다른 창의 변경 → 다음 이동 때 재스냅샷하도록 dirty 표시
  useEffect(() => {
    const un = listen("concept-changed", () => {
      dirty.current = true;
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  // 설정(메인 창)에서 언어 변경 → 새 사전으로 다시 로드 (t()는 페이지 로드 시 고정이라 reload 필수)
  useEffect(() => {
    const un = listen(LANG_CHANGED_EVENT, () => location.reload());
    return () => {
      un.then((f) => f());
    };
  }, []);

  const current = queue[idx] ?? null;
  curIdRef.current = current?.id ?? null;

  // seen 추적: 활성 카드가 3초 유지 + 창이 보일 때만 1회 기록
  useEffect(() => {
    if (!current) return;
    const id = current.id;
    const t = setTimeout(() => {
      if (document.visibilityState === "visible") markSeen(id);
    }, 3000);
    return () => clearTimeout(t);
  }, [current?.id]);

  // 이동: dirty면 현재 카드 유지한 채 최신 큐로 재스냅샷 후, 최신 길이/위치 기준으로 delta 적용
  const go = useCallback(
    async (delta: number) => {
      const curId = curIdRef.current;
      let q = queue;
      if (dirty.current) q = await snapshot(curId);
      const n = q.length;
      if (n === 0) return;
      const base = curId != null ? q.findIndex((c) => c.id === curId) : -1;
      const from = base >= 0 ? base : idx;
      setIdx((((from + delta) % n) + n) % n);
    },
    [queue, idx, snapshot],
  );

  async function complete() {
    if (!current) return;
    await setStatus(current.id, "learned");
    await emitTo("main", "concept-changed", {});
    await snapshot(); // 큐에서 빠지고 현재 인덱스가 다음 카드를 가리킴
  }

  async function conf(delta: number) {
    if (!current) return;
    await adjustConfidence(current.id, delta);
    await emitTo("main", "concept-changed", {});
    dirty.current = true; // 순서 재계산은 다음 이동부터
    setQueue((q) =>
      q.map((c) =>
        c.id === current.id
          ? {
              ...c,
              confidence: Math.min(3, Math.max(1, c.confidence + delta)) as Confidence,
            }
          : c,
      ),
    );
  }

  async function openDetail() {
    if (!current) return;
    await emitTo("main", "open-concept", { id: current.id });
    await showMain();
  }

  // 키보드 단축키 — 아래 버튼들과 1:1 (마우스 없이도 위젯을 다 쓸 수 있게).
  // IME 조합 중이거나 입력에 포커스가 있으면 글자 입력을 뺏으므로 무시하고,
  // 조합키(⌘/⌃/⌥)가 눌린 조합도 앱·OS 단축키 몫으로 남긴다.
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.isComposing || e.metaKey || e.ctrlKey || e.altKey) return;
      const el = document.activeElement as HTMLElement | null;
      if (el && (el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName))) return;
      const k = e.key.toLowerCase();
      // Enter/Space 는 포커스된 버튼을 네이티브로 누르니 그쪽에 양보(이중 실행 방지)
      if ((k === "enter" || k === " ") && el?.tagName === "BUTTON") return;
      if (k === "arrowleft") go(-1);
      else if (k === "arrowright") go(1);
      else if (k === "enter" || k === "d") complete();
      // 자신감 ±는 버튼 disabled 조건과 같은 경계에서 멈춘다
      else if ((k === "arrowdown" || k === "[") && current && current.confidence > 1) conf(-1);
      else if ((k === "arrowup" || k === "]") && current && current.confidence < 3) conf(1);
      else if (k === "o" || k === " ") {
        e.preventDefault(); // Space 기본 스크롤 차단
        openDetail();
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [go, current]);

  function hideWidget() {
    getCurrentWindow().hide();
  }

  function onHeadMouseDown(e: ReactMouseEvent) {
    if ((e.target as HTMLElement).closest("button")) return;
    getCurrentWindow().startDragging();
  }

  if (ready && queue.length === 0) {
    return (
      <div className="widget-card">
        <div className="widget-head" onMouseDown={onHeadMouseDown}>
          <span className="widget-eyebrow">TIL</span>
          <span className="widget-count" />
          <button className="widget-x" onClick={hideWidget} aria-label={t("app.widget.hide")}>
            <Icon name="x" size={13} />
          </button>
        </div>
        <div className="widget-empty">
          {t("app.widget.emptyQueue")}
          <button
            className="widget-btn"
            style={{ width: "auto", padding: "0 10px" }}
            onClick={showMain}
          >
            {t("app.widget.openMain")}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="widget-card">
      <div className="widget-head" onMouseDown={onHeadMouseDown}>
        <span className="widget-eyebrow">TIL</span>
        <span className="widget-count">
          {current ? idx + 1 : 0} / {queue.length}
        </span>
        <button className="widget-x" onClick={hideWidget} aria-label={t("app.widget.hide")}>
          <Icon name="x" size={13} />
        </button>
      </div>

      <div className="widget-mid">
        <button className="widget-nav" onClick={() => go(-1)} aria-label={t("app.widget.prev")}>
          <span>
            <Icon name="chevron-left" size={18} />
          </span>
        </button>
        <div className="widget-main">
          <div className="widget-title">{current?.title}</div>
          <div className="widget-summary">{current?.summary}</div>
        </div>
        <button className="widget-nav" onClick={() => go(1)} aria-label={t("app.widget.next")}>
          <span>
            <Icon name="chevron-right" size={18} />
          </span>
        </button>
      </div>

      <div className="widget-foot">
        {current && <ConfidenceDots value={current.confidence} />}
        <span className="spacer" />
        <button className="widget-btn" onClick={complete} aria-label={t("app.widget.complete")}>
          <Icon name="check" size={15} />
        </button>
        <button
          className="widget-btn"
          onClick={() => conf(-1)}
          disabled={!current || current.confidence <= 1}
          aria-label={t("app.widget.confDown")}
        >
          <Icon name="minus" size={15} />
        </button>
        <button
          className="widget-btn"
          onClick={() => conf(1)}
          disabled={!current || current.confidence >= 3}
          aria-label={t("app.widget.confUp")}
        >
          <Icon name="plus" size={15} />
        </button>
        <button className="widget-btn" onClick={openDetail} aria-label={t("app.widget.openDetail")}>
          <Icon name="expand" size={14} />
        </button>
      </div>
    </div>
  );
}
