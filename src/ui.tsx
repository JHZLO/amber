// 공유 프레젠테이션 컴포넌트

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";
import { createPortal } from "react-dom";
import type { Confidence, ConceptStatus } from "./types";
import { Icon, type IconName } from "./icons";
import { dateLocale, t } from "./lib/i18n";

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
    <span className="dots" title={t("common.confidence", { n: value })}>
      {[1, 2, 3].map((i) => (
        <span key={i} className={`dot ${i <= value ? "on" : ""}`} />
      ))}
    </span>
  );
}

export function StatusBadge({ status }: { status: ConceptStatus }) {
  return (
    <span className={`badge ${status}`}>
      {status === "learning" ? t("common.status.learning") : t("common.status.learned")}
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

/** 초기 포커스 후보. 헤더 닫기 버튼에 걸리지 않게 푸터·본문 안에서만 찾는다
 *  — 거기에 포커스가 가면 Enter 가 '승인'이 아니라 '닫기'가 돼버린다. */
const MODAL_FOCUSABLE =
  'button:not(:disabled), a[href], [tabindex]:not([tabindex="-1"])';

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
  const boxRef = useRef<HTMLDivElement>(null);

  // 열릴 때 초기 포커스 — 확인 모달이 Esc 로 취소만 되고 Enter 로 승인이 안 되던 문제.
  // 입력이 있는 모달은 손대지 않는다: 이미 autoFocus 로 입력을 잡거나(이름 변경·설정),
  // 사용자가 먼저 타이핑할 자리라 버튼이 포커스를 뺏으면 안 된다.
  useEffect(() => {
    if (!open) return;
    const box = boxRef.current;
    if (!box || box.querySelector("input, textarea")) return;
    const foot = box.querySelector(".modal-foot");
    // 주 액션 = 푸터의 primary, 파괴적 확인이면 danger-ghost (DESIGN.md §8)
    const target =
      foot?.querySelector<HTMLElement>(
        ".btn-primary:not(:disabled), .btn-danger-ghost:not(:disabled)",
      ) ??
      foot?.querySelector<HTMLElement>(MODAL_FOCUSABLE) ??
      box.querySelector(".modal-body")?.querySelector<HTMLElement>(MODAL_FOCUSABLE) ??
      box;
    target.focus();
  }, [open]);

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
        ref={boxRef}
        tabIndex={-1}
        className={`modal ${wide ? "wide" : ""} ${narrow ? "narrow" : ""} ${
          fixedHeight ? "fixed-h" : ""
        }`}
      >
        <div className="modal-head">
          <h2>{title}</h2>
          <button className="icon-btn" onClick={onClose} aria-label={t("common.close")}>
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
  if (m < 1) return t("common.timeago.now");
  if (m < 60) return t("common.timeago.minutes", { m });
  const h = Math.floor(m / 60);
  if (h < 24) return t("common.timeago.hours", { h });
  const d = Math.floor(h / 24);
  if (d < 30) return t("common.timeago.days", { d });
  return new Date(ms).toLocaleDateString(dateLocale());
}

/** 드롭다운 메뉴 배치 상수 — `.select-menu`(styles.css)와 짝을 이룬다 */
const MENU_MAX_H = 260; // 최대 높이. 뷰포트가 좁으면 아래에서 더 줄인다
const MENU_GAP = 6; // 트리거와 메뉴 사이 간격
const MENU_EDGE = 10; // 뷰포트 가장자리에 남길 최소 여백
const MENU_MIN_H = 96; // 뒤집어도 좁을 때의 바닥값(≈항목 3개) — 그 아래로는 스크롤

/** 커스텀 드롭다운 (네이티브 select 대신).
 *  메뉴는 body 로 portal + position:fixed 로 띄워, 모달 등 overflow 컨테이너에
 *  잘리거나 스크롤 높이를 밀어 레이아웃을 흔드는 문제를 원천 차단한다.
 *  아래 공간이 모자라면 위로 뒤집어(flip) 모달 푸터의 저장/닫기 같은 하단 액션을 덮지 않는다. */
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
    top?: number;
    bottom?: number;
    left?: number;
    right?: number;
    minWidth: number;
    maxHeight: number;
  } | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  const place = useCallback(() => {
    const el = triggerRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    // 메뉴가 벗어나면 안 되는 경계. 모달 안이면 '모달 본문' — 뷰포트 기준으로 재면 화면엔 들어가도
    // 푸터의 저장/닫기를 덮어버린다. 모달 밖이면 뷰포트.
    const host = el.closest(".modal-body")?.getBoundingClientRect();
    const limitTop = Math.max(MENU_EDGE, host?.top ?? 0);
    const limitBottom = Math.min(
      window.innerHeight - MENU_EDGE,
      host?.bottom ?? window.innerHeight,
    );
    const roomBelow = limitBottom - r.bottom - MENU_GAP;
    const roomAbove = r.top - limitTop - MENU_GAP;
    // 실제 내용 높이로 판단(메뉴는 open 과 동시에 마운트되므로 measure 가능).
    // 아직 못 쟀으면 최대치로 가정해 보수적으로 뒤집는다.
    const need = Math.min(menuRef.current?.scrollHeight || MENU_MAX_H, MENU_MAX_H);
    // 아래가 모자라고 위가 더 넓을 때만 뒤집는다 — 공간이 되면 늘 아래(예측 가능한 기본값)
    const up = roomBelow < need && roomAbove > roomBelow;
    setPos({
      ...(align === "right"
        ? { right: window.innerWidth - r.right }
        : { left: r.left }),
      ...(up
        ? { bottom: window.innerHeight - r.top + MENU_GAP }
        : { top: r.bottom + MENU_GAP }),
      minWidth: r.width,
      maxHeight: Math.max(MENU_MIN_H, Math.min(MENU_MAX_H, up ? roomAbove : roomBelow)),
    });
  }, [align]);

  // 배치는 paint 전에 끝낸다 — 잘못된 위치가 한 프레임 보이지 않게
  useLayoutEffect(() => {
    if (open) place();
  }, [open, place]);

  useEffect(() => {
    if (!open) return;
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
  }, [open]);

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
        createPortal(
          // pos 계산(useLayoutEffect) 전 한 프레임은 숨긴다 — 측정용으로 마운트만 해둔 상태
          <div
            ref={menuRef}
            className="select-menu"
            style={{
              top: pos?.top,
              bottom: pos?.bottom,
              left: pos?.left,
              right: pos?.right,
              minWidth: pos?.minWidth,
              maxHeight: pos?.maxHeight,
              visibility: pos ? undefined : "hidden",
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
