// mermaid 다이어그램 렌더러. mermaid 는 무거우므로 동적 import 로 첫 다이어그램이 보일 때만 로드.
// 파싱 실패 시 원문 코드를 그대로 보여줘(폴백) 내용 유실이 없게 한다.

import { useEffect, useState } from "react";
import { Icon } from "../icons";
import { MermaidZoom } from "./MermaidZoom";

// mermaid 모듈 캐시 + 1회 초기화 (DiagramCanvas 등 다른 렌더러도 공유)
export type MermaidApi = {
  initialize: (c: Record<string, unknown>) => void;
  render: (id: string, text: string) => Promise<{ svg: string }>;
};
let mermaidPromise: Promise<MermaidApi> | null = null;
export function getMermaid(): Promise<MermaidApi> {
  if (!mermaidPromise) {
    mermaidPromise = import("mermaid").then((m) => {
      const api = m.default as unknown as MermaidApi;
      api.initialize({
        startOnLoad: false,
        theme: "neutral", // 앱의 흑백 미니멀 톤과 어울림
        securityLevel: "strict", // 라벨을 HTML 로 해석하지 않음(XSS 방지)
        suppressErrorRendering: true, // 실패 시 mermaid 가 에러 SVG 를 DOM 에 심지 않게
        fontFamily: "var(--font)",
      });
      return api;
    });
  }
  return mermaidPromise;
}

let seq = 0;

/** LLM/외부 도구가 자주 쓰는 잘못된 mermaid 패턴 자동 복구.
 *  mermaid 는 라벨 안 큰따옴표의 백슬래시 이스케이프(\")를 지원하지 않는다 → #quot; 로 치환. */
export function repairMermaid(chart: string): string {
  return chart.replace(/\\"/g, "#quot;");
}

/** 렌더 시도: 원문 실패 시 자동 복구본으로 1회 재시도 */
export async function renderMermaid(
  id: string,
  chart: string,
): Promise<{ svg: string }> {
  const mermaid = await getMermaid();
  try {
    return await mermaid.render(id, chart);
  } catch (e) {
    const repaired = repairMermaid(chart);
    if (repaired !== chart) return mermaid.render(`${id}r`, repaired);
    throw e;
  }
}

export function Mermaid({ chart }: { chart: string }) {
  const [svg, setSvg] = useState("");
  const [failed, setFailed] = useState(false);
  const [errMsg, setErrMsg] = useState("");
  const [zoom, setZoom] = useState(false);

  useEffect(() => {
    let alive = true;
    // 렌더마다 고유 id — 빠른 연속 렌더(라이브 프리뷰)에서 mermaid 내부 임시 노드 충돌 방지
    const id = `mmd-${(seq += 1)}`;
    renderMermaid(id, chart)
      .then(({ svg }) => {
        if (alive) {
          setSvg(svg); // 성공 시에만 교체 → 편집 중 문법이 잠깐 깨져도 마지막 정상 렌더 유지
          setFailed(false);
        }
      })
      .catch((e: unknown) => {
        if (alive) {
          setFailed(true);
          setErrMsg(e instanceof Error ? e.message : String(e));
        }
      });
    return () => {
      alive = false;
    };
  }, [chart]);

  // 한 번도 성공 못 했을 때만 폴백/로딩 (렌더 실패해도 내용은 보이게)
  if (!svg) {
    if (failed) {
      return (
        <div className="mermaid-fail">
          <div className="mermaid-fail-head">
            mermaid 문법 오류 — 렌더하지 못해 원본 코드를 표시해요
          </div>
          {errMsg && <div className="mermaid-fail-msg">{errMsg}</div>}
          <pre className="mermaid-fallback">
            <code>{chart}</code>
          </pre>
        </div>
      );
    }
    return <div className="mermaid-loading">다이어그램 렌더링 중…</div>;
  }
  return (
    <>
      <div
        className="mermaid-rendered"
        onClick={() => setZoom(true)}
        title="클릭하면 확대해서 볼 수 있어요"
      >
        {/* strict 모드로 mermaid 가 이미 살균한 SVG */}
        <div
          className="mermaid-svg"
          dangerouslySetInnerHTML={{ __html: svg }}
        />
        {failed && (
          <span className="mermaid-error-badge">
            문법 오류 — 마지막 정상 상태 표시 중
          </span>
        )}
        <span className="mermaid-expand">
          <Icon name="expand" size={13} />
          확대
        </span>
      </div>
      <MermaidZoom svg={svg} open={zoom} onClose={() => setZoom(false)} />
    </>
  );
}
