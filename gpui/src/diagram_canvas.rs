// 다이어그램 캔버스 — 지금 앱 DiagramCanvas.tsx(svg-pan-zoom). 휠 = 커서 자리를 붙잡고 확대/축소, 끌기 = 이동,
// 더블클릭 = 확대(Shift = 축소), 맞춤/100%/±, 전체 화면, 배치 엔진(ELK/Dagre). 그림은 다크에서도 흰 판이다.
// 배율 표시는 처음 맞춘 크기가 100% 다(svg-pan-zoom 의 getZoom 과 같은 기준).
//
// 벡터를 어느 배율에서도 선명하게: 그림 전체는 판에 맞춘 해상도로 한 번 굽고(옮기고 키우는 동안 바로 보인다),
// 확대가 멈추면 보이는 곳(+여백)만 그 배율로 다시 구워 위에 얹는다. 굽기는 뒤에서 돈다.
// 문법이 깨지면 마지막으로 그린 그림을 그대로 두고 아래에 오류 줄만 띄운다.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use amber_gpui::diagram_graph::{
    Column, GLabel, Structure, extract_node_id, find_line_for_node_id, neighbors_of, read_structure,
};
use amber_gpui::i18n::{t, t_with};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use resvg::usvg;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::Tag;
use crate::theme::Tokens;

const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 20.;
/// svg-pan-zoom 의 zoomScaleSensitivity — 단추 한 번 1.4배, 더블클릭 1.8배
const SENS: f32 = 0.4;
/// 전체 그림을 굽는 한 변의 상한(픽셀)
const FULL_MAX_PX: f32 = 4096.;
/// 선명하게 다시 구울 때 보이는 곳 둘레에 더 굽는 몫(판 한 변의 비율)
const MARGIN: f32 = 0.5;

/// 판에 맞추는 배율과 가운데 자리 — svg-pan-zoom 의 fit(contain) + center
pub fn fit(view: (f32, f32), size: (f32, f32)) -> (f32, (f32, f32)) {
    let s = (view.0 / size.0).min(view.1 / size.1);
    let s = if s.is_finite() && s > 0. { s } else { 1. };
    (s, ((view.0 - size.0 * s) / 2., (view.1 - size.1 * s) / 2.))
}

/// 판 안의 점 p 를 붙잡고 배율을 old → new 로 바꿀 때의 새 자리
pub fn zoom_at(pan: (f32, f32), old: f32, new: f32, p: (f32, f32)) -> (f32, f32) {
    let r = new / old;
    (p.0 - (p.0 - pan.0) * r, p.1 - (p.1 - pan.1) * r)
}

/// 보이는 곳(그림 좌표)에 둘레를 더하고 그림 경계로 자른 구간 (x, y, w, h)
pub fn visible_region(
    view: (f32, f32),
    pan: (f32, f32),
    scale: f32,
    size: (f32, f32),
) -> (f32, f32, f32, f32) {
    let (vx, vy) = (-pan.0 / scale, -pan.1 / scale);
    let (vw, vh) = (view.0 / scale, view.1 / scale);
    let x0 = (vx - vw * MARGIN).max(0.);
    let y0 = (vy - vh * MARGIN).max(0.);
    let x1 = (vx + vw * (1. + MARGIN)).min(size.0);
    let y1 = (vy + vh * (1. + MARGIN)).min(size.1);
    (x0, y0, (x1 - x0).max(0.), (y1 - y0).max(0.))
}

/// 그림 좌표의 점 p 를 품은 노드 중 가장 작은 것 — 안쪽 상자를 누르면 그 상자다
pub fn hit_node(boxes: &[Option<(f32, f32, f32, f32)>], p: (f32, f32)) -> Option<usize> {
    boxes
        .iter()
        .enumerate()
        .filter_map(|(i, b)| b.map(|b| (i, b)))
        .filter(|(_, (x, y, w, h))| p.0 >= *x && p.0 <= x + w && p.1 >= *y && p.1 <= y + h)
        .min_by(|a, b| (a.1.2 * a.1.3).total_cmp(&(b.1.2 * b.1.3)))
        .map(|(i, _)| i)
}

/// 노드 상자(그림 좌표) — graph.nodes 와 같은 자리. 고르기, 선택 고리, 찾기 상자가 쓴다
pub fn node_boxes(tree: &usvg::Tree, st: &Structure) -> Vec<Option<(f32, f32, f32, f32)>> {
    st.nodes
        .iter()
        .map(|n| content_box(tree.node_by_id(&n.dom_id)?))
        .collect()
}

/// 보이는 잎(도형, 글자, 그림)만 합친 상자. usvg 는 빈 그룹(글자 없는 ER 코멘트 칸)을 원점의 0 크기 상자로 셈해
/// 그룹 상자가 (0, 0) 까지 늘어난다
fn content_box(node: &usvg::Node) -> Option<(f32, f32, f32, f32)> {
    let leaf = |r: usvg::Rect| {
        (r.width() > 0. || r.height() > 0.).then(|| (r.left(), r.top(), r.right(), r.bottom()))
    };
    let mut acc: Option<(f32, f32, f32, f32)> = None;
    let mut add = |b: (f32, f32, f32, f32)| {
        acc = Some(match acc {
            None => b,
            Some(a) => (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)),
        });
    };
    match node {
        usvg::Node::Group(g) => {
            for c in g.children() {
                if let Some((x, y, w, h)) = content_box(c) {
                    add((x, y, x + w, y + h));
                }
            }
        }
        _ => {
            if let Some(b) = leaf(node.abs_bounding_box()) {
                add(b);
            }
        }
    }
    acc.map(|(l, t, r, b)| (l, t, r - l, b - t))
}

/// 포커스 모드(.dgm-focused) — 고른 노드와 직접 이어진 노드, 엣지, 라벨만 남기고 나머지를 0.1 로.
/// 연결을 못 읽었으면(엣지 0개) 아무것도 흐리게 하지 않는다 — 전부 죽어버리는 것보단 낫다
pub fn focus_css(st: &Structure, key: &str) -> Option<String> {
    if st.edges.is_empty() {
        return None;
    }
    let pairs: Vec<(String, String)> = st
        .edges
        .iter()
        .map(|e| (e.source.clone(), e.target.clone()))
        .collect();
    let mut keep = neighbors_of(key, &pairs);
    keep.push(key.to_string());
    let mut dim: Vec<&str> = st
        .nodes
        .iter()
        .filter(|n| !keep.contains(&n.key))
        .map(|n| n.dom_id.as_str())
        .collect();
    for e in &st.edges {
        if e.source != key && e.target != key {
            dim.push(&e.dom_id);
            dim.extend(e.label_id.as_deref());
        }
    }
    if dim.is_empty() {
        return None;
    }
    let sel: Vec<String> = dim.iter().map(|id| css_id(id)).collect();
    Some(format!("{} {{ opacity: 0.1 !important; }}", sel.join(", ")))
}

/// 찾기 — 렌더된 라벨 글자를 대소문자 없이 뒤진다(원문이 아니라 화면에 보이는 문자열이라야
/// "분명 있는데 안 잡힌다"가 없다). 라벨 자리 목록
pub fn search_hits(labels: &[GLabel], q: &str) -> Vec<usize> {
    let term = q.trim().to_lowercase();
    if term.is_empty() {
        return Vec::new();
    }
    labels
        .iter()
        .enumerate()
        .filter(|(_, l)| l.text.to_lowercase().contains(&term))
        .map(|(i, _)| i)
        .collect()
}

const HIT: &str = "#b3460f";

