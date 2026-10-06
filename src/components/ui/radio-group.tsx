import * as React from "react";
import { RadioGroup as RadioGroupPrimitive } from "radix-ui";

import { cn } from "@/lib/utils";

/* 라디오 묶음 — Radix RadioGroup. 화살표로 고르고 Tab 은 고른 하나에만 멈춘다.
   모양은 호출부(ui.tsx 의 OptionCard)가 amber 클래스로 정한다. */
function RadioGroup({ className, ...props }: React.ComponentProps<typeof RadioGroupPrimitive.Root>) {
  return <RadioGroupPrimitive.Root data-slot="radio-group" className={cn(className)} {...props} />;
}

function RadioGroupItem({ className, ...props }: React.ComponentProps<typeof RadioGroupPrimitive.Item>) {
  return <RadioGroupPrimitive.Item data-slot="radio-group-item" className={cn(className)} {...props} />;
}

export { RadioGroup, RadioGroupItem };
