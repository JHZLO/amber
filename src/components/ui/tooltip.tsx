import * as React from "react";
import { Tooltip as TooltipPrimitive } from "radix-ui";

import { cn } from "@/lib/utils";

/* 툴팁 — Radix Tooltip 에 amber 의 `.tip` 판을 씌운다.
   마우스는 350ms 머물러야 뜨고(DESIGN.md §8), 키보드 포커스는 바로 뜬다(Radix 기본값).
   누르면 사라진다. 판은 pointer-events:none 이라 내용 위로 손을 옮겨도 잡히지 않는다. */
const TOOLTIP_DELAY = 350;

function Tooltip(props: React.ComponentProps<typeof TooltipPrimitive.Root>) {
  return (
    <TooltipPrimitive.Provider delayDuration={TOOLTIP_DELAY}>
      <TooltipPrimitive.Root {...props} />
    </TooltipPrimitive.Provider>
  );
}

const TooltipTrigger = TooltipPrimitive.Trigger;

function TooltipContent({
  className,
  sideOffset = 6,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Content>) {
  return (
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Content
        data-slot="tooltip-content"
        side="bottom"
        sideOffset={sideOffset}
        className={cn("tip", className)}
        {...props}
      />
    </TooltipPrimitive.Portal>
  );
}

export { Tooltip, TooltipContent, TooltipTrigger };