/// 걸린 라벨 글자를 주황으로, 지금 것은 밑줄까지(.dgm-hit, .dgm-hit-cur). 글자 폭을 바꾸는 속성은 쓰지 않는다 —
/// 라벨 상자 폭이 렌더 때 글자 폭으로 굳어 있어 굵히면 뒷글자가 잘린다
pub fn hit_css(labels: &[GLabel], hits: &[usize], cur: usize) -> Option<String> {
    let sels = |i: usize| {
        let l = &labels[i];
        let mut v = vec![css_id(&l.dom_id), format!("{} *", css_id(&l.dom_id))];
        v.extend(l.refs.iter().map(|r| format!("{} *", css_id(r))));
        v
    };
    if hits.is_empty() {
        return None;
    }
    let all: Vec<String> = hits.iter().flat_map(|&i| sels(i)).collect();
    let mut css = format!(
        "{} {{ fill: {HIT} !important; color: {HIT} !important; }}",
        all.join(", ")
    );
    if let Some(&i) = hits.get(cur) {
        css += &format!(
            " {} {{ text-decoration: underline !important; }}",
            sels(i).join(", ")
        );
    }
    Some(css)
}

fn css_id(id: &str) -> String {
    format!("[id=\"{}\"]", id.replace('\\', "\\\\").replace('"', "\\\""))
}

/// 찾기 바(.dgm-search)
pub struct DgmSearch {
    pub input: Entity<InputState>,
    pub query: String,
    /// graph.labels 의 자리
    pub hits: Vec<usize>,
    pub idx: usize,
}

/// 고른 노드 — 지금 앱의 NodeSel. idx 는 graph.nodes 의 자리
pub struct NodeSel {
    pub idx: usize,
    pub text: String,
    pub id: String,
    /// 원문에서 처음 나오는 줄(1부터, 없으면 -1)
    pub line: i64,
    pub columns: Vec<Column>,
}

/// 끌어 옮기기 — GPUI 의 끌기 값
#[derive(Clone)]
pub struct CanvasGrab;

pub struct CanvasDrag;

impl Render for CanvasDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

pub struct Canvas {
    /// 지금 그린(또는 그리려다 실패한) 원문
    pub chart: String,
    pub tree: Option<Arc<usvg::Tree>>,
    pub error: Option<String>,
    /// 100% 배율 — 그림을 받은 때 판에 맞춘 배율
    pub base: f32,
    /// base 에 대한 배율(1 = 100%)
    pub zoom: f32,
    /// 그림 왼쪽 위의 판 안 자리
    pub pan: Point<Pixels>,
    /// 판의 자리(창 좌표)와 화면 배율 — 그린 뒤 잰다
    pub viewport: Rc<Cell<Bounds<Pixels>>>,
    hitbox: Rc<std::cell::RefCell<Option<Hitbox>>>,
    pub dpr: Rc<Cell<f32>>,
    /// 다음 그림에서 맞춘다(새 그림, 전체 화면 전환)
    pub fit_pending: bool,
    /// 맞춘 때의 판 크기 — 판이 바뀌면 다시 맞춘다
    fitted_view: (f32, f32),
    /// 전체(낮은 해상도)와 보이는 곳(선명하게: 구간, 배율)
    pub full: Option<Arc<Image>>,
    full_scale: f32,
    pub sharp: Option<((f32, f32, f32, f32), f32, Arc<Image>)>,
    raster_gen: u64,
    raster_task: Option<Task<()>>,
    /// 누른 자리와 그때의 그림 자리 — 끌기가 시작되면 grab 으로 옮긴다(누른 뒤 끌기 전에 다시 그려져도 남게)
    pub press: Option<(Point<Pixels>, Point<Pixels>)>,
    /// 끄는 중(창 밖에서 놓으면 render 가 지운다)
    pub grab: Option<(Point<Pixels>, Point<Pixels>)>,
    pub fullscreen: bool,
    /// 그림에서 읽은 노드, 엣지, 라벨(선택, 포커스, 찾기)과 노드 상자(그림 좌표, nodes 와 같은 자리)
    pub graph: Option<Arc<Structure>>,
    boxes: Vec<Option<(f32, f32, f32, f32)>>,
    pub sel: Option<NodeSel>,
    /// 컬럼 패널 — 노드를 바꿔 고르면 접힌다
    pub show_cols: bool,
    /// 이름 복사 피드백 — 1.5초 뒤 원래대로
    pub copied: bool,
    copy_task: Option<Task<()>>,
    /// 누른 자리 — 4px 넘게 끌었으면 클릭이 아니다
    down: Option<Point<Pixels>>,
    pub search: Option<DgmSearch>,
    /// 지금 트리가 css 를 얹어 읽은 것인지(포커스, 찾기)
    styled: bool,
    /// 트리가 바뀌어(포커스) 구운 그림이 낡았다 — 새로 구울 때까지 옛 그림을 그대로 둔다
    stale: bool,
}

impl Default for Canvas {
    fn default() -> Self {
        Canvas {
            chart: String::new(),
            tree: None,
            error: None,
            base: 0.,
            zoom: 1.,
            pan: Point::default(),
            viewport: Rc::default(),
            hitbox: Rc::default(),
            dpr: Rc::new(Cell::new(1.)),
            fit_pending: false,
            fitted_view: (0., 0.),
            full: None,
            full_scale: 0.,
            sharp: None,
            raster_gen: 0,
            raster_task: None,
            press: None,
            grab: None,
            fullscreen: false,
            graph: None,
            boxes: Vec::new(),
            sel: None,
            show_cols: false,
            copied: false,
            copy_task: None,
            down: None,
            search: None,
            styled: false,
            stale: false,
        }
    }
}

impl Canvas {
    fn size(&self) -> Option<(f32, f32)> {
        self.tree
            .as_ref()
            .map(|t| (t.size().width(), t.size().height()))
    }

    fn scale(&self) -> f32 {
        self.base * self.zoom
    }

    fn view(&self) -> (f32, f32) {
        let b = self.viewport.get();
        (f32::from(b.size.width), f32::from(b.size.height))
    }

    fn pan_f(&self) -> (f32, f32) {
        (f32::from(self.pan.x), f32::from(self.pan.y))
    }

