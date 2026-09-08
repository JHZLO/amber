// 앱 공용 마크다운 렌더러: GFM + ```mermaid``` 코드펜스를 다이어그램으로, ```svg 펜스를 인라인 그래픽으로 렌더.
// 호출부는 기존처럼 .markdown 컨테이너로 감싸서 쓴다 (여긴 래퍼를 추가하지 않음).

import { memo } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import { Mermaid } from "./Mermaid";
import { Icon, type IconName } from "../icons";
import { t, type MsgKey } from "../lib/i18n";
import { remarkAlerts, type AlertKind } from "../lib/mdAlerts";
import { remarkSecRefs } from "../lib/mdSecRefs";
import { looksLikeMermaid } from "../lib/mdMermaid";
import { remarkSvgHtml, sanitizeSvg } from "../lib/mdSvg";
// 블록마다 소스 좌표를 심는다 — 읽기 모드 드래그를 마크다운 구간으로 되돌리는 좌표계(§7)
import { srcAttrs } from "../lib/mdBlocks";

// pre>code 의 AST 노드에서 mermaid 여부/원문을 뽑기 위한 최소 형태
type HastNode = { tagName?: string; value?: string; children?: HastNode[] };
type PreNode = {
  children?: Array<HastNode & { properties?: { className?: unknown } }>;
};

/** 코드 노드의 원문 — 자손 텍스트를 전부 이어 붙인다. rehype-highlight 가 토큰을 span 으로 쪼갠
 *  뒤에도 원문이 필요하다(첫 자식만 읽으면 하이라이트된 블록에서 빈 문자열이 나온다 — svg 가 그랬다). */
function codeText(node: HastNode | undefined): string {
  if (!node) return "";
  if (typeof node.value === "string") return node.value;
  return (node.children ?? []).map(codeText).join("");
}

function isMermaid(className: unknown): boolean {
  return Array.isArray(className) && className.includes("language-mermaid");
}

// 알림 종류별 아이콘 — GitHub 과 같은 대응. 색(--alert-*)과 함께 종류를 나르므로 5종을 다 나눈다.
const ALERT_ICONS: Record<AlertKind, IconName> = {
  NOTE: "info",
  TIP: "lightbulb",
  IMPORTANT: "message",
  WARNING: "alert-triangle",
  CAUTION: "alert-octagon",
};

// t() 는 키 유니온으로 타입 체크되므로 문자열을 조립하지 않고 리터럴로 적어 둔다
const ALERT_LABELS: Record<AlertKind, MsgKey> = {
  NOTE: "common.alert.note",
  TIP: "common.alert.tip",
  IMPORTANT: "common.alert.important",
  WARNING: "common.alert.warning",
  CAUTION: "common.alert.caution",
};

/** ```ts 처럼 붙인 언어 이름 (없으면 빈 문자열) — 코드블록 헤더에 표시한다 */
function fenceLang(className: unknown): string {
  if (!Array.isArray(className)) return "";
  const hit = className.find(
    (c) => typeof c === "string" && c.startsWith("language-"),
  );
  return typeof hit === "string" ? hit.slice("language-".length) : "";
}

/** blockquote 의 data-alert 속성(remarkAlerts 가 심는다)에서 종류를 읽는다 */
function alertKind(props: unknown): AlertKind | null {
  const kind = (props as { "data-alert"?: string })["data-alert"];
  return kind && kind in ALERT_ICONS ? (kind as AlertKind) : null;
}

