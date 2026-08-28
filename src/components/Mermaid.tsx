// mermaid 다이어그램 렌더러. mermaid 는 무거우므로 동적 import 로 첫 다이어그램이 보일 때만 로드.
// 파싱 실패 시 원문 코드를 그대로 보여줘(폴백) 내용 유실이 없게 한다.

import { useEffect, useState } from "react";
import { Icon } from "../icons";
import { MermaidZoom } from "./MermaidZoom";
import { t } from "../lib/i18n";
import { errText } from "../lib/errors";
import {
  getDiagramLayout,
  useDiagramLayout,
  type DiagramLayout,
} from "../lib/diagramLayout";

// mermaid 모듈 캐시 + 1회 초기화 (DiagramCanvas 등 다른 렌더러도 공유)
export type MermaidApi = {
  initialize: (c: Record<string, unknown>) => void;
  render: (id: string, text: string) => Promise<{ svg: string }>;
  registerLayoutLoaders: (loaders: unknown[]) => void;
};
const BASE_CONFIG = {
  startOnLoad: false,
  theme: "neutral", // 앱의 흑백 미니멀 톤과 어울림
  securityLevel: "strict", // 라벨을 HTML 로 해석하지 않음(XSS 방지)
  suppressErrorRendering: true, // 실패 시 mermaid 가 에러 SVG 를 DOM 에 심지 않게
  fontFamily: "var(--font)",
  // 시퀀스 다이어그램: 긴 Note·메시지를 줄바꿈한다. 기본값(wrap: false)이면 상자 폭을
  // 넘긴 글자가 **잘리지 않고 삐져나와** 양옆이 잘린 것처럼 보인다 — 한글은 같은 글자
  // 수로도 폭이 넓어 특히 자주 걸린다(`Note over A,B` 는 두 참여자 사이 폭에 묶인다).
  sequence: { wrap: true },
};

// mermaid 설정은 전역(싱글턴)이라 레이아웃도 전역이다. 마지막으로 적용한 값을 들고
// 있다가 달라졌을 때만 재초기화한다 — initialize 는 부분 병합이 아니라 BASE 를 통째로
// 다시 넘겨, 재호출이 다른 설정을 흘리지 않게 한다.
let appliedLayout: DiagramLayout | null = null;
function applyLayout(api: MermaidApi, layout: DiagramLayout): void {
  if (appliedLayout === layout) return;
  api.initialize({ ...BASE_CONFIG, layout });
  appliedLayout = layout;
}

let mermaidPromise: Promise<MermaidApi> | null = null;
export function getMermaid(): Promise<MermaidApi> {
  if (!mermaidPromise) {
    mermaidPromise = Promise.all([
      import("mermaid"),
      import("@mermaid-js/layout-elk"),
    ]).then(([m, elk]) => {
      const api = m.default as unknown as MermaidApi;
      // ELK 를 레이아웃 후보로 등록. 등록 자체는 가볍다(로더만 담긴 배열) —
      // 실제 elkjs 번들은 ELK 로 첫 렌더를 할 때 lazy 로 받아온다.
      api.registerLayoutLoaders(elk.default);
      applyLayout(api, getDiagramLayout());
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
  applyLayout(mermaid, getDiagramLayout()); // 사용자가 그새 엔진을 바꿨을 수 있다
  try {
    return await mermaid.render(id, chart);
  } catch (e) {
    const repaired = repairMermaid(chart);
    if (repaired !== chart) return mermaid.render(`${id}r`, repaired);
    throw e;
  }
}

export function Mermaid({
  chart,
  onAsk,
}: {
  chart: string;
  /** 있으면 다이어그램에 '질문' 칩이 붙는다 — 누르면 mermaid 소스를 그대로 넘긴다.
   *  소스를 넘기는 이유: 렌더된 SVG 라벨만으로는 화살표 방향·순서 같은 구조가 안 읽힌다. */
  onAsk?: (source: string) => void;
}) {
  const [svg, setSvg] = useState("");
  const [failed, setFailed] = useState(false);
  const [errMsg, setErrMsg] = useState("");
  const [zoom, setZoom] = useState(false);
  const layout = useDiagramLayout(); // 엔진을 바꾸면 노트 안 다이어그램도 다시 그린다

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
          setErrMsg(errText(e));
        }
      });
    return () => {
      alive = false;
    };
  }, [chart, layout]);

  // 한 번도 성공 못 했을 때만 폴백/로딩 (렌더 실패해도 내용은 보이게)
  if (!svg) {
    if (failed) {
      return (
        <div className="mermaid-fail">
          <div className="mermaid-fail-head">{t("diagrams.mmd.failHead")}</div>
          {errMsg && <div className="mermaid-fail-msg">{errMsg}</div>}
          <pre className="mermaid-fallback">
            <code>{chart}</code>
          </pre>
        </div>
      );
    }
    return <div className="mermaid-loading">{t("diagrams.mmd.rendering")}</div>;
  }
  return (
    <>
      <div
        className="mermaid-rendered"
        onClick={() => setZoom(true)}
        title={t("diagrams.mmd.clickToZoom")}
      >
        {/* strict 모드로 mermaid 가 이미 살균한 SVG */}
        <div
          className="mermaid-svg"
          dangerouslySetInnerHTML={{ __html: svg }}
        />
        {failed && (
          <span className="mermaid-error-badge">
            {t("diagrams.mmd.staleBadge")}
          </span>
        )}
        {onAsk && (
          <button
            className="mermaid-ask"
            title={t("diagrams.mmd.ask")}
            onClick={(e) => {
              e.stopPropagation(); // 컨테이너 클릭은 확대다 — 질문은 확대를 열지 않는다
              onAsk(chart);
            }}
          >
            <Icon name="message" size={12} />
            {t("diagrams.mmd.askShort")}
          </button>
        )}
        <span className="mermaid-expand">
          <Icon name="expand" size={13} />
          {t("diagrams.mmd.expand")}
        </span>
      </div>
      <MermaidZoom svg={svg} open={zoom} onClose={() => setZoom(false)} />
    </>
  );
}