    /// 그림을 바꾼다 — 같은 원문이면 아무것도 안 한다. 실패하면 마지막 그림을 두고 오류만
    pub fn set_chart(&mut self, chart: &str) {
        if self.chart == chart && (self.tree.is_some() || self.error.is_some()) {
            return;
        }
        self.chart = chart.to_string();
        let built = amber_gpui::mermaid::to_svg(chart).and_then(|svg| {
            let st = read_structure(&svg);
            crate::figure::parse(&st.svg, "#333333").map(|tree| (st, tree))
        });
        match built {
            Ok((st, tree)) => {
                self.boxes = node_boxes(&tree, &st);
                self.graph = Some(Arc::new(st));
                self.tree = Some(Arc::new(tree));
                self.error = None;
                self.full = None;
                self.sharp = None;
                self.fit_pending = true;
                // 그릴 때마다 선택은 처음으로(지금 앱도 렌더마다 판을 새로 만든다)
                self.sel = None;
                self.show_cols = false;
                self.copied = false;
                self.styled = false;
                self.stale = false;
                // 찾기가 열려 있으면 새 그림에서 다시 찾는다
                if let Some(f) = self.search.as_mut() {
                    f.hits = search_hits(&self.graph.as_ref().unwrap().labels, &f.query);
                    f.idx = 0;
                    self.restyle();
                }
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// 판 안의 점 p(판 좌표)를 누름 — 노드면 고르기(같은 노드면 해제), 빈 곳이면 해제
    pub fn click_at(&mut self, p: (f32, f32)) {
        let s = self.scale();
        if self.graph.is_none() || s <= 0. {
            return;
        }
        let (x, y) = self.pan_f();
        match hit_node(&self.boxes, ((p.0 - x) / s, (p.1 - y) / s)) {
            Some(i) if self.sel.as_ref().is_some_and(|c| c.idx == i) => self.deselect(),
            Some(i) => self.select(i),
            None => self.deselect(),
        }
    }

    pub fn select(&mut self, idx: usize) {
        let Some(st) = self.graph.clone() else {
            return;
        };
        let Some(n) = st.nodes.get(idx) else {
            return;
        };
        let id = extract_node_id(&n.key);
        let mut line = find_line_for_node_id(&id, &self.chart);
        if line < 1 && !n.text.is_empty() && n.text != id {
            line = find_line_for_node_id(&n.text, &self.chart);
        }
        self.sel = Some(NodeSel {
            idx,
            text: if n.text.is_empty() {
                t("diagrams.node.unnamed")
            } else {
                n.text.clone()
            },
            id,
            line,
            columns: n.columns.clone(),
        });
        // 다른 테이블의 컬럼이 펼쳐진 채로, '복사됨'이 남은 채로 두지 않는다
        self.show_cols = false;
        self.copied = false;
        self.copy_task = None;
        self.restyle();
    }

    pub fn deselect(&mut self) {
        let had = self.sel.take().is_some();
        self.show_cols = false;
        self.copied = false;
        self.copy_task = None;
        if had {
            self.restyle();
        }
    }

    /// 고른 노드(포커스)와 찾기 결과에 맞춰 칠한 트리로 바꾼다 — 레이아웃은 그대로라 그림이 흔들리지 않는다
    fn restyle(&mut self) {
        let Some(st) = self.graph.clone() else {
            return;
        };
        let css: String = [
            self.sel
                .as_ref()
                .and_then(|s| focus_css(&st, &st.nodes[s.idx].key)),
            self.search
                .as_ref()
                .and_then(|f| hit_css(&st.labels, &f.hits, f.idx)),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
        if css.is_empty() && !self.styled {
            return;
        }
        let styled = !css.is_empty();
        match crate::figure::parse_styled(&st.svg, "#333333", styled.then_some(css)) {
            Ok(tree) => {
                self.tree = Some(Arc::new(tree));
                self.styled = styled;
                self.stale = true;
            }
            Err(e) => eprintln!("다이어그램 칠하기: {e}"),
        }
    }

    pub fn close_search(&mut self) {
        if self.search.take().is_some() {
            self.restyle();
        }
    }

    /// 검색어가 바뀌었다 — 처음 결과로
    pub fn run_search(&mut self, q: &str) {
        let Some(st) = self.graph.clone() else {
            return;
        };
        let Some(f) = self.search.as_mut() else {
            return;
        };
        f.query = q.to_string();
        f.hits = search_hits(&st.labels, q);
        f.idx = 0;
        let any = !f.hits.is_empty();
        self.restyle();
        if any {
            self.center_hit();
        }
    }

    /// Enter / Shift+Enter, ‹ ›
    pub fn step_hit(&mut self, delta: i64) {
        let Some(f) = self.search.as_mut() else {
            return;
        };
        let n = f.hits.len() as i64;
        if n == 0 {
            return;
        }
        f.idx = (f.idx as i64 + delta).rem_euclid(n) as usize;
        self.restyle();
        self.center_hit();
    }

    /// 지금 결과를 판 가운데로 — 배율은 그대로
    fn center_hit(&mut self) {
        let (Some(st), Some(tree), Some(f)) = (&self.graph, &self.tree, &self.search) else {
            return;
        };
        let Some(l) = f.hits.get(f.idx).map(|&i| &st.labels[i]) else {
            return;
        };
        let Some(b) = tree
            .node_by_id(&l.dom_id)
            .and_then(content_box)
            .or_else(|| self.node_box(l.node.as_deref()))
        else {
            return;
        };
        let (v, s) = (self.view(), self.scale());
        self.pan = point(
            px(v.0 / 2. - (b.0 + b.2 / 2.) * s),
            px(v.1 / 2. - (b.1 + b.3 / 2.) * s),
        );
    }

    fn node_box(&self, key: Option<&str>) -> Option<(f32, f32, f32, f32)> {
        let i = self
            .graph
            .as_ref()?
            .nodes
            .iter()
            .position(|n| Some(n.key.as_str()) == key)?;
        *self.boxes.get(i)?
    }

    /// 걸린 라벨을 품은 노드 상자(판 좌표)와 지금 것인지 — 컬럼 하나가 걸려도 어느 테이블인지 보이게
    fn hit_boxes(&self) -> Vec<((f32, f32, f32, f32), bool)> {
        let (Some(st), Some(f)) = (&self.graph, &self.search) else {
            return Vec::new();
        };
        let (s, (px0, py0)) = (self.scale(), self.pan_f());
        let mut out: Vec<(Option<&str>, bool)> = Vec::new();
        for (j, &i) in f.hits.iter().enumerate() {
            let node = st.labels[i].node.as_deref();
            if node.is_none() {
                continue;
            }
            match out.iter_mut().find(|(n, _)| *n == node) {
                Some(e) => e.1 |= j == f.idx,
                None => out.push((node, j == f.idx)),
            }
        }
        out.into_iter()
            .filter_map(|(n, cur)| {
                let (x, y, w, h) = self.node_box(n)?;
                Some(((px0 + x * s, py0 + y * s, w * s, h * s), cur))
            })
            .collect()
    }

    /// 시험용 — 키가 key 인 노드 가운데의 창 좌표
    #[cfg(any(test, feature = "shots"))]
    pub fn node_center(&self, key: &str) -> Option<Point<Pixels>> {
        let i = self
            .graph
            .as_ref()?
            .nodes
            .iter()
            .position(|n| n.key == key)?;
        let (x, y, w, h) = (*self.boxes.get(i)?)?;
        let (s, (px0, py0), o) = (self.scale(), self.pan_f(), self.viewport.get().origin);
        Some(point(
            o.x + px(px0 + (x + w / 2.) * s),
            o.y + px(py0 + (y + h / 2.) * s),
        ))
    }

    /// 고른 노드의 상자(판 좌표 x, y, w, h)
    fn sel_rect(&self) -> Option<(f32, f32, f32, f32)> {
        let (x, y, w, h) = (*self.boxes.get(self.sel.as_ref()?.idx)?)?;
        let s = self.scale();
        let (px0, py0) = self.pan_f();
        Some((px0 + x * s, py0 + y * s, w * s, h * s))
    }

    /// 배율을 판 안의 점 p 를 붙잡고 바꾼다(최소 0.1, 최대 20)
    pub fn zoom_by(&mut self, factor: f32, p: (f32, f32)) {
        if self.tree.is_none() || self.base <= 0. {
            return;
        }
        let new = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let (x, y) = zoom_at(self.pan_f(), self.scale(), self.base * new, p);
        self.zoom = new;
        self.pan = point(px(x), px(y));
    }

    /// 판 가운데를 붙잡고
    pub fn zoom_center(&mut self, factor: f32) {
        let v = self.view();
        self.zoom_by(factor, (v.0 / 2., v.1 / 2.));
    }

    /// 맞춤 — 지금 판 크기로 다시 맞추고 그것을 100% 로
    pub fn fit_now(&mut self) {
        let (Some(size), v) = (self.size(), self.view()) else {
            return;
        };
        if v.0 <= 0. || v.1 <= 0. {
            self.fit_pending = true;
            return;
        }
        let (s, (x, y)) = fit(v, size);
        self.base = s;
        self.zoom = 1.;
        self.pan = point(px(x), px(y));
        self.fit_pending = false;
        self.fitted_view = v;
        // 전체 그림의 해상도가 맞춘 배율에 달려 있다
        self.full = None;
        self.sharp = None;
    }

    /// 100% — 처음 맞춘 배율로 되돌리고 가운데로
    pub fn reset_100(&mut self) {
        let (Some(size), v) = (self.size(), self.view()) else {
            return;
        };
        self.zoom = 1.;
        let s = self.scale();
        self.pan = point(px((v.0 - size.0 * s) / 2.), px((v.1 - size.1 * s) / 2.));
    }

    /// 손이 그림 위에 있다(.dgm-canvas-wrap 의 hover). 모달이 덮고 있으면 아니다.
    /// hover 이벤트는 쓰지 않는다 — GPUI 는 마지막 입력이 키보드면 is_hovered 를 false 로 돌려 키 하나 뒤에 꺼진다
    pub fn pointer_in(&self, window: &Window) -> bool {
        self.tree.is_some()
            && self
                .hitbox
                .borrow()
                .as_ref()
                .is_some_and(|h| h.is_hovered_at(window.mouse_position(), window))
    }

    pub fn pct(&self) -> u32 {
        (self.zoom * 100.).round().max(1.) as u32
    }
}

impl Spike {
    /// render 에서 — 판을 잰 뒤 맞추고(판이 바뀌었으면 다시), 굽기를 건다
    pub(crate) fn prepare_diagram_canvas(&mut self, cx: &mut Context<Self>) {
        let c = &mut self.diagrams.canvas;
        let v = c.view();
        if c.tree.is_some() && v.0 > 0. && (c.fit_pending || (c.fitted_view != v && c.zoom == 1.)) {
            c.fit_now();
        }
        self.schedule_raster(cx);
    }

    /// 전체 그림이 없으면 굽고, 확대가 전체 해상도를 넘으면 보이는 곳을 선명하게 — 120ms 멈춘 뒤에
    fn schedule_raster(&mut self, cx: &mut Context<Self>) {
        let c = &mut self.diagrams.canvas;
        let (Some(tree), Some(size)) = (c.tree.clone(), c.size()) else {
            return;
        };
        if c.base <= 0. || c.fit_pending {
            return;
        }
        let dpr = c.dpr.get().max(1.);
        let want = c.scale() * dpr;
        let need_full = c.full.is_none() || c.stale;
        // 전체 해상도로 충분하면 선명 층은 필요 없다
        let need_sharp = !need_full
            && want > c.full_scale * 1.05
            && match &c.sharp {
                None => true,
                Some((region, scale, _)) => {
                    let vis = visible_region(c.view(), c.pan_f(), c.scale(), size);
                    let inside = vis.0 >= region.0 - 0.5
                        && vis.1 >= region.1 - 0.5
                        && vis.0 + vis.2 <= region.0 + region.2 + 0.5
                        && vis.1 + vis.3 <= region.1 + region.3 + 0.5;
                    // 보이는 곳이 구운 곳을 벗어났거나 배율이 크게 달라졌다
                    (*scale - want).abs() / want > 0.2 || !inside
                }
            };
        if !need_full && !need_sharp {
            return;
        }
        c.raster_gen += 1;
        let generation = c.raster_gen;
        let full_scale = (c.base * dpr)
            .min(FULL_MAX_PX / size.0.max(size.1))
            .max(0.05);
        let region = visible_region(c.view(), c.pan_f(), c.scale(), size);
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(if need_full { 0 } else { 120 }))
                .await;
            let ex = cx.background_executor().clone();
            let out = ex
                .spawn(async move {
                    if need_full {
                        crate::figure::raster(&tree, (0., 0., size.0, size.1), full_scale)
                            .map(|i| (true, region, full_scale, i))
                    } else {
                        crate::figure::raster(&tree, region, want).map(|i| (false, region, want, i))
                    }
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                let c = &mut s.diagrams.canvas;
                if c.raster_gen != generation {
                    return;
                }
                match out {
                    Ok((true, _, scale, img)) => {
                        c.full = Some(img);
                        c.full_scale = scale;
                        // 낡은 선명 층이 새 그림을 덮지 않게
                        if c.stale {
                            c.stale = false;
                            c.sharp = None;
                        }
                    }
                    Ok((false, region, scale, img)) => c.sharp = Some((region, scale, img)),
                    Err(e) => eprintln!("다이어그램 굽기: {e}"),
                }
                cx.notify();
            });
        });
        self.diagrams.canvas.raster_task = Some(task);
    }

    /// 휠 — 커서 자리를 붙잡고 확대/축소(svg-pan-zoom 처럼 1.4 의 거듭제곱)
    fn canvas_wheel(&mut self, e: &ScrollEvent) {
        let dy = match e.delta {
            ScrollDelta::Pixels(p) => f32::from(p.y),
            ScrollDelta::Lines(l) => l.y * 30.,
        };
        let c = &mut self.diagrams.canvas;
        let o = c.viewport.get().origin;
        let p = (f32::from(e.position.x - o.x), f32::from(e.position.y - o.y));
        c.zoom_by((1. + SENS).powf(dy / 90.), p);
    }

    pub(crate) fn canvas_pan_to(&mut self, pos: Point<Pixels>) -> bool {
        let c = &mut self.diagrams.canvas;
        let Some((start, pan)) = c.grab else {
            return false;
        };
        c.pan = pan + (pos - start);
        true
    }

    pub(crate) fn canvas_drop(&mut self) {
        self.diagrams.canvas.grab = None;
    }

    /// 입력칸(원문, 찾기, 모달)에서 치는 중 — 캔버스 단축키는 글자를 먹으면 안 된다(웹은 INPUT/TEXTAREA 의 키를 건너뛴다).
    /// 뿌리나 트리 판이 아닌 곳에 포커스가 있으면 입력칸이다
    pub(crate) fn diagram_typing(&self, window: &Window, cx: &App) -> bool {
        window
            .focused(cx)
            .is_some_and(|f| f != self.focus && f != self.tree_focus && f != self.dtree_focus)
    }

    /// 찾기 바를 연다 — ⌘F 는 글자를 골라 둔 채(다시 치면 바로 바뀐다), '/' 와 단추는 커서만
    pub(crate) fn open_diagram_search(
        &mut self,
        select: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.diagrams.selected.is_none() {
            return;
        }
        if let Some(f) = &self.diagrams.canvas.search {
            f.input.update(cx, |s, cx| {
                if select {
                    s.select_all(window, cx);
                }
                s.focus(window, cx);
            });
            return;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(t("diagrams.search.ph")));
        cx.subscribe_in(
            &input,
            window,
            |this, input, ev: &InputEvent, _, cx| match ev {
                InputEvent::Change => {
                    let q = input.read(cx).value().to_string();
                    // Enter 뒤에도 값이 그대로인 Change 가 온다 — 검색어가 바뀔 때만 처음으로
                    let c = &mut this.diagrams.canvas;
                    if c.search.as_ref().is_some_and(|f| f.query == q) {
                        return;
                    }
                    c.run_search(&q);
                    cx.notify();
                }
                InputEvent::PressEnter { shift, .. } => {
                    this.diagrams.canvas.step_hit(if *shift { -1 } else { 1 });
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();
        input.update(cx, |s, cx| s.focus(window, cx));
        self.diagrams.canvas.search = Some(DgmSearch {
            input,
            query: String::new(),
            hits: Vec::new(),
            idx: 0,
        });
    }

    /// 찾기 바(.dgm-search) — 줌 단추 바로 아래. 결과 개수와 이동이 한 줄에 있어야 훑는 흐름이 안 끊긴다.
    /// 흰 판 위라 테마 토큰이 아니라 밝은 색을 고정으로 쓴다
    fn search_bar(&self, f: &DgmSearch, cx: &mut Context<Self>) -> Div {
        let ink = rgb(0x52525b);
        let count = f.hits.len();
        let nav = |id: &'static str, name: &str, enabled: bool| {
            div()
                .id(id)
                .tag(id)
                .flex()
                .p(px(3.))
                .rounded(px(5.))
                .when(enabled, |d| {
                    d.cursor_pointer().hover(|st| st.bg(rgba(0x18181b12)))
                })
                .when(!enabled, |d| d.opacity(0.4))
                .child(icon(name, 14.).text_color(ink))
        };
        let label = if count > 0 {
            format!("{}/{count}", f.idx + 1)
        } else if !f.query.trim().is_empty() {
            "0".into()
        } else {
            String::new()
        };
        div()
            .absolute()
            .top(px(52.))
            .right(px(10.))
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(8.))
            .py(px(5.))
            .rounded(px(9.))
            .border_1()
            .border_color(rgba(0x18181b1a))
            .bg(rgba(0xffffffeb))
            .shadow(vec![BoxShadow {
                color: rgba(0x18181b1a).into(),
                offset: point(px(0.), px(1.)),
                blur_radius: px(3.),
                spread_radius: px(0.),
                inset: false,
            }])
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "escape" {
                    this.diagrams.canvas.close_search();
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .child(icon("search", 14.).text_color(ink))
            .child(
                div()
                    .tag("dgm-search-input")
                    .w(px(190.))
                    .text_size(px(13.))
                    .text_color(rgb(0x18181b))
                    .child(Input::new(&f.input).appearance(false).px_0()),
            )
            .child(
                div()
                    .id("dgm-search-count")
                    .tag("dgm-search-count")
                    .min_w(px(34.))
                    .flex()
                    .justify_end()
                    .text_size(px(11.5))
                    .text_color(rgb(0x71717a))
                    .child(label),
            )
            .child(
                nav("dgm-search-prev", "chevron-left", count > 0).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.diagrams.canvas.step_hit(-1);
                        cx.notify();
                    },
                )),
            )
            .child(
                nav("dgm-search-next", "chevron-right", count > 0).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.diagrams.canvas.step_hit(1);
                        cx.notify();
                    },
                )),
            )
            .child(
                nav("dgm-search-close", "x", true).on_click(cx.listener(|this, _, _, cx| {
                    this.diagrams.canvas.close_search();
                    cx.notify();
                })),
            )
    }

    /// 캔버스에 손이 올라가 있을 때의 단축키(+ - 1 0), Esc 는 전체 화면부터 닫는다. 받았으면 true
    pub(crate) fn canvas_key(&mut self, key: &str, window: &Window) -> bool {
        let c = &mut self.diagrams.canvas;
        if key == "escape" && c.fullscreen {
            c.fullscreen = false;
            c.fit_pending = true;
            return true;
        }
        if !c.pointer_in(window) {
            return false;
        }
        match key {
            "+" | "=" => c.zoom_center(1. + SENS),
            "-" => c.zoom_center(1. / (1. + SENS)),
            "1" => c.reset_100(),
            "0" => c.fit_now(),
            "escape" if c.sel.is_some() => c.deselect(),
            _ => return false,
        }
        true
    }

    /// 캔버스(.dgm-canvas-wrap) — 그림 층 둘, 오른쪽 위 화면 조작, 왼쪽 아래 배치 엔진, 아래 오류 줄
    pub(crate) fn diagram_canvas(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let c = &self.diagrams.canvas;
        let (viewport, dpr, hitbox) = (c.viewport.clone(), c.dpr.clone(), c.hitbox.clone());
        let scale = c.scale();
        let layer = |img_: Arc<Image>, x: f32, y: f32, w: f32, h: f32| {
            img(img_)
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(w))
                .h(px(h))
        };
        let mut stage = div()
            .id("dgm-canvas")
            .tag("dgm-canvas")
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .cursor(if c.grab.is_some() {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            })
            .child(
                canvas(
                    move |b, window, _| {
                        let changed = viewport.get() != b;
                        viewport.set(b);
                        dpr.set(window.scale_factor());
                        *hitbox.borrow_mut() =
                            Some(window.insert_hitbox(b, HitboxBehavior::Normal));
                        // 처음 잰 판이면 다음 그림에서 맞춘다
                        if changed {
                            window.request_animation_frame();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            );
        if let (Some(tree), false) = (&c.tree, c.fit_pending) {
            let (w, h) = (tree.size().width(), tree.size().height());
            let (px0, py0) = (f32::from(c.pan.x), f32::from(c.pan.y));
            if let Some(full) = &c.full {
                stage = stage.child(layer(full.clone(), px0, py0, w * scale, h * scale));
            }
            if let Some(((rx, ry, rw, rh), _, sharp)) = &c.sharp {
                stage = stage.child(layer(
                    sharp.clone(),
                    px0 + rx * scale,
                    py0 + ry * scale,
                    rw * scale,
                    rh * scale,
                ));
            }
            // 찾기 결과가 든 노드 상자(.dgm-hit-box) — 지금 것은 한 단계 굵게
            for ((x, y, w, h), cur) in c.hit_boxes() {
                let width = if cur { 3.5 } else { 2. };
                stage = stage.child(
                    div()
                        .tag(if cur {
                            "dgm-hit-box-cur"
                        } else {
                            "dgm-hit-box"
                        })
                        .absolute()
                        .left(px(x - width / 2.))
                        .top(px(y - width / 2.))
                        .w(px(w + width))
                        .h(px(h + width))
                        .border(px(width))
                        .border_color(rgb(0xb3460f)),
                );
            }
            // 선택 고리(.node-selected) — 캔버스는 늘 밝아 파란 강조 고정. 획 2.5px 이 도형 테두리에 걸치게.
            // 번짐(drop-shadow)은 옅은 테 한 겹으로 — GPUI 의 그림자는 상자 안까지 칠해 노드를 덮는다
            if let Some((x, y, w, h)) = c.sel_rect() {
                let ring = |grow: f32, width: f32, color: Rgba| {
                    div()
                        .absolute()
                        .left(px(x - grow))
                        .top(px(y - grow))
                        .w(px(w + grow * 2.))
                        .h(px(h + grow * 2.))
                        .rounded(px(2. + grow))
                        .border(px(width))
                        .border_color(color)
                };
                stage = stage
                    .child(ring(4.25, 3., rgba(0x2563eb40)))
                    .child(ring(1.25, 2.5, rgba(0x2563ebff)).tag("dgm-sel-ring"));
            }
        } else if c.error.is_none() {
            stage = stage.child(
                div()
                    .p(px(30.))
                    .flex()
                    .justify_center()
                    .text_size(px(13.))
                    .text_color(k.text_3)
                    .child(t("diagrams.canvas.rendering")),
            );
        }
        let stage = stage
            .on_scroll(cx.listener(|this, e: &ScrollEvent, _, cx| {
                this.canvas_wheel(e);
                cx.stop_propagation();
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    // 판은 포커스를 받지 않는 곳 — 누르면 트리, 원문 칸에서 빠진다(브라우저처럼). 그래야 ⌘F 가 캔버스 찾기다
                    this.focus.focus(window, cx);
                    let c = &mut this.diagrams.canvas;
                    c.press = Some((e.position, c.pan));
                    c.down = Some(e.position);
                }),
            )
            .on_drag(CanvasGrab, {
                let this = cx.entity().downgrade();
                move |_, _, _, cx: &mut App| {
                    let _ = this.update(cx, |s, _| {
                        let c = &mut s.diagrams.canvas;
                        c.grab = c.press.take();
                    });
                    cx.new(|_| CanvasDrag)
                }
            })
            .on_click(cx.listener(|this, e: &ClickEvent, _, cx| {
                let c = &mut this.diagrams.canvas;
                let o = c.viewport.get().origin;
                let p = e.position();
                let at = (f32::from(p.x - o.x), f32::from(p.y - o.y));
                // 클릭 = 노드 고르기 토글. 끌어 옮긴 뒤는 아니다(4px 가드).
                // 더블클릭의 두 번째 누름도 클릭이다 — 지금 앱처럼 고르고 바로 풀린다
                let moved = c.down.take().is_some_and(|d| {
                    f32::from((p.x - d.x).abs()) + f32::from((p.y - d.y).abs()) > 4.
                });
                if !moved {
                    c.click_at(at);
                }
                // 더블클릭 = 커서 자리에서 확대(Shift = 축소)
                if e.click_count() == 2 {
                    let f = 1. + SENS * 2.;
                    let f = if e.modifiers().shift { 1. / f } else { f };
                    c.zoom_by(f, at);
                }
                cx.notify();
            }));

        // 흰 판 위라 테마 토큰이 아니라 밝은 색을 고정으로 쓴다(.dgm-float)
        let ink = rgb(0x52525b);
        let float_btn = |id: &'static str, active: bool| {
            div()
                .id(id)
                .tag(id)
                .min_w(px(28.))
                .h(px(28.))
                .px(px(6.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                // 고른 탭은 채움(§3) — hover 는 한 요소에 한 번만 걸 수 있다
                .when(active, |d| {
                    d.bg(rgb(0x18181b))
                        .text_color(rgb(0xffffff))
                        .hover(|s| s.bg(rgb(0x333333)))
                })
                .when(!active, |d| {
                    d.text_color(ink)
                        .hover(|s| s.bg(rgba(0x18181b0f)).text_color(rgb(0x18181b)))
                })
        };
        let float = || {
            div()
                .absolute()
                .flex()
                .items_center()
                .gap(px(2.))
                .p(px(3.))
                .rounded(px(9.))
                .border_1()
                .border_color(rgba(0x18181b1a))
                .bg(rgba(0xffffffdb))
                .shadow(vec![BoxShadow {
                    color: rgba(0x18181b1a).into(),
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(3.),
                    spread_radius: px(0.),
                    inset: false,
                }])
                // 판 위 단추를 누른 것은 끌기가 아니다
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        let full_icon = if c.fullscreen { "x" } else { "expand" };
        let tools = float()
            .top(px(10.))
            .right(px(10.))
            .child(
                float_btn("dgm-search-btn", false)
                    .child(icon("search", 16.).text_color(ink))
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.diagrams.canvas.search.is_some() {
                            this.diagrams.canvas.close_search();
                        } else {
                            this.open_diagram_search(false, window, cx);
                        }
                        cx.notify();
                    })),
            )
            .child(
                float_btn("dgm-fit", false)
                    .child(icon("fit-screen", 16.).text_color(ink))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.diagrams.canvas.fit_now();
                        cx.notify();
                    })),
            )
            .child(
                float_btn("dgm-zoom-out", false)
                    .child(icon("zoom-out", 16.).text_color(ink))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.diagrams.canvas.zoom_center(1. / (1. + SENS));
                        cx.notify();
                    })),
            )
            .child(
                float_btn("dgm-pct", false)
                    .min_w(px(46.))
                    .child(format!("{}%", c.pct()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.diagrams.canvas.reset_100();
                        cx.notify();
                    })),
            )
            .child(
                float_btn("dgm-zoom-in", false)
                    .child(icon("zoom-in", 16.).text_color(ink))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.diagrams.canvas.zoom_center(1. + SENS);
                        cx.notify();
                    })),
            )
            .child(
                float_btn("dgm-fullscreen", false)
                    .child(icon(full_icon, 16.).text_color(ink))
                    .on_click(cx.listener(|this, _, _, cx| {
                        let c = &mut this.diagrams.canvas;
                        c.fullscreen = !c.fullscreen;
                        c.fit_pending = true;
                        cx.notify();
                    })),
            );
        // 배치 엔진 — 고른 쪽만 채움. 앱 전체 설정이라 노트 안 그림도 이 엔진으로 그린다
        let cur = amber_gpui::mermaid::layout();
        let layouts = float()
            .bottom(px(10.))
            .left(px(10.))
            .children(["elk", "dagre"].map(|l| {
                let on = cur == l;
                float_btn(
                    if l == "elk" {
                        "dgm-layout-elk"
                    } else {
                        "dgm-layout-dagre"
                    },
                    on,
                )
                .px(px(10.))
                .whitespace_nowrap()
                .child(t(&format!("diagrams.layout.{l}")))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_diagram_layout(l);
                    cx.notify();
                }))
            }));
        let card = c.sel.as_ref().map(|s| self.node_card(s, k, cx));
        let search = c.search.as_ref().map(|f| self.search_bar(f, cx));
        let cols = c
            .sel
            .as_ref()
            .filter(|s| c.show_cols && !s.columns.is_empty())
            .map(|s| self.cols_panel(s, k, cx));
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(0xffffff))
            .child(stage)
            .child(tools)
            .child(layouts)
            .children(search)
            .children(card)
            .children(cols)
            .when_some(c.error.clone(), |d, e| {
                d.child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .px(px(12.))
                        .py(px(8.))
                        .border_t_1()
                        .border_color(k.danger)
                        .bg(k.danger_soft)
                        .child(
                            div()
                                .id("dgm-error")
                                .tag("dgm-error")
                                .flex_1()
                                .min_w_0()
                                .max_h(px(76.))
                                .overflow_y_scroll()
                                .font_family(crate::markdown::MONO)
                                .text_size(px(11.5))
                                .text_color(k.danger)
                                .child(e.clone()),
                        )
                        .child(
                            crate::day_card::button("dgm-error-copy", k, false, false)
                                .child(t("diagrams.copy"))
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(e.clone()));
                                })),
                        ),
                )
            })
    }

    /// 노드 정보 카드(.dgm-node-info) — 캔버스 아래 가운데. 이름(누르면 복사), 컬럼 단추, id, 라인 단추, 해제
    fn node_card(&self, s: &NodeSel, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let copied = self.diagrams.canvas.copied;
        let text = s.text.clone();
        let name =
            div()
                .id("dgm-node-name")
                .tag("dgm-node-name")
                .group("dgm-node-name")
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(8.))
                .mx(px(-6.))
                .my(px(-4.))
                .px(px(6.))
                .py(px(4.))
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|st| st.bg(k.surface_2))
                .child(
                    div()
                        .min_w_0()
                        .text_size(px(13.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(k.text)
                        .child(s.text.clone()),
                )
                // '복사' → '복사됨' 으로 글자가 늘어도 카드가 흔들리지 않게 자리를 미리 잡아 둔다
                .child(
                    div()
                        .flex_none()
                        .min_w(px(54.))
                        .flex()
                        .items_center()
                        .gap(px(3.))
                        .text_size(px(11.))
                        .text_color(if copied { k.text } else { k.text_3 })
                        .when(!copied, |d| {
                            d.opacity(0.6)
                                .group_hover("dgm-node-name", |st| st.opacity(1.))
                        })
                        .child(
                            icon(if copied { "check" } else { "copy" }, 12.)
                                .text_color(if copied { k.text } else { k.text_3 }),
                        )
                        .child(if copied {
                            t("diagrams.node.copied")
                        } else {
                            t("diagrams.copy")
                        }),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                    let task = cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(1500))
                            .await;
                        let _ = this.update(cx, |s, cx| {
                            s.diagrams.canvas.copied = false;
                            cx.notify();
                        });
                    });
                    let c = &mut this.diagrams.canvas;
                    c.copied = true;
                    c.copy_task = Some(task);
                    cx.notify();
                }));
        let n = s.columns.len().to_string();
        let open = self.diagrams.canvas.show_cols;
        let top =
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(name)
                .when(!s.columns.is_empty(), |d| {
                    d.child(
                        crate::day_card::button("dgm-cols-toggle", k, false, false)
                            .flex_none()
                            .gap(px(5.))
                            // 펼친 동안은 눌린 상태로 — 색이 아니라 채움으로(§3)
                            .when(open, |b| b.bg(k.surface_3).border_color(k.border_strong))
                            .child(icon("layers", 13.).text_color(k.text_2))
                            .child(t_with("diagrams.node.columns", &[("n", &n)]))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let c = &mut this.diagrams.canvas;
                                c.show_cols = !c.show_cols;
                                cx.notify();
                            })),
                    )
                });
        let line = s.line;
        let meta = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .when(!s.id.is_empty() && s.id != s.text, |d| {
                d.child(
                    div()
                        .tag("dgm-node-id")
                        .font_family(crate::markdown::MONO)
                        .text_size(px(11.))
                        .text_color(k.text_3)
                        .child(s.id.clone()),
                )
            })
            .when(line > 0, |d| {
                d.child(
                    crate::day_card::button("dgm-node-line", k, false, false)
                        .child(t_with("diagrams.node.line", &[("n", &line.to_string())]))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.jump_to_diagram_line(line, window, cx);
                            cx.notify();
                        })),
                )
            });
        let close = div()
            .id("dgm-node-deselect")
            .tag("dgm-node-deselect")
            .group("dgm-node-deselect")
            .flex_none()
            .p(px(5.))
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|st| st.bg(k.surface_2))
            .child(
                icon("x", 14.)
                    .text_color(k.text_3)
                    .group_hover("dgm-node-deselect", |st| st.text_color(k.text)),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.diagrams.canvas.deselect();
                cx.notify();
            }));
        // 판을 가로지르는 줄은 누름을 받지 않는다 — 카드만 받는다
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(px(14.))
            .flex()
            .justify_center()
            .px(px(14.))
            .child(
                div()
                    .id("dgm-node-info")
                    .tag("dgm-node-info")
                    .flex()
                    .items_start()
                    .gap(px(6.))
                    .max_w(px(520.))
                    .min_w_0()
                    .pt(px(10.))
                    .pr(px(10.))
                    .pb(px(10.))
                    .pl(px(14.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(k.border_strong)
                    .bg(k.surface)
                    .shadow(k.shadow())
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(top)
                            .child(meta),
                    )
                    .child(close),
            )
    }

    /// 컬럼 패널(.dgm-cols) — 오른쪽 위, 줌 단추 아래에서 시작하고 아래 카드와 겹치지 않게 잘린다
    fn cols_panel(&self, s: &NodeSel, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let n = s.columns.len().to_string();
        let last = s.columns.len().saturating_sub(1);
        let mono = crate::markdown::MONO;
        let rows = s.columns.iter().enumerate().map(|(i, c)| {
            div()
                .flex()
                .items_start()
                .gap(px(4.))
                .py(px(4.))
                .text_size(px(11.5))
                .line_height(px(17.25))
                .when(i != last, |d| d.border_b_1().border_color(k.border))
                // 이름이 먼저 — 찾을 때 눈이 훑는 건 타입이 아니라 이름이다
                .child(
                    div()
                        .w(px(200.))
                        .flex_none()
                        .font_family(mono)
                        .font_weight(FontWeight::BOLD)
                        .text_color(k.text)
                        .child(c.name.clone()),
                )
                .child(
                    div()
                        .w(px(82.))
                        .flex_none()
                        .font_family(mono)
                        .text_size(px(11.))
                        .text_color(k.text_3)
                        .child(c.ty.clone()),
                )
                // 키가 없는 줄에서도 자리를 지켜야 널 표기가 한 줄로 선다
                .child(div().w(px(30.)).flex_none().when(!c.keys.is_empty(), |d| {
                    d.child(
                        div()
                            .px(px(4.))
                            .rounded(px(4.))
                            .border_1()
                            .border_color(k.border_strong)
                            .text_color(k.text_2)
                            .text_size(px(9.5))
                            .font_weight(FontWeight::BOLD)
                            .whitespace_nowrap()
                            .child(c.keys.clone()),
                    )
                }))
                .when(!c.flag.is_empty(), |d| {
                    d.child(
                        div()
                            .font_family(mono)
                            .text_size(px(11.))
                            .text_color(k.text_3)
                            .whitespace_nowrap()
                            .child(c.flag.clone()),
                    )
                })
        });
        div()
            .absolute()
            .top(px(56.))
            .bottom(px(94.))
            .right(px(10.))
            .w(px(440.))
            .flex()
            .flex_col()
            .child(
                div()
                    .id("dgm-cols")
                    .tag("dgm-cols")
                    .max_h_full()
                    .flex()
                    .flex_col()
                    .rounded(px(10.))
                    .border_1()
                    .border_color(k.border_strong)
                    .bg(k.surface)
                    .shadow(k.shadow())
                    .overflow_hidden()
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_scroll(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_start()
                            .gap(px(6.))
                            .pt(px(9.))
                            .pr(px(8.))
                            .pb(px(9.))
                            .pl(px(13.))
                            .border_b_1()
                            .border_color(k.border)
                            .bg(k.surface_2)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(1.))
                                    .child(
                                        div()
                                            .font_family(mono)
                                            .text_size(px(12.5))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(k.text)
                                            .child(s.text.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.5))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(k.text_3)
                                            .child(
                                                t_with("diagrams.node.columns", &[("n", &n)])
                                                    .to_uppercase(),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .id("dgm-cols-close")
                                    .tag("dgm-cols-close")
                                    .group("dgm-cols-close")
                                    .flex_none()
                                    .p(px(5.))
                                    .rounded(px(6.))
                                    .cursor_pointer()
                                    .hover(|st| st.bg(k.surface_3))
                                    .child(
                                        icon("x", 14.)
                                            .text_color(k.text_3)
                                            .group_hover("dgm-cols-close", |st| {
                                                st.text_color(k.text)
                                            }),
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.diagrams.canvas.show_cols = false;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .id("dgm-cols-body")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .pt(px(6.))
                            .px(px(13.))
                            .pb(px(10.))
                            .children(rows),
                    ),
            )
    }

    /// 배치 엔진을 바꾼다 — 설정에 남기고 지금 그림을 다시 그린다(lib/diagramLayout.ts)
    pub(crate) fn set_diagram_layout(&mut self, name: &str) {
        amber_gpui::mermaid::set_layout(name);
        amber_gpui::prefs::set(&self.db, "amber.diagram.layout", name);
        let c = &mut self.diagrams.canvas;
        let chart = std::mem::take(&mut c.chart);
        c.tree = None;
        c.error = None;
        c.set_chart(&chart);
    }
}

