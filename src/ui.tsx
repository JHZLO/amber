// 공유 프레젠테이션 컴포넌트

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";
import { createPortal } from "react-dom";
import type { Confidence, ConceptStatus } from "./types";
import { Icon, type IconName } from "./icons";

/** 파일 트리 드래그 중 커서를 따라오는 오버레이(원본 행 복제본, dnd-kit DragOverlay 패턴).
 *  body 로 portal → 중첩 폴더의 overflow:hidden 을 벗어나므로 하위 뎁스에서도 안 잘린다.
 *  잡는 순간의 원본 위치/크기를 그대로 써서 '그 자리에서 들린' 것처럼 보인다. 위치 추적은 훅이 담당. */
export function TreeDragOverlay({
  drag,
  leafIcon,
  overlayRef,
}: {
  drag: {
    name: string;
    isDir: boolean;
    left: number;
    top: number;
    width: number;
    height: number;
    padLeft: number;
  };
  leafIcon: IconName;
  overlayRef: RefObject<HTMLDivElement | null>;
}) {
  return createPortal(
    <div
      ref={overlayRef}
      className="tree-drag-overlay"
      style={{
        left: drag.left,
        top: drag.top,
        width: drag.width,
        height: drag.height,
        paddingLeft: drag.padLeft,
      }}
    >
      {/* caret 자리(13px)를 비워 원본 행의 아이콘/라벨 위치와 정확히 겹치게 */}
      <span style={{ width: 13, flexShrink: 0 }} aria-hidden="true" />
      <Icon
        name={drag.isDir ? "folder" : leafIcon}
        size={14}
        className="tree-ico"
      />
      <span className="label">{drag.name}</span>
    </div>,
    document.body,
  );
}

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

/** 체크박스 primitive — 모노톤 채움/아웃라인 문법(꺼짐=아웃라인, 켜짐=primary 필+체크).
 *  색으로 상태를 칠하지 않는다(.claude/DESIGN.md §3). */
export function Checkbox({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: () => void;
  label?: string;
}) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      aria-label={label}
      className={`checkbox ${checked ? "checked" : ""}`}
      onClick={onChange}
    >
      {/* 항상 렌더하고 색으로 표시 — 체크 시 primary-fg, 미체크 hover 시 옅은 힌트 */}
      <Icon name="check" size={12} />
    </button>
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

/** 호버 툴팁 — 자식을 감싸면 잠깐 머무를 때 라벨이 뜬다.
 *  네이티브 `title` 은 Tauri macOS WKWebView 에서 안 뜨므로 아이콘 버튼 힌트는 이걸 쓴다.
 *  라벨은 body 로 portal → 사이드바 overflow 에 안 잘린다. 접근성 이름은 자식에 aria-label 로 따로. */
export function Tooltip({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);

  const show = () => {
    timer.current = setTimeout(() => {
      const r = ref.current?.getBoundingClientRect();
      if (r) setPos({ left: r.left + r.width / 2, top: r.bottom + 6 });
    }, 350);
  };
  const hide = () => {
    if (timer.current) clearTimeout(timer.current);
    setPos(null);
  };
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  return (
    <span
      ref={ref}
      className="tip-wrap"
      onMouseEnter={show}
      onMouseLeave={hide}
      onMouseDown={hide}
    >
      {children}
      {pos &&
        createPortal(
          <span className="tip" style={{ left: pos.left, top: pos.top }}>
            {label}
          </span>,
          document.body,
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
  narrow,
  fixedHeight,
}: {
  open: boolean;
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
  narrow?: boolean;
  /** 내부 탭·섹션 전환이 있는 모달용 — 내용 높이와 무관하게 크기 고정(본문만 스크롤) */
  fixedHeight?: boolean;
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
      <div
        className={`modal ${wide ? "wide" : ""} ${narrow ? "narrow" : ""} ${
          fixedHeight ? "fixed-h" : ""
        }`}
      >
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
    // 드롭다운 '내부' 스크롤(긴 목록)은 닫지 않는다 — 바깥(모달 본문 등) 스크롤에만 닫아 앵커 이탈 방지
    const onScroll = (e: Event) => {
      if (menuRef.current?.contains(e.target as Node)) return;
      setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", onReflow);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      document.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", onReflow);
      window.removeEventListener("scroll", onScroll, true);
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
