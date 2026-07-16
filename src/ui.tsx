// 공유 프레젠테이션 컴포넌트

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import type { Confidence, ConceptStatus } from "./types";
import { Icon } from "./icons";

export function ConfidenceDots({ value }: { value: Confidence }) {
  return (
    <span className="dots" title={`자신감 ${value}/3`}>
      {[1, 2, 3].map((i) => (
        <span key={i} className={`dot ${i <= value ? "on" : ""}`} />
      ))}
    </span>
  );
}

export function StatusBadge({ status }: { status: ConceptStatus }) {
  return (
    <span className={`badge ${status}`}>
      {status === "learning" ? "학습중" : "학습완료"}
    </span>
  );
}

export function Spinner() {
  return <span className="spinner" />;
}

/** AI 응답 대기 공통 로딩 — 생동감 있는 인디터미닛 스윕 바 + 펄스 스파클.
 *  모든 AI 기능(질문·작성·개념 생성/보강)에서 이 컴포넌트로 통일한다.
 *  compact: 스레드 말풍선 등 인라인 자리(중앙정렬·큰 여백 없이 좌측·꽉 찬 바). */
export function AiThinking({
  label,
  hint,
  compact,
}: {
  label: string;
  hint?: string;
  compact?: boolean;
}) {
  return (
    <div
      className={`ai-thinking ${compact ? "compact" : ""}`}
      role="status"
      aria-live="polite"
    >
      <div className="ai-thinking-label">
        <span className="ai-thinking-spark">
          <Icon name="sparkles" size={compact ? 12 : 14} />
        </span>
        <span>{label}</span>
      </div>
      <div className="ai-progress" aria-hidden="true">
        <span className="ai-progress-bar" />
      </div>
      {hint && <div className="hint ai-thinking-hint">{hint}</div>}
    </div>
  );
}

export function TagChip({
  label,
  active,
  onClick,
  onRemove,
}: {
  label: string;
  active?: boolean;
  onClick?: () => void;
  onRemove?: () => void;
}) {
  return (
    <span
      className={`chip ${active ? "active" : ""} ${onClick ? "btn-like" : ""}`}
      onClick={onClick}
    >
      #{label}
      {onRemove && (
        <span
          className="x"
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
        >
          <Icon name="x" size={12} />
        </span>
      )}
    </span>
  );
}

export function Modal({
  open,
  title,
  onClose,
  children,
  footer,
  wide,
}: {
  open: boolean;
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
}) {
  useEffect(() => {
    if (!open) return;
    const h = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      // 커스텀 Select 드롭다운이 열려 있으면 그쪽이 먼저 닫히도록 모달은 유지
      if (document.querySelector(".select-menu")) return;
      // mermaid 확대 뷰어가 위에 떠 있으면 그쪽만 닫히게 모달은 유지
      if (document.querySelector(".mmd-zoom-overlay")) return;
      onClose();
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [open, onClose]);

  if (!open) return null;
  return (
    <div
      className="overlay"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className={`modal ${wide ? "wide" : ""}`}>
        <div className="modal-head">
          <h2>{title}</h2>
          <button className="icon-btn" onClick={onClose} aria-label="닫기">
            <Icon name="x" />
          </button>
        </div>
        <div className="modal-body">{children}</div>
        {footer && <div className="modal-foot">{footer}</div>}
      </div>
    </div>
  );
}

export function timeAgo(ms: number): string {
  const diff = Date.now() - ms;
  const m = Math.floor(diff / 60000);
  if (m < 1) return "방금";
  if (m < 60) return `${m}분 전`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}시간 전`;
  const d = Math.floor(h / 24);
  if (d < 30) return `${d}일 전`;
  return new Date(ms).toLocaleDateString("ko-KR");
}

/** 커스텀 드롭다운 (네이티브 select 대신).
 *  메뉴는 body 로 portal + position:fixed 로 띄워, 모달 등 overflow 컨테이너에
 *  잘리거나 스크롤 높이를 밀어 레이아웃을 흔드는 문제를 원천 차단한다. */
export function Select<T extends string>({
  value,
  options,
  onChange,
  align = "left",
  block = false,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  align?: "left" | "right";
  block?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{
    top: number;
    left?: number;
    right?: number;
    minWidth: number;
  } | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  const place = useCallback(() => {
    const el = triggerRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos(
      align === "right"
        ? { top: r.bottom + 6, right: window.innerWidth - r.right, minWidth: r.width }
        : { top: r.bottom + 6, left: r.left, minWidth: r.width },
    );
  }, [align]);

  useEffect(() => {
    if (!open) return;
    place();
    const onDown = (e: MouseEvent) => {
      const t = e.target as Node;
      if (rootRef.current?.contains(t) || menuRef.current?.contains(t)) return;
      setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    const onReflow = () => setOpen(false);
    document.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", onReflow);
    window.addEventListener("scroll", onReflow, true);
    return () => {
      document.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", onReflow);
      window.removeEventListener("scroll", onReflow, true);
    };
  }, [open, place]);

  const cur = options.find((o) => o.value === value);

  return (
    <div className={`select ${block ? "block" : ""}`} ref={rootRef}>
      <button
        ref={triggerRef}
        className="select-trigger"
        onClick={() => setOpen((v) => !v)}
      >
        <span>{cur?.label}</span>
        <svg className="select-caret" width="10" height="6" viewBox="0 0 10 6">
          <path
            d="M1 1l4 4 4-4"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </button>
      {open &&
        pos &&
        createPortal(
          <div
            ref={menuRef}
            className="select-menu"
            style={{
              top: pos.top,
              left: pos.left,
              right: pos.right,
              minWidth: pos.minWidth,
            }}
          >
            {options.map((o) => (
              <button
                key={o.value}
                className={`select-item ${o.value === value ? "active" : ""}`}
                onClick={() => {
                  onChange(o.value);
                  setOpen(false);
                }}
              >
                <span className="select-check">
                  {o.value === value ? "✓" : ""}
                </span>
                {o.label}
              </button>
            ))}
          </div>,
          document.body,
        )}
    </div>
  );
}
