// 앱 공용 마크다운 렌더러: GFM + ```mermaid``` 코드펜스를 다이어그램으로 렌더.
// 호출부는 기존처럼 .markdown 컨테이너로 감싸서 쓴다 (여긴 래퍼를 추가하지 않음).

import { memo } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Mermaid } from "./Mermaid";

// pre>code 의 AST 노드에서 mermaid 여부/원문을 뽑기 위한 최소 형태
type PreNode = {
  children?: Array<{
    tagName?: string;
    properties?: { className?: unknown };
    children?: Array<{ value?: string }>;
  }>;
};

function isMermaid(className: unknown): boolean {
  return Array.isArray(className) && className.includes("language-mermaid");
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
      remarkPlugins={[remarkGfm]}
      components={{
        // 코드블록은 pre 레벨에서 가로채, mermaid 면 다이어그램으로 대체 (그 외엔 기본 pre)
        pre(props) {
          const node = (props as { node?: PreNode }).node;
          const code = node?.children?.[0];
          if (code?.tagName === "code" && isMermaid(code.properties?.className)) {
            const text = code.children?.[0]?.value ?? "";
            return <Mermaid chart={String(text).trim()} />;
          }
          return <pre>{props.children}</pre>;
        },
      }}
    >
      {children}
    </ReactMarkdown>
  );
});
