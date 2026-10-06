// 공유 프레젠테이션 컴포넌트

import {
  useEffect,
  useRef,
  useState,
  type ReactElement,
  type ReactNode,
  type RefObject,
} from "react";
import { createPortal } from "react-dom";
import type { Confidence, ConceptStatus } from "./types";
import { Icon, type IconName } from "./icons";
import { dateLocale, t } from "./lib/i18n";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle, useReturnFocus } from "@/components/ui/dialog";
import { Checkbox as CheckboxUI } from "@/components/ui/checkbox";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import {
  Select as SelectRoot,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Toggle } from "@/components/ui/toggle";
import {
  Tooltip as TooltipUI,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";

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

/** AI 응답 대기 공통 로딩 — 생동감 있는 인디터미닛 표시 + 펄스 스파클.
 *  모든 AI 기능(질문·작성·개념 생성/보강)에서 이 컴포넌트로 통일한다.
 *  compact: 스레드 말풍선 등 인라인 자리(중앙정렬·큰 여백 없이 좌측·꽉 찬 바).
 *
 *  indicator:
 *   - "bar"(기본) 트랙을 훑는 스윕 바. 폭이 정해진 자리(모달·좁은 패널)에서 자연스럽다.
 *   - "ring"  도는 원호. **결과가 들어올 자리가 넓게 비어 있는 곳**에 쓴다 — 빈 화면 위의
 *             가로 막대는 진행률처럼 읽혀 '얼마나 남았나'를 잘못 약속한다. */
export function AiThinking({
  label,
  hint,
  compact,
  indicator = "bar",
  activity,
}: {
  label: string;
  hint?: string;
  compact?: boolean;
  indicator?: "bar" | "ring";
  /** 지금 하고 있는 도구 호출 한 줄(파일 읽기·검색). 모션이 아니라 글자 교체라 '모션은 하나' 규칙과 충돌하지 않는다 */
  activity?: string;
}) {
  const ring = indicator === "ring";
  return (
    <div
      className={`ai-thinking ${compact ? "compact" : ""} ${ring ? "ring" : ""}`}
      role="status"
      aria-live="polite"
    >
      {ring && <AiRing />}
      {/* 라벨에 진행 표식을 붙이지 않는다 — **한 대기 상태에 모션은 하나**다(ring 이나 bar).
          둘을 함께 두면 도는 것이 두 개가 되어 어디를 봐야 할지 흐려진다. */}
      <div className="ai-thinking-label">
        <span>{label}</span>
      </div>
      {!ring && (
        <div className="ai-progress" aria-hidden="true">
          {/* compact 전환은 트랙 폭을 240px→전체로 바꾼다 — WKWebView 가 퍼센트 transform
              애니메이션을 시작 시점 크기로 굳힐 수 있어, 리마운트로 새 크기에서 다시 돌린다. */}
          <span key={compact ? "wide" : "narrow"} className="ai-progress-bar" />
        </div>
      )}
      {activity && <div className="ai-thinking-activity">{activity}</div>}
      {hint && <div className="hint ai-thinking-hint">{hint}</div>}
    </div>
  );
}

/** 인디터미닛 원호 — 트랙 원 위에서 호 하나가 돈다.
 *  회전은 transform 하나뿐이라 컴포지터에서 돌고(§9.2), 호 길이는 고정이다:
 *  stroke-dasharray 를 애니메이트하면 진행률처럼 읽히는데 실제로는 아는 바가 없다. */
function AiRing() {
  return (
    <span className="ai-ring" aria-hidden="true">
      <svg viewBox="0 0 24 24">
        <circle className="ai-ring-track" cx="12" cy="12" r="9" />
        <circle className="ai-ring-arc" cx="12" cy="12" r="9" />
      </svg>
    </span>
  );
}

/** 체크박스 primitive — 모노톤 채움/아웃라인 문법(꺼짐=아웃라인, 켜짐=primary 필+체크).
 *  색으로 상태를 칠하지 않는다(.claude/DESIGN.md §3). */
export function Checkbox({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: () => void;
  label?: string;
  disabled?: boolean;
}) {
  return (
    <CheckboxUI
      checked={checked}
      onCheckedChange={() => onChange()}
      aria-label={label}
      disabled={disabled}
    />
  );
}

/** 설정 한 구획 — 눈썹 제목 + (오른쪽 동작) + 설명 + 내용.
 *  설정 화면마다 `.set-head` 를 손으로 조립하던 걸 모은 것이다. 손으로 짜면 어떤 구획은
 *  설명이 있고 어떤 구획은 없고, 동작 버튼 크기가 `btn` 과 `btn-sm` 으로 갈린다(실제로 그랬다). */
export function SetSection({
  title,
  desc,
  action,
  children,
}: {
  title: string;
  desc?: ReactNode;
  /** 오른쪽 끝 동작 하나. 크기는 여기서 정한다 — 호출부가 고르게 두지 않는다 */
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="set-section">
      <div className="set-head">
        <span className="set-eyebrow">{title}</span>
        <span className="spacer" />
        {action}
      </div>
      {desc && <p className="set-desc">{desc}</p>}
      {children}
    </section>
  );
}

/** 라벨 + 컨트롤 + 힌트 한 벌. 여백은 CSS 가 갖는다 —
 *  예전엔 자리마다 `style={{ marginBottom: 0, marginTop: 14 }}` 을 손으로 붙여 줄이 안 맞았다. */
export function SetField({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="field">
      <label>{label}</label>
      {children}
      {hint && <div className="hint">{hint}</div>}
    </div>
  );
}

/** 컨트롤 + 그 옆 동작 하나(경로 입력 + 연결 테스트처럼). 크기와 간격을 한 곳에서 잡는다 */
export function SetInline({ children }: { children: ReactNode }) {
  return <div className="set-inline">{children}</div>;
}

/** 골라 쓰는 카드 묶음 — 라디오다(여럿 중 하나). 화살표로 고르고 Tab 은 고른 한 장에만 멈춘다.
 *  AI 프로바이더 고르기(설정)와 온보딩이 같은 물건을 쓴다: 처음 본 모양이 설정에서도 같아야 한다. */
export function OptionGroup({
  value,
  onValueChange,
  label,
  className,
  children,
}: {
  value: string;
  onValueChange: (v: string) => void;
  label?: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <RadioGroup value={value} onValueChange={onValueChange} aria-label={label} className={className}>
      {children}
    </RadioGroup>
  );
}

/** 카드 한 장. 선택은 색이 아니라 채움/아웃라인(§3) — 고른 것 = 두꺼운 유리 + 채운 점.
 *  고른 상태는 묶음(OptionGroup)의 값이 정하고, 모양은 `[data-state="checked"]` 가 받는다. */
export function OptionCard({
  value,
  name,
  meta,
  sub,
}: {
  value: string;
  name: string;
  /** 이름 옆 작은 글씨 (버전 등) */
  meta?: string;
  /** 아랫줄 (경로 등) */
  sub?: string;
}) {
  return (
    <RadioGroupItem value={value} className="onb-card">
      <span className="onb-dot" />
      <span className="onb-name">{name}</span>
      {meta && <span className="onb-version">{meta}</span>}
      {sub && (
        <span className="onb-path" title={sub}>
          {sub}
        </span>
      )}
    </RadioGroupItem>
  );
}

/** 삭제 확인 — **지우는 동작은 예외 없이 이 문을 지난다**(.claude/DESIGN.md §3 파괴 동작).
 *  되돌릴 수 없는 일에 도달하는 길이 클릭 하나면 안 된다.
 *
 *  Modal 을 직접 짜지 말고 이걸 쓴다. 문구 자리, 버튼 순서, 강조, "되돌릴 수 없어요" 한 줄이
 *  한 곳에 있어야 "어느 화면은 묻고 어느 화면은 안 묻는다"가 생기지 않는다 — 실제로
 *  문답 스레드, 시간표 블록, 저장 프롬프트가 그렇게 조용히 빠져 있었다. */
export function ConfirmDelete({
  open,
  title,
  name,
  body,
  onCancel,
  onConfirm,
}: {
  open: boolean;
  title: string;
  /** 무엇을 지우는지. 본문에서 굵게 나온다 — 이름 없이 "정말 삭제할까요?"만 묻지 않는다 */
  name: string;
  /** `{name}` 자리를 가진 한 문장. 함께 사라지는 것이 있으면 여기서 말한다 */
  body: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const [before, after] = body.split("{name}");
  return (
    <ConfirmShell
      open={open}
      title={title}
      cancelLabel={t("common.cancel")}
      actionLabel={t("common.delete")}
      onCancel={onCancel}
      onAction={onConfirm}
      spacer
    >
      <p style={{ margin: 0 }}>
        {before}
        <b>{name}</b>
        {after}
        <br />
        {t("common.irreversible")}
      </p>
    </ConfirmShell>
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
/** 선택 토글 칩 — AI 모달의 저장 프롬프트·빠른 지시. 누르면 텍스트를 입력칸에 붙이는 대신
 *  '요청에 포함' 상태만 켜고 끈다(합치기는 lib/aiInstruction). 켜짐 = success 채움 + 체크(§3).
 *  `peek` 를 주면 오른쪽에 칸을 나눈 [내용 보기] 버튼이 붙는다 — 저장 프롬프트처럼 이름만으로
 *  무엇이 들어가는지 알 수 없을 때. 토글과 보기가 한 칩 안에서 갈리므로 바깥은 span, 안은 버튼 둘이다. */
export function ChoiceChip({
  label,
  on,
  onToggle,
  icon = "plus",
  peek,
}: {
  label: string;
  on: boolean;
  onToggle: () => void;
  /** 꺼져 있을 때의 앞 아이콘 (켜지면 항상 체크) */
  icon?: IconName;
  /** 내용 보기 버튼 — 툴팁 라벨과 열기 동작 */
  peek?: { label: string; onOpen: () => void };
}) {
  return (
    <span className={`chip chip-choice ${on ? "on" : ""}`}>
      <Toggle pressed={on} onPressedChange={() => onToggle()} className="chip-main">
        <Icon name={on ? "check" : icon} size={12} />
        <span className="chip-label">{label}</span>
      </Toggle>
      {peek && (
        <Tooltip label={peek.label}>
          <button
            type="button"
            className="chip-peek"
            aria-label={peek.label}
            onClick={peek.onOpen}
          >
            <Icon name="eye" size={12} />
          </button>
        </Tooltip>
      )}
    </span>
  );
}

export function Tooltip({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  // 래퍼 span 이 트리거다 — 안쪽 버튼의 포커스(React onFocus 는 거품을 탄다)와 hover 를 함께 받는다.
  // 래퍼는 레이아웃에도 쓰인다(styles.css .tip-wrap 의 flex-shrink).
  return (
    <TooltipUI>
      <TooltipTrigger asChild>
        <span className="tip-wrap">{children}</span>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </TooltipUI>
  );
}

/** 초기 포커스 후보. 헤더 닫기 버튼에 걸리지 않게 푸터·본문 안에서만 찾는다
 *  — 거기에 포커스가 가면 Enter 가 '승인'이 아니라 '닫기'가 돼버린다. */
const MODAL_FOCUSABLE =
  'button:not(:disabled), a[href], [tabindex]:not([tabindex="-1"])';

/** Tab 순환 대상. 초기 포커스와 달리 입력·헤더 닫기 버튼까지 포함한다 */
const MODAL_TRAPPABLE =
  'button:not(:disabled), a[href], input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])';

/** 모든 모달. 판과 레이어는 Radix Dialog(components/ui/dialog), 닫기 규칙은 여기서 건다.
 *  닫기는 X, 취소, Esc 뿐이다 — 바깥(배경판)을 눌러도 닫히지 않는다(DESIGN.md §8 창 닫기). */
export function Modal({
  open,
  title,
  onClose,
  children,
  footer,
  wide,
  narrow,
  fixedHeight,
  settings,
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
  /** 설정 셸 — 넓게 열고 본문을 [왼쪽 내비 | 내용] 두 칸으로 나눈다(본문 패딩은 안쪽이 맡는다) */
  settings?: boolean;
}) {
  const boxRef = useRef<HTMLDivElement>(null);
  // 이 창을 연 뒤 사용자가 직접 입력했는가 — Esc 로 닫기 전에 한 번 묻는 기준.
  // input 이벤트는 **사람이 친 것**에만 난다(React 가 value 를 바꾸는 건 안 난다). 그래서 저장값을 늦게
  // 불러와 채우는 창도 헛경보가 없고, 호출부마다 dirty 를 넘길 필요가 없다.
  const typedRef = useRef(false);
  const [askDiscard, setAskDiscard] = useState(false);
  const returnFocus = useReturnFocus(open);

  useEffect(() => {
    if (!open) return;
    typedRef.current = false;
    setAskDiscard(false);
  }, [open]);

  const markTyped = (e: { target: EventTarget }) => {
    const el = e.target;
    if (
      el instanceof HTMLInputElement ||
      el instanceof HTMLTextAreaElement ||
      el instanceof HTMLSelectElement ||
      (el instanceof HTMLElement && el.isContentEditable)
    ) {
      typedRef.current = true;
    }
  };

  // 열릴 때 초기 포커스 — Radix 기본값(첫 포커스 가능 요소 = 헤더의 X)을 쓰지 않는다.
  // 확인 모달이 Esc 로 취소만 되고 Enter 로 승인이 안 되던 문제가 있었다.
  // 입력이 있는 모달은 손대지 않는다: 이미 autoFocus 로 입력을 잡거나(이름 변경·설정),
  // 사용자가 먼저 타이핑할 자리라 버튼이 포커스를 뺏으면 안 된다.
  // 닫힐 때 이전 포커스로 되돌리는 건 useReturnFocus 가 한다.
  const focusInitial = (e: Event) => {
    e.preventDefault();
    const box = boxRef.current;
    if (!box || box.querySelector("input, textarea")) return;
    const foot = box.querySelector(".modal-foot");
    // 파괴적 확인은 취소에 포커스를 둔다 — Enter 두 번(근육 기억)으로 지워지지 않게.
    // 그 외에는 주 액션(primary)에 둬서 Enter=승인을 유지한다. (macOS 관례)
    const destructive = foot?.querySelector<HTMLElement>(".btn-danger-ghost:not(:disabled)");
    const target =
      (destructive
        ? foot?.querySelector<HTMLElement>(MODAL_FOCUSABLE)
        : foot?.querySelector<HTMLElement>(".btn-primary:not(:disabled)")) ??
      foot?.querySelector<HTMLElement>(MODAL_FOCUSABLE) ??
      box.querySelector(".modal-body")?.querySelector<HTMLElement>(MODAL_FOCUSABLE) ??
      box;
    target.focus();
  };

  // Esc — 겹친 창 중 맨 위 것만 받는 건 Radix 레이어가 한다(위에 뜬 드롭다운, 확인 창, 확대 뷰어가
  // 먼저 닫힌다). 여기서는 입력한 게 있으면 한 번 더 묻기만 한다.
  // X 와 취소는 누른 것 자체가 뜻이라 묻지 않는다(키 하나는 실수로 눌리지만 버튼은 겨냥해야 눌린다).
  const onEscape = (e: KeyboardEvent) => {
    if (typedRef.current) {
      e.preventDefault();
      setAskDiscard(true);
    }
  };

  // 포커스가 모달 밖으로 새지 않게 가둔다 — 뒤에 있는 화면의 버튼이 Tab 으로 잡히면 안 된다.
  // non-modal Dialog 라 Radix 가 가두지 않는다(components/ui/dialog 머리말). 맨 위 창만 가둔다.
  useEffect(() => {
    if (!open) return;
    const h = (e: KeyboardEvent) => {
      if (e.key !== "Tab") return;
      const overlays = document.querySelectorAll(".overlay");
      if (overlays.length > 1 && overlays[overlays.length - 1] !== boxRef.current?.parentElement) {
        return;
      }
      const box = boxRef.current;
      if (!box) return;
      const items = Array.from(box.querySelectorAll<HTMLElement>(MODAL_TRAPPABLE)).filter(
        (el) => el.offsetParent !== null || el === document.activeElement,
      );
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement as HTMLElement | null;
      if (e.shiftKey && (active === first || !box.contains(active))) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [open]);

  // body 로 portal(Radix Portal) — 호출한 자리에 그리면 `.section-wrap.hidden { display: none }` 에 걸려,
  // ⌘1~4 로 섹션을 바꾸는 순간 모달이 **화면에서만 사라지고 state 는 열린 채** 남는다.
  // 바깥(배경판)을 눌러도 닫지 않는다 — AI 작성 지시처럼 길게 쓰던 입력이 창 밖을 한 번 잘못 누른
  // 것으로 통째로 날아갔다. 입력칸에서 드래그로 글을 고르다 커서가 창 밖에서 떨어져도 같은 일이 난다.
  return (
    <Dialog modal={false} open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent
        ref={boxRef}
        size={settings ? "settings" : wide ? "wide" : narrow ? "narrow" : "default"}
        fixedHeight={!!fixedHeight}
        aria-modal="true"
        aria-describedby={undefined}
        onOpenAutoFocus={focusInitial}
        onCloseAutoFocus={returnFocus}
        onEscapeKeyDown={onEscape}
        onInteractOutside={(e) => e.preventDefault()}
        onInputCapture={markTyped}
      >
        <div className="modal-head">
          <DialogTitle asChild>
            <h2>{title}</h2>
          </DialogTitle>
          <Button size="icon" onClick={onClose} aria-label={t("common.close")}>
            <Icon name="x" />
          </Button>
        </div>
        <div className="modal-body">{children}</div>
        {footer && <div className="modal-foot">{footer}</div>}
        {/* 자기 위에 한 겹 더 띄운다 — Esc 는 맨 위 창만 받으니 여기서 Esc 는 '계속 편집'이다 */}
        <DiscardModal
          open={askDiscard}
          onKeep={() => setAskDiscard(false)}
          onDiscard={() => {
            setAskDiscard(false);
            onClose();
          }}
        />
      </DialogContent>
    </Dialog>
  );
}

/** 확인 창 네 종(삭제, 입력 버리기, 미저장 이동, AI 결과 버리기)의 공통 판.
 *  첫 버튼 = 안전한 쪽(Radix AlertDialog.Cancel) — 초기 포커스와 Enter 가 그쪽으로 간다.
 *  X, 취소, Esc 는 전부 onCancel 이다. */
function ConfirmShell({
  open,
  title,
  cancelLabel,
  actionLabel,
  onCancel,
  onAction,
  spacer,
  children,
}: {
  open: boolean;
  title: string;
  cancelLabel: string;
  actionLabel: string;
  onCancel: () => void;
  onAction: () => void;
  /** 버튼을 오른쪽으로 미는 빈칸(삭제 확인) */
  spacer?: boolean;
  /** 본문 한 덩어리 — 화면 낭독기가 '설명'으로 읽는다 */
  children: ReactElement;
}) {
  const returnFocus = useReturnFocus(open);
  return (
    <AlertDialog open={open} onOpenChange={(o) => !o && onCancel()}>
      <AlertDialogContent onCloseAutoFocus={returnFocus}>
        <div className="modal-head">
          <AlertDialogTitle asChild>
            <h2>{title}</h2>
          </AlertDialogTitle>
          <Button size="icon" onClick={onCancel} aria-label={t("common.close")}>
            <Icon name="x" />
          </Button>
        </div>
        <div className="modal-body">
          <AlertDialogDescription asChild>{children}</AlertDialogDescription>
        </div>
        <div className="modal-foot">
          {spacer && <span className="spacer" />}
          <AlertDialogCancel asChild>
            <Button size="sm">{cancelLabel}</Button>
          </AlertDialogCancel>
          <Button variant="danger" size="sm" onClick={onAction}>
            {actionLabel}
          </Button>
        </div>
      </AlertDialogContent>
    </AlertDialog>
  );
}

/** 입력한 내용이 있는 창을 닫으려 할 때의 확인. 파괴적 확인이라 초기 포커스는 '계속 편집'에 간다 */
export function DiscardModal({
  open,
  onKeep,
  onDiscard,
}: {
  open: boolean;
  onKeep: () => void;
  onDiscard: () => void;
}) {
  return (
    <ConfirmShell
      open={open}
      title={t("common.closeDirty.title")}
      cancelLabel={t("common.unsaved.keep")}
      actionLabel={t("common.closeDirty.discard")}
      onCancel={onKeep}
      onAction={onDiscard}
    >
      <p>{t("common.closeDirty.body")}</p>
    </ConfirmShell>
  );
}

/** 미저장 초안이 있는데 다른 곳으로 이동하려 할 때의 확인.
 *  노트·다이어그램·개념·작업폴더 전환이 전부 같은 문장을 쓰므로 한 곳에 둔다. */
export function UnsavedModal({
  open,
  onKeep,
  onDiscard,
}: {
  open: boolean;
  onKeep: () => void;
  onDiscard: () => void;
}) {
  return (
    <ConfirmShell
      open={open}
      title={t("common.unsaved.title")}
      cancelLabel={t("common.unsaved.keep")}
      actionLabel={t("common.unsaved.discard")}
      onCancel={onKeep}
      onAction={onDiscard}
    >
      <p style={{ margin: 0 }}>{t("common.unsaved.body")}</p>
    </ConfirmShell>
  );
}

/** AI 결과 버리기 확인 — 결과가 떠 있거나 아직 쓰는 중인 AI 모달을 닫을 때. 몇 분 걸리고 크레딧을 쓴
 *  생성물이 X 한 번에 사라지면 안 된다(§8 삭제 정책: 복구 가치가 큰 대상만 확인). UnsavedModal 과 같은 구조 —
 *  첫 버튼이 '계속 보기'라 초기 포커스·Enter 가 안전한 쪽으로 간다. */
export function DiscardAiModal({
  open,
  running,
  mode = "close",
  onKeep,
  onDiscard,
}: {
  open: boolean;
  /** 아직 생성 중이면 문구가 "중단하고 버린다"로 바뀐다 */
  running?: boolean;
  /** "discard" = 닫기가 아니라 **버리기 버튼**에서 왔다 — 노트 전문 작성처럼 닫아도 결과가 남는 흐름.
   *  문구가 "닫으면 사라져요" 대신 "초안이 사라져요" 가 되고 실행 버튼은 "버리기" 다 */
  mode?: "close" | "discard";
  onKeep: () => void;
  onDiscard: () => void;
}) {
  return (
    <ConfirmShell
      open={open}
      title={t("common.aiDiscard.title")}
      cancelLabel={t("common.aiDiscard.keep")}
      actionLabel={
        mode === "discard" ? t("common.aiDiscard.discardOnly") : t("common.aiDiscard.discard")
      }
      onCancel={onKeep}
      onAction={onDiscard}
    >
      <p style={{ margin: 0 }}>
        {running
          ? t("common.aiDiscard.bodyRunning")
          : mode === "discard"
            ? t("common.aiDiscard.bodyDiscard")
            : t("common.aiDiscard.body")}
      </p>
    </ConfirmShell>
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
const MENU_GAP = 6; // 트리거와 메뉴 사이 간격
const MENU_EDGE = 10; // 뷰포트(또는 모달 본문) 가장자리에 남길 최소 여백
/** Radix Select 는 빈 문자열을 항목 값으로 받지 않는다(빈 값 = '고르지 않음' 예약).
 *  우리 목록엔 ''(기본값으로 두기)이 실제 선택지로 있어서 안에서만 표식으로 바꿔 쓴다 */
const EMPTY_VALUE = "\u0000empty";
const enc = (v: string) => (v === "" ? EMPTY_VALUE : v);

/** 커스텀 드롭다운(네이티브 select 금지). 판과 키보드는 Radix Select(components/ui/select).
 *  - 미설정은 `placeholder` 로 — '없음' 항목을 목록에 끼워 넣지 않는다(목록엔 고를 값만)
 *  - 모달 안에서는 메뉴가 `.modal-body` 경계를 넘지 않는다 — 넘으면 푸터의 저장/닫기를 덮는다
 *  - 트리거는 type="button" 이다(Radix 기본값) — <form> 안에서 열 때마다 제출되던 버그(DB 연결 모달) */
export function Select<T extends string>({
  value,
  options,
  onChange,
  align = "left",
  block = false,
  placeholder,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  align?: "left" | "right";
  block?: boolean;
  /** value 가 options 에 없을 때(=미설정) 트리거에 보일 문구.
   *  '없음' 항목을 목록에 끼워 넣지 않고도 빈 상태를 표현한다 — 목록은 고를 값만 담는다. */
  placeholder?: string;
}) {
  const triggerRef = useRef<HTMLButtonElement>(null);
  const [boundary, setBoundary] = useState<Element | null>(null);
  const cur = options.find((o) => o.value === value);

  return (
    <div className={`select ${block ? "block" : ""}`}>
      <SelectRoot
        value={cur ? enc(cur.value) : ""}
        onValueChange={(v) => onChange((v === EMPTY_VALUE ? "" : v) as T)}
        onOpenChange={(o) => {
          if (o) setBoundary(triggerRef.current?.closest(".modal-body") ?? null);
        }}
      >
        <SelectTrigger ref={triggerRef} className={cur ? "" : "select-empty"}>
          <SelectValue placeholder={placeholder} />
        </SelectTrigger>
        <SelectContent
          align={align === "right" ? "end" : "start"}
          sideOffset={MENU_GAP}
          collisionPadding={MENU_EDGE}
          collisionBoundary={boundary}
        >
          {options.map((o) => (
            <SelectItem key={o.value} value={enc(o.value)}>
              {o.label}
            </SelectItem>
          ))}
        </SelectContent>
      </SelectRoot>
    </div>
  );
}
