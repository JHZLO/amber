// mermaid → SVG (merman — mermaid 와 같은 결과를 목표로 하는 Rust 포트, Zed 도 쓴다).
// 지금 앱(components/Mermaid.tsx)의 설정을 따른다: 테마 neutral, 레이아웃 ELK, 시퀀스 줄바꿈,
// 실패하면 \" 를 #quot; 로 바꿔 한 번 더(repairMermaid). 앱과 얼마나 같은지는 bin/mermaid-check 가 잰다.

use std::sync::atomic::{AtomicBool, Ordering};

use merman::{
    MermaidEngine, OperationControl, RenderOutput, RenderRequest, Renderer,
    SourcePresentationConfig, SvgRequest,
};

/// 배치 엔진(lib/diagramLayout.ts) — 앱 전체에 하나다. 캔버스에서 고르면 노트 안 그림도 같은 엔진으로 그린다
static DAGRE: AtomicBool = AtomicBool::new(false);

/// "elk" | "dagre" (설정 키 amber.diagram.layout). 모르는 값은 elk
pub fn set_layout(name: &str) {
    DAGRE.store(name == "dagre", Ordering::Relaxed);
}

pub fn layout() -> &'static str {
    if DAGRE.load(Ordering::Relaxed) {
        "dagre"
    } else {
        "elk"
    }
}

pub fn to_svg(source: &str) -> Result<String, String> {
    match render(source) {
        Err(e) if source.contains("\\\"") => {
            render(&source.replace("\\\"", "#quot;")).map_err(|_| e)
        }
        // `$$…$$` 는 수식인데 resvg 길로는 수식을 못 그린다. 노트에선 대개 CGLIB 클래스 이름(`Foo$$SpringCGLIB$$0`)이라
        // 글자 그대로 보이게 한다 — 지금 앱은 이걸 기울인 수식 글자로 그린다(의도와 다른 쪽은 지금 앱이다)
        Err(e) if source.contains("$$") => render(&source.replace('$', "#36;")).map_err(|_| e),
        other => other,
    }
}

fn render(source: &str) -> Result<String, String> {
    let site = SourcePresentationConfig::from_value(serde_json::json!({
        "theme": "neutral",
        "layout": layout(),
        "securityLevel": "strict",
        "fontFamily": "Apple SD Gothic Neo, sans-serif",
        "sequence": { "wrap": true },
    }));
    let renderer =
        Renderer::new().with_presentation_engine(MermaidEngine::new().with_site_config(site));
    // resvg 는 HTML 라벨(foreignObject)을 못 그린다 — 라벨을 SVG 글자로 내는 길을 고른다
    let request = SvgRequest {
        pipeline: Some(merman::svg::SvgPipeline::resvg_safe()),
        ..Default::default()
    };
    match renderer.render(RenderRequest::svg(source, OperationControl::new(), request)) {
        Ok(RenderOutput::Svg(Some(svg))) => Ok(svg.svg().to_owned()),
        Ok(_) => Err("mermaid 다이어그램이 아니다".into()),
        Err(e) => Err(e.to_string()),
    }
}
