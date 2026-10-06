import * as React from "react";
import { Checkbox as CheckboxPrimitive } from "radix-ui";

import { cn } from "@/lib/utils";
import { Icon } from "@/icons";

/* 체크박스 — Radix Checkbox 에 amber 의 `.checkbox` 원을 씌운다(꺼짐=아웃라인, 켜짐=두꺼운 유리+체크).
   체크 표시는 Indicator 가 아니라 늘 그린다: 꺼져 있을 때는 글자색이 투명이고, hover 에서 옅게
   비치는 힌트까지 CSS 가 색으로 정한다(styles.css .checkbox). */
function Checkbox({ className, checked, ...props }: React.ComponentProps<typeof CheckboxPrimitive.Root>) {
  return (
    <CheckboxPrimitive.Root
      data-slot="checkbox"
      checked={checked}
      className={cn("checkbox", checked === true && "checked", className)}
      {...props}
    >
      <Icon name="check" size={12} />
    </CheckboxPrimitive.Root>
  );
}

export { Checkbox };
