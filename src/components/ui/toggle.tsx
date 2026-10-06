import * as React from "react";
import { Toggle as TogglePrimitive } from "radix-ui";

import { cn } from "@/lib/utils";

/* 켜고 끄는 버튼 — Radix Toggle(aria-pressed, data-state="on"). 모양은 호출부의 amber 클래스 */
function Toggle({ className, ...props }: React.ComponentProps<typeof TogglePrimitive.Root>) {
  return <TogglePrimitive.Root data-slot="toggle" className={cn(className)} {...props} />;
}

export { Toggle };
