// 노트 그림(```svg, ```mermaid)을 웹 엔진 없이 그린다. svg 는 resvg 로, mermaid 는 crate::mermaid 로
// SVG 를 만든 뒤 같은 길로 간다. 레티나에서 흐리지 않게 2배로 굽고 절반 크기로 놓는다.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use gpui_kit::RenderImage;
use resvg::{tiny_skia, usvg};

const SCALE: f32 = 2.0;

#[derive(Clone)]
pub struct Figure {
    pub image: Arc<RenderImage>,
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

/// 그림의 한 구간(그림 좌표 x, y, w, h)을 scale 배로 굽는다 — 캔버스가 보이는 곳만 선명하게 다시 굽는다.
/// GPUI 가 바로 올리는 BGRA 로 넘긴다 — PNG 로 싸면 압축에 굽는 만큼 들고, 그리기 전에 다시 풀어야 한다
pub fn raster(
    tree: &usvg::Tree,
    region: (f32, f32, f32, f32),
    scale: f32,
) -> Result<Arc<RenderImage>, String> {
    let (x, y, w, h) = region;
    let (pw, ph) = ((w * scale).ceil() as u32, (h * scale).ceil() as u32);
    let mut pixmap = tiny_skia::Pixmap::new(pw.max(1), ph.max(1)).ok_or("빈 그림")?;
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(scale, scale).pre_translate(-x, -y),
        &mut pixmap.as_mut(),
    );
    let (w, h) = (pixmap.width(), pixmap.height());
    let mut data = pixmap.take();
    for px in data.chunks_exact_mut(4) {
        gpui_kit::swap_rgba_pa_to_bgra(px);
    }
    let frame = image::RgbaImage::from_raw(w, h, data).ok_or("그림 크기")?;
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(frame)])))
}

/// 같은 그림은 다시 굽지 않는다 — 노트를 다시 열 때, 고치는 동안 미리보기를 다시 지을 때(바뀐 그림만 새로).
/// 넘치면 비운다(다시 그리면 그만이다)
const CACHE_MAX: usize = 256;

thread_local! {
    static CACHE: RefCell<HashMap<(bool, String, String), Result<Figure, String>>> =
        RefCell::default();
}

fn cached(
    mermaid: bool,
    source: &str,
    text: &str,
    make: impl FnOnce() -> Result<Figure, String>,
) -> Result<Figure, String> {
    let key = (mermaid, source.to_string(), text.to_string());
    if let Some(hit) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return hit;
    }
    let out = make();
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() >= CACHE_MAX {
            c.clear();
        }
        c.insert(key, out.clone());
    });
    out
}

pub fn svg(source: &str, text: &str) -> Result<Figure, String> {
    cached(false, source, text, || build(source, text))
}

/// 아직 없는 그림을 코어마다 나눠 미리 굽는다 — 그림이 스무 장 넘는 노트도 한 장씩 차례로 기다리지 않게.
/// (mermaid 인가, 원문). 그 뒤의 svg()/mermaid() 는 캐시에서 바로 꺼낸다
pub fn prepare(items: &[(bool, String)], text: &str) {
    let todo: Vec<&(bool, String)> = CACHE.with(|c| {
        let c = c.borrow();
        items
            .iter()
            .filter(|(m, src)| {
                let key_text = if *m { "" } else { text };
                !c.contains_key(&(*m, src.clone(), key_text.to_string()))
            })
            .collect()
    });
    if todo.len() < 2 {
        return;
    }
    // 그림마다 드는 시간이 달라 덩어리로 나누지 않고 다음 그림을 하나씩 집어 간다
    let next = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(todo.len());
    let built: Vec<(bool, String, Result<Figure, String>)> = std::thread::scope(|s| {
        let jobs: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((m, src)) = todo.get(i) else {
                            return out;
                        };
                        let fig = if *m {
                            amber_gpui::mermaid::to_svg(src).and_then(|svg| build(&svg, "#333333"))
                        } else {
                            build(src, text)
                        };
                        out.push((*m, src.clone(), fig));
                    }
                })
            })
            .collect();
        jobs.into_iter()
            .flat_map(|j| j.join().unwrap_or_default())
            .collect()
    });
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() + built.len() > CACHE_MAX {
            c.clear();
        }
        for (m, src, out) in built {
            let key_text = if m { "" } else { text };
            c.insert((m, src, key_text.to_string()), out);
        }
    });
}

fn build(source: &str, text: &str) -> Result<Figure, String> {
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
    cached(true, source, "", || {
        build(&amber_gpui::mermaid::to_svg(source)?, "#333333")
    })
}