#[cfg(test)]
mod tests {
    use super::{fit, focus_css, hit_css, hit_node, search_hits, visible_region, zoom_at};
    use amber_gpui::diagram_graph::read_structure;
    use resvg::usvg;

    #[test]
    fn hit_picks_the_smallest_box_under_the_point() {
        let boxes = [
            Some((0., 0., 100., 100.)),
            Some((10., 10., 20., 20.)),
            None,
            Some((200., 0., 10., 10.)),
        ];
        assert_eq!(hit_node(&boxes, (15., 15.)), Some(1), "안쪽 상자");
        assert_eq!(hit_node(&boxes, (50., 50.)), Some(0));
        assert_eq!(hit_node(&boxes, (150., 50.)), None, "빈 곳");
    }

    fn opacity(tree: &usvg::Tree, id: &str) -> f32 {
        match tree.node_by_id(id) {
            Some(usvg::Node::Group(g)) => g.opacity().get(),
            Some(_) => 1.,
            None => panic!("{id} 가 트리에 없다"),
        }
    }

    #[test]
    fn focus_dims_only_what_is_not_connected() {
        let svg = amber_gpui::mermaid::to_svg(
            "erDiagram\n  USER ||--o{ ORDER : places\n  ORDER ||--|{ ITEM : contains\n  PRODUCT ||--o{ ITEM : listed\n",
        )
        .unwrap();
        let st = read_structure(&svg);
        let key = |name: &str| {
            st.nodes
                .iter()
                .find(|n| n.text == name)
                .unwrap_or_else(|| panic!("{name}"))
                .clone()
        };
        let user = key("USER");
        let css = focus_css(&st, &user.key).expect("ER 은 연결을 읽는다");
        // 엣지와 라벨 — USER 에 닿은 것만 남는다(흐린 path 는 usvg 가 이름 없는 그룹으로 감싸 트리에선 못 찾는다)
        assert_eq!(st.edges.len(), 3);
        for e in &st.edges {
            let touches = e.source == user.key || e.target == user.key;
            let label = e.label_id.as_deref().expect("라벨과 짝이 맞는다");
            for id in [e.dom_id.as_str(), label] {
                assert_eq!(
                    css.contains(&format!("[id=\"{id}\"]")),
                    !touches,
                    "{id}: {css}"
                );
            }
        }
        let tree = crate::figure::parse_styled(&st.svg, "#333333", Some(css)).unwrap();
        assert_eq!(opacity(&tree, &user.dom_id), 1., "고른 노드");
        assert_eq!(opacity(&tree, &key("ORDER").dom_id), 1., "이웃");
        assert!(opacity(&tree, &key("ITEM").dom_id) < 0.2, "이웃 아님");
        assert!(opacity(&tree, &key("PRODUCT").dom_id) < 0.2);
    }

