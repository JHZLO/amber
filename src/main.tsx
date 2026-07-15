import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import { Widget } from "./Widget";
import { initTheme } from "./lib/theme";
import "./styles.css";

// 렌더 전에 테마 적용(플래시 방지). 메인/위젯 모두 동일 적용
initTheme();

// 창 label 로 메인/위젯 분기 (단일 엔트리)
const isWidget = getCurrentWindow().label === "widget";
if (isWidget) document.documentElement.classList.add("widget-root");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isWidget ? <Widget /> : <App />}</React.StrictMode>,
);
