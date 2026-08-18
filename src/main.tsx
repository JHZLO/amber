import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";
import { initTheme } from "./lib/theme";
import { t } from "./lib/i18n";
import { errText } from "./lib/errors";
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

// 어디서도 catch 하지 않은 rejection 은 조용히 사라진다 — 1인 개발이라 이게 유일한 신고 창구다.
// 화면을 막지 않고 콘솔에만 남긴다(개발자 도구 / `tauri dev` 로그에서 보인다).
window.addEventListener("unhandledrejection", (e) => {
  console.error("[amber] unhandled rejection:", errText(e.reason), e.reason);
});

/** 렌더 중 throw 로 창이 백지가 되는 것만 막는다. 복구는 리로드 한 가지 — 상태를 짐작하지 않는다. */
class ErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { error: Error | null }
> {
  state: { error: Error | null } = { error: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error("[amber] render crash:", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="crash">
        <h1 className="crash-title">{t("common.crash.title")}</h1>
        <p className="crash-hint">{t("common.crash.hint")}</p>
        <pre className="crash-detail">{this.state.error.message}</pre>
        <button className="btn btn-primary btn-sm" onClick={() => location.reload()}>
          {t("common.crash.reload")}
        </button>
      </div>
    );
  }
}

// 창 label 로 메인/위젯 분기 (단일 엔트리)
const isWidget = getCurrentWindow().label === "widget";
if (isWidget) document.documentElement.classList.add("widget-root");

// 동적 import 로 갈라야 위젯 창이 mermaid·elk 를 포함한 메인 앱 번들을 파싱하지 않는다.
// (top-level await 은 tsconfig target ES2020 에서 못 쓰므로 then 체인)
const load: Promise<React.ComponentType> = isWidget
  ? import("./Widget").then((m) => m.Widget)
  : import("./App").then((m) => m.default);

void load.then((Root) => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <ErrorBoundary>
        <Root />
      </ErrorBoundary>
    </React.StrictMode>,
  );
});
