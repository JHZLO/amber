import * as React from "react";
import { useLayoutEffect, useRef } from "react";
import { Dialog as DialogPrimitive } from "radix-ui";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "@/lib/utils";

/* 대화창 — Radix Dialog 에 amber 의 `.overlay > .modal` 판을 씌운다(모양은 styles.css).

   ui.tsx 의 Modal 은 이걸 **non-modal**(`modal={false}`)로 연다. modal 모드는 body 에
   pointer-events:none 과 aria-hidden 을 걸어서, 창 안에서 여는 body portal(경로 고르기 같은 우리
   드롭다운)이 눌리지 않는다. 대신 Tab 가두기는 Modal 이 직접 한다. Esc 를 맨 위 창만 받는 것,
   제목 연결은 Radix 가 한다. 닫힌 뒤 포커스 복귀는 Trigger 없이 상태로 여닫아서 Radix 가 못 하고,
   ui.tsx 의 useReturnFocus 가 한다.

   non-modal 에서는 Radix 의 Overlay 가 그려지지 않으므로 배경판은 그냥 div 다. 그 위를 눌러도
   닫히지 않게 하는 건 호출부의 onInteractOutside 몫이다(DESIGN.md §8 창 닫기). */
const modalVariants = cva("modal", {
  variants: {
    size: { default: "", wide: "wide", narrow: "narrow", settings: "settings" },
    fixedHeight: { true: "fixed-h", false: "" },
  },
  defaultVariants: { size: "default", fixedHeight: false },
});

/** Portal 에 container 를 직접 준다. 안 주면 Radix Portal 이 한 박자 늦게(마운트 뒤 상태 갱신으로)
 *  붙어서, 창을 여는 순간 도는 호출부의 useEffect 가 아직 비어 있는 ref 를 본다
 *  — 확대 뷰어의 휠 줌이 그렇게 붙지 않았다. container 가 있으면 같은 렌더에 붙는다. */
const portalContainer = () => (typeof document === "undefined" ? undefined : document.body);

const Dialog = DialogPrimitive.Root;
const DialogTitle = DialogPrimitive.Title;
const DialogDescription = DialogPrimitive.Description;
const DialogClose = DialogPrimitive.Close;

/** 창이 닫히면 열기 직전에 포커스가 있던 곳으로 돌려보낸다.
 *  Radix 는 Dialog.Trigger 로 연 창만 트리거에 포커스를 돌려준다. 우리 창은 전부 상태로 열고 닫아서
 *  (Trigger 가 없다) 그대로 두면 포커스가 <body> 로 떨어지고, Tab 이 문서 처음부터 다시 시작한다.
 *  확인 창을 닫으면 쓰던 입력칸으로 돌아가야 한다. */
function useReturnFocus(open: boolean) {
  const prev = useRef<HTMLElement | null>(null);
  // 레이아웃 단계에서 잰다 — Radix 가 창 안으로 포커스를 옮기기(FocusScope 의 effect) 전이다
  useLayoutEffect(() => {
    if (open) prev.current = document.activeElement as HTMLElement | null;
  }, [open]);
  return (e: Event) => {
    e.preventDefault();
    const el = prev.current;
    prev.current = null;
    if (el?.isConnected) el.focus();
  };
}

function DialogContent({
  className,
  size,
  fixedHeight,
  children,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Content> & VariantProps<typeof modalVariants>) {
  return (
    <DialogPrimitive.Portal container={portalContainer()}>
      <div className="overlay" data-slot="dialog-overlay">
        <DialogPrimitive.Content
          data-slot="dialog-content"
          className={cn(modalVariants({ size, fixedHeight }), className)}
          {...props}
        >
          {children}
        </DialogPrimitive.Content>
      </div>
    </DialogPrimitive.Portal>
  );
}

export {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogTitle,
  modalVariants,
  portalContainer,
  useReturnFocus,
};
