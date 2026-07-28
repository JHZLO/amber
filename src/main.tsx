import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";
import App from "./App";
import { Widget } from "./Widget";
import { initTheme } from "./lib/theme";
import "./styles.css";

// 렌더 전에 테마 적용(플래시 방지). 메인/위젯 모두 동일 적용
initTheme();

// 외부 링크(노트 마크다운·리포트 등)는 웹뷰 안에서 열면 앱 화면이 그 페이지로 바뀌어 버린다 —
// 가로채서 사용자 기본 브라우저로 보낸다. capture 단계: 렌더러별 onClick 보다 먼저.
document.addEventListener(
  "click",
  (e) => {
    if (e.defaultPrevented || e.metaKey || e.ctrlKey) return;
    const a = (e.target as HTMLElement).closest?.("a[href]");
    const href = a?.getAttribute("href");
    if (!href || !/^(https?:|mailto:)/i.test(href)) return; // 앱 내 앵커/상대경로는 그대로
    e.preventDefault();
    void openUrl(href);
  },
  true,
);

// 창 label 로 메인/위젯 분기 (단일 엔트리)
const isWidget = getCurrentWindow().label === "widget";
if (isWidget) document.documentElement.classList.add("widget-root");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isWidget ? <Widget /> : <App />}</React.StrictMode>,
);
