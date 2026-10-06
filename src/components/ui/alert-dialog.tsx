import * as React from "react";
import { AlertDialog as AlertDialogPrimitive } from "radix-ui";
import type { VariantProps } from "class-variance-authority";

import { cn } from "@/lib/utils";
import { modalVariants } from "@/components/ui/dialog";

/* 확인 창 — Radix AlertDialog 에 Modal 과 같은 `.overlay > .modal` 판을 씌운다.

   AlertDialog 는 언제나 modal 이다: 배경판을 눌러도 닫히지 않고, 열리면 **취소(Cancel)에 초기
   포커스**가 간다. 둘 다 amber 의 파괴적 확인 규칙과 같다(Enter 두 번으로 지워지지 않게).
   확인 창 안에는 입력도 드롭다운도 없으니 modal 의 pointer-events 잠금이 걸릴 일이 없다.

   주의: Radix 의 Action 은 누르면 onOpenChange(false) 도 부른다. 호출부가 onOpenChange 를
   '취소'로 묶어 두므로, 실행 버튼은 Action 이 아니라 평범한 Button 으로 둔다. */
const AlertDialog = AlertDialogPrimitive.Root;
const AlertDialogTitle = AlertDialogPrimitive.Title;
const AlertDialogDescription = AlertDialogPrimitive.Description;
const AlertDialogCancel = AlertDialogPrimitive.Cancel;

function AlertDialogContent({
  className,
  size = "narrow",
  children,
  ...props
}: React.ComponentProps<typeof AlertDialogPrimitive.Content> &
  Pick<VariantProps<typeof modalVariants>, "size">) {
  return (
    <AlertDialogPrimitive.Portal>
      <AlertDialogPrimitive.Overlay className="overlay" data-slot="alert-dialog-overlay">
        <AlertDialogPrimitive.Content
          data-slot="alert-dialog-content"
          className={cn(modalVariants({ size }), className)}
          {...props}
        >
          {children}
        </AlertDialogPrimitive.Content>
      </AlertDialogPrimitive.Overlay>
    </AlertDialogPrimitive.Portal>
  );
}

export {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogTitle,
};