    /// 그룹 아래 글자들의 (채움 색, 밑줄 여부)
    fn spans(g: &usvg::Group, out: &mut Vec<(Option<(u8, u8, u8)>, bool)>) {
        for n in g.children() {
            match n {
                usvg::Node::Group(g) => spans(g, out),
                usvg::Node::Text(t) => {
                    for c in t.chunks() {
                        for sp in c.spans() {
                            let fill = sp.fill().and_then(|f| match f.paint() {
                                usvg::Paint::Color(c) => Some((c.red, c.green, c.blue)),
                                _ => None,
                            });
                            out.push((fill, sp.decoration().underline().is_some()));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn label_spans(tree: &usvg::Tree, id: &str) -> Vec<(Option<(u8, u8, u8)>, bool)> {
        let mut out = Vec::new();
        if let Some(usvg::Node::Group(g)) = tree.node_by_id(id) {
            spans(g, &mut out);
        }
        out
    }

    #[test]
    fn search_paints_hits_through_the_use_reference() {
        let svg = amber_gpui::mermaid::to_svg(
            "erDiagram\n  USER ||--o{ ORDER : places\n  USER {\n    bigint id PK\n  }\n",
        )
        .unwrap();
        let st = read_structure(&svg);
        let hits = search_hits(&st.labels, "  uSe ");
        let texts: Vec<&str> = hits.iter().map(|&i| st.labels[i].text.as_str()).collect();
        assert_eq!(texts, ["USER"], "대소문자, 앞뒤 공백 무시");
        assert!(search_hits(&st.labels, " ").is_empty());
        let both = search_hits(&st.labels, "er");
        assert_eq!(both.len(), 2, "USER, ORDER");
        let css = hit_css(&st.labels, &both, 1).unwrap();
        let tree = crate::figure::parse_styled(&st.svg, "#333333", Some(css)).unwrap();
        let orange = Some((0xb3, 0x46, 0x0f));
        let (user, order) = (&st.labels[both[0]], &st.labels[both[1]]);
        assert!(
            !user.refs.is_empty(),
            "글자는 defs 에 있고 <use> 로 가리킨다"
        );
        let u = label_spans(&tree, &user.dom_id);
        let o = label_spans(&tree, &order.dom_id);
        assert!(!u.is_empty() && !o.is_empty(), "{u:?} {o:?}");
        assert!(u.iter().all(|s| *s == (orange, false)), "걸린 것: {u:?}");
        assert!(
            o.iter().all(|s| *s == (orange, true)),
            "지금 것은 밑줄: {o:?}"
        );
        // 안 걸린 라벨은 그대로
        let id = st.labels.iter().find(|l| l.text == "id").unwrap();
        assert!(label_spans(&tree, &id.dom_id).iter().all(|s| s.0 != orange));
    }

    fn dump(g: &usvg::Group, depth: usize, out: &mut String) {
        for n in g.children() {
            let r = n.abs_bounding_box();
            out.push_str(&format!(
                "{}{} {:?} y={:.0}..{:.0}\n",
                "  ".repeat(depth),
                n.id(),
                std::mem::discriminant(n),
                r.y(),
                r.y() + r.height()
            ));
            if let usvg::Node::Group(c) = n {
                dump(c, depth + 1, out);
            }
        }
    }

    #[test]
    fn er_node_boxes_hug_their_entities() {
        let svg = amber_gpui::mermaid::to_svg(
            "erDiagram\n  USER ||--o{ ORDER : places\n  ORDER ||--|{ ORDER_ITEM : contains\n  USER {\n    bigint id PK\n  }\n  ORDER {\n    bigint id PK\n  }\n",
        )
        .unwrap();
        let st = read_structure(&svg);
        let tree = crate::figure::parse(&st.svg, "#333333").unwrap();
        let boxes = super::node_boxes(&tree, &st);
        for (i, a) in boxes.iter().enumerate() {
            for (j, b) in boxes.iter().enumerate().skip(i + 1) {
                let (a, b) = (a.unwrap(), b.unwrap());
                let overlap = a.1 < b.1 + b.3 - 0.5 && b.1 < a.1 + a.3 - 0.5;
                if overlap && a.0 < b.0 + b.2 && b.0 < a.0 + a.2 {
                    let mut out = String::new();
                    for k in [i, j] {
                        if let Some(usvg::Node::Group(g)) = tree.node_by_id(&st.nodes[k].dom_id) {
                            out.push_str(&format!("== {}\n", st.nodes[k].text));
                            dump(g, 1, &mut out);
                        }
                    }
                    panic!(
                        "{} {a:?} 와 {} {b:?} 가 겹친다\n{out}",
                        st.nodes[i].text, st.nodes[j].text
                    );
                }
            }
        }
    }

    #[test]
    fn focus_is_off_without_edges() {
        let svg = amber_gpui::mermaid::to_svg("flowchart LR\n  A --> B\n").unwrap();
        let st = read_structure(&svg);
        assert!(st.edges.is_empty(), "흐름도 엣지는 지금 앱도 못 읽는다");
        assert_eq!(focus_css(&st, &st.nodes[0].key), None);
    }

    #[test]
    fn fit_contains_and_centers() {
        let (s, (x, y)) = fit((800., 400.), (400., 400.));
        assert_eq!(s, 1.);
        assert_eq!((x, y), (200., 0.));
        let (s, _) = fit((200., 200.), (400., 100.));
        assert_eq!(s, 0.5);
    }

    #[test]
    fn zooming_keeps_the_point_under_the_cursor() {
        let (pan, old, new, p) = ((10., 20.), 1., 2., (110., 120.));
        let n = zoom_at(pan, old, new, p);
        // 커서 밑 그림 좌표가 그대로
        let before = ((p.0 - pan.0) / old, (p.1 - pan.1) / old);
        let after = ((p.0 - n.0) / new, (p.1 - n.1) / new);
        assert_eq!(before, after);
    }

    #[test]
    fn visible_region_is_clipped_to_the_figure() {
        // 100% 에서 다 보이면 그림 전체
        assert_eq!(
            visible_region((400., 400.), (0., 0.), 1., (400., 400.)),
            (0., 0., 400., 400.)
        );
        // 4배로 왼쪽 위만 보이면 그 둘레만
        let r = visible_region((400., 400.), (0., 0.), 4., (400., 400.));
        assert_eq!(r, (0., 0., 150., 150.));
    }
}
