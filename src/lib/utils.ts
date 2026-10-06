import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** shadcn 컴포넌트의 className 합치기 — 뒤에 온 유틸리티가 앞의 같은 축을 이긴다 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
