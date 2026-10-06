import * as React from "react";
import { Slot } from "radix-ui";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "@/lib/utils";

/* 버튼 — shadcn 의 Button API(variant/size/asChild)에 amber 의 버튼 문법을 얹는다.

   모양은 여기서 유틸리티로 다시 쓰지 않고 styles.css 의 `.btn`, `.icon-btn` 클래스를 붙인다.
   그 클래스에 맥락별 규칙(`.notes-tree-head .icon-btn`, `.todo-row .row-actions .icon-btn.danger:hover`
   등)이 수십 곳 걸려 있고, 유틸리티(레이어 안)로 옮기면 그 규칙들(레이어 밖)과의 우선순위가
   선택자 구체성이 아니라 레이어로 정해져 hover 가 조용히 바뀐다. 정본은 그대로 한 곳이다.

   두 가족이다(DESIGN.md §5):
   - 글자 버튼  size: default | sm         → .btn, .btn.btn-sm
   - 아이콘 버튼 size: icon | icon-sm      → .icon-btn, .icon-btn.sm
   variant 는 가족마다 뜻이 같다: primary = 강조 유리, danger = 손이 닿을 때 빨강,
   ghost = 쉴 때 면이 없는 아이콘 버튼(글자 버튼에는 없다). */
const buttonVariants = cva("", {
  variants: {
    variant: {
      default: "",
      primary: "",
      danger: "",
      ghost: "",
      "ghost-danger": "",
    },
    size: {
      default: "btn",
      sm: "btn btn-sm",
      icon: "icon-btn",
      "icon-sm": "icon-btn sm",
    },
  },
  compoundVariants: [
    { variant: "primary", size: ["default", "sm"], className: "btn-primary" },
    { variant: "danger", size: ["default", "sm"], className: "btn-danger-ghost" },
    { variant: "danger", size: ["icon", "icon-sm"], className: "danger" },
    { variant: "ghost", size: ["icon", "icon-sm"], className: "ghost" },
    { variant: "ghost-danger", size: ["icon", "icon-sm"], className: "ghost danger" },
  ],
  defaultVariants: { variant: "default", size: "default" },
});

type ButtonProps = React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & { asChild?: boolean };

function Button({ className, variant, size, asChild = false, ...props }: ButtonProps) {
  const Comp = asChild ? Slot.Root : "button";
  return (
    <Comp
      data-slot="button"
      className={cn(buttonVariants({ variant, size }), className)}
      {...props}
    />
  );
}

export { Button, buttonVariants, type ButtonProps };
