import * as React from "react";
import { Select as SelectPrimitive } from "radix-ui";

import { cn } from "@/lib/utils";
import { Icon } from "@/icons";

/* 드롭다운 — Radix Select 에 amber 의 `.select-trigger` / `.select-menu` / `.select-item` 을 씌운다.
   배치는 popper(트리거 아래, 모자라면 위로 뒤집기)이고 너비는 트리거보다 좁아지지 않는다.
   모양은 styles.css 가 정본이다 — Radix 가 붙이는 상태는 data 속성(`data-highlighted`,
   `data-state="checked"`)이라 거기에 맞춘 규칙도 styles.css 에 있다. */
const Select = SelectPrimitive.Root;
const SelectValue = SelectPrimitive.Value;

function SelectTrigger({ className, children, ...props }: React.ComponentProps<typeof SelectPrimitive.Trigger>) {
  return (
    <SelectPrimitive.Trigger data-slot="select-trigger" className={cn("select-trigger", className)} {...props}>
      {children}
      <SelectPrimitive.Icon asChild>
        <svg className="select-caret" width="10" height="6" viewBox="0 0 10 6" aria-hidden>
          <path
            d="M1 1l4 4 4-4"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </SelectPrimitive.Icon>
    </SelectPrimitive.Trigger>
  );
}

function SelectContent({ className, children, ...props }: React.ComponentProps<typeof SelectPrimitive.Content>) {
  return (
    <SelectPrimitive.Portal>
      <SelectPrimitive.Content
        data-slot="select-content"
        position="popper"
        className={cn("select-menu", className)}
        {...props}
      >
        <SelectPrimitive.Viewport>{children}</SelectPrimitive.Viewport>
      </SelectPrimitive.Content>
    </SelectPrimitive.Portal>
  );
}

function SelectItem({ className, children, ...props }: React.ComponentProps<typeof SelectPrimitive.Item>) {
  return (
    <SelectPrimitive.Item data-slot="select-item" className={cn("select-item", className)} {...props}>
      {/* 체크 칸은 모든 항목이 늘 들고 있는 자리다 — 글이 고른 것만 오른쪽으로 밀리지 않게 */}
      <span className="select-check">
        <SelectPrimitive.ItemIndicator>
          <Icon name="check" size={12} />
        </SelectPrimitive.ItemIndicator>
      </span>
      <SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
    </SelectPrimitive.Item>
  );
}

export { Select, SelectContent, SelectItem, SelectTrigger, SelectValue };
