// 노트 그림(```svg, ```mermaid)을 웹 엔진 없이 그린다. svg 는 resvg 로, mermaid 는 crate::mermaid 로
// SVG 를 만든 뒤 같은 길로 간다. 레티나에서 흐리지 않게 2배로 굽고 절반 크기로 놓는다.

use std::sync::{Arc, OnceLock};

use gpui_kit::{Image, ImageFormat};
use resvg::{tiny_skia, usvg};

const SCALE: f32 = 2.0;

#[derive(Clone)]
pub struct Figure {
    pub image: Arc<Image>,
    pub width: f32,
    pub height: f32,
    /// 확대 뷰어가 더 큰 배율로 다시 굽는다
    pub tree: Arc<usvg::Tree>,
}

fn fonts() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    })
    .clone()
}

/// `text` 는 currentColor 가 가리킬 글자색 — 노트 그림은 색을 대부분 currentColor 로 받는다
pub fn parse(source: &str, text: &str) -> Result<usvg::Tree, String> {
    parse_styled(source, text, None)
}

/// css 를 얹어 읽는다 — 다이어그램 캔버스의 포커스 흐림, 찾기 칠(!important 라야 그림 안 규칙을 이긴다)
pub fn parse_styled(source: &str, text: &str, css: Option<String>) -> Result<usvg::Tree, String> {
    // width="100%" 는 브라우저 폭을 따르라는 뜻이라 여기선 viewBox 크기를 쓴다
    let source = source
        .replacen("<svg", &format!("<svg color=\"{text}\""), 1)
        .replace(" width=\"100%\"", "");
    let options = usvg::Options {
        fontdb: fonts(),
        font_family: "Apple SD Gothic Neo".into(),
        style_sheet: css,
        ..Default::default()
    };
    usvg::Tree::from_str(&source, &options).map_err(|e| e.to_string())
}

/// 그림의 한 구간(그림 좌표 x, y, w, h)을 scale 배로 굽는다 — 캔버스가 보이는 곳만 선명하게 다시 굽는다
pub fn raster(
    tree: &usvg::Tree,
    region: (f32, f32, f32, f32),
    scale: f32,
) -> Result<Arc<Image>, String> {
    let (x, y, w, h) = region;
    let (pw, ph) = ((w * scale).ceil() as u32, (h * scale).ceil() as u32);
    let mut pixmap = tiny_skia::Pixmap::new(pw.max(1), ph.max(1)).ok_or("빈 그림")?;
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(scale, scale).pre_translate(-x, -y),
        &mut pixmap.as_mut(),
    );
    let png = pixmap.encode_png().map_err(|e| e.to_string())?;
    Ok(Arc::new(Image::from_bytes(ImageFormat::Png, png)))
}

pub fn svg(source: &str, text: &str) -> Result<Figure, String> {
    let tree = parse(source, text)?;
    let size = tree.size();
    Ok(Figure {
        image: raster(&tree, (0., 0., size.width(), size.height()), SCALE)?,
        width: size.width(),
        height: size.height(),
        tree: Arc::new(tree),
    })
}

/// 지금 앱처럼 밝은(neutral) 테마로 그린다 — 흰 판 위에 놓인다
pub fn mermaid(source: &str) -> Result<Figure, String> {
    svg(&amber_gpui::mermaid::to_svg(source)?, "#333333")
}