// memo: 부모(예: NotesView 스크롤 스파이)가 재렌더돼도 본문 문자열이 그대로면
// 마크다운 재파싱/mermaid 재렌더를 건너뛴다 → 스크롤 시 깜빡임 제거.
export const Markdown = memo(function Markdown({
  children,
}: {
  children: string;
}) {
  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm, remarkAlerts, remarkSecRefs, remarkSvgHtml]}
      // 문법 하이라이트 — 클래스(hljs-*)만 붙이고 색은 styles.css 의 토큰이 정한다.
      // detect: false 로 **언어를 적은 블록만** 칠한다(추측이 틀리면 색이 엉뚱해진다).
      // svg·mermaid 는 칠하지 않는다 — highlight.js 가 svg 를 xml 별칭으로 알아 토큰으로 쪼개면
      // 그림으로 바꿀 원문을 잃는다(mermaid 도 같은 위험).
      rehypePlugins={[
        [rehypeHighlight, { detect: false, ignoreMissing: true, plainText: ["svg", "mermaid"] }],
      ]}
      components={{
        // `> [!NOTE]` 인용구 → 콜아웃. 종류는 remarkAlerts 가 data-alert 로 넘긴다.
        blockquote(props) {
          const kind = alertKind(props);
          const src = srcAttrs(props.node, children);
          if (!kind) return <blockquote {...src}>{props.children}</blockquote>;
          return (
            <div className={`md-alert md-alert-${kind.toLowerCase()}`} {...src}>
              <div className="md-alert-title">
                <Icon name={ALERT_ICONS[kind]} size={14} />
                {t(ALERT_LABELS[kind])}
              </div>
              {props.children}
            </div>
          );
        },
        // 아래 블록들은 렌더를 바꾸지 않는다 — 소스 좌표만 얹는다(읽기 모드 드래그 → 부분 수정)
        p(props) {
          return <p {...srcAttrs(props.node, children)}>{props.children}</p>;
        },
        h1(props) {
          return <h1 {...srcAttrs(props.node, children)}>{props.children}</h1>;
        },
        h2(props) {
          return <h2 {...srcAttrs(props.node, children)}>{props.children}</h2>;
        },
        h3(props) {
          return <h3 {...srcAttrs(props.node, children)}>{props.children}</h3>;
        },
        li(props) {
          return <li {...srcAttrs(props.node, children)}>{props.children}</li>;
        },
        table(props) {
          return <table {...srcAttrs(props.node, children)}>{props.children}</table>;
        },
        // 코드블록은 pre 레벨에서 가로채, mermaid 면 다이어그램으로 대체 (그 외엔 기본 pre)
        pre(props) {
          const node = (props as { node?: PreNode }).node;
          const code = node?.children?.[0];
          const lang = fenceLang(code?.properties?.className);
          const text = codeText(code);
          // ```mermaid 로 열었으면 그대로, **언어를 안 적었으면** 첫 줄로 알아본다 —
          // 모델이 태그를 빠뜨린 노트의 다이어그램이 코드블록으로 굳어 있지 않게 (mdMermaid.ts)
          const isDiagram =
            code?.tagName === "code" &&
            (isMermaid(code.properties?.className) ||
              (!lang && looksLikeMermaid(text)));
          if (isDiagram) {
            return <Mermaid chart={text.trim()} />;
          }
          // ```svg — 차트 같은 그래픽. 펜스가 컨벤션이고(context/note-compose.md), 펜스 없는 raw <svg> 는
          // remarkSvgHtml 이 같은 모양으로 바꿔 여기로 온다.
          if (code?.tagName === "code" && lang === "svg") {
            const html = sanitizeSvg(text);
            // <svg> 를 못 찾으면 아래 코드블록으로 떨어진다 — 그림으로 못 바꿨다고 내용을 숨기지 않는다
            if (html !== null) {
              return (
                <figure
                  className="md-svg"
                  {...srcAttrs(props.node, children)}
                  dangerouslySetInnerHTML={{ __html: html }}
                />
              );
            }
          }
          // macOS 창 크롬 — 신호등 + 언어 라벨을 얹고 코드는 그 아래.
          return (
            <div className="code-win" {...srcAttrs(props.node, children)}>
              <div className="code-win-bar" aria-hidden="true">
                <span className="code-win-dot" />
                <span className="code-win-dot" />
                <span className="code-win-dot" />
                {lang && <span className="code-win-lang">{lang}</span>}
              </div>
              <pre>{props.children}</pre>
            </div>
          );
        },
      }}
    >
      {children}
    </ReactMarkdown>
  );
});
