// ERD 탭 — 지금 앱 ErdView.tsx(연결 → 스키마 → 표 트리, 머리, 캔버스)와 resin/viewer 의 PanZoom(view.ts).
// 그림은 resin 이 WebGL2 없이 그리는 정지 SVG(stage: true, `<무대>-<어둠>` 룩) — 유리 배경은 D4g.
// 스냅샷(vault/erd/<연결>/<스키마>/schema.json) → erd_gen → resin::compile → svg::to_svg → usvg 를 뒤에서 돌리고,
// 같은 그림(키)이면 다시 그리지 않는다. 이동/확대는 resin 과 같다: 휠 = 포인터 자리, 끌기 = 이동, 더블클릭 = 맞춤.
// 누르기는 지금 앱 ErdCanvas.tsx 와 같다: 표 이름 = 오른쪽 패널, 컬럼 = 정보창, 표 몸통 = 초점만, 빈 곳 = 정보창 닫고
// 초점은 패널의 표로. 패널은 X 와 Esc 로만 닫힌다. 초점(흐림)은 그린 SVG 에 CSS 를 얹어 다시 읽는다(resin::viewer).

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use amber_gpui::dbconn::{
    DbConnection, DbSchemaPref, conn_status, conn_status_dot, enabled_schemas, env_label,
    pref_audit, schema_folder, snapshot_path,
};
use amber_gpui::erd_gen::SchemaSnapshot;
use amber_gpui::i18n::{t, t_with};
use amber_gpui::resin::Model;
use amber_gpui::resin::details::{Act, column_details, table_details};
use amber_gpui::resin::svg::{
    AuditView, CARD_PARTS, CARD_RADIUS, CardPart, Columns, Direction, Edges, SvgOptions,
};
use amber_gpui::resin::viewer::{Drawn, FindHit};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use resvg::usvg;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::Tag;
use crate::theme::Tokens;
use crate::tip::tip;

pub const LIST_PANE: crate::pane::PaneGrab = crate::pane::PaneGrab {
    key: "amber.erd.list-width",
    min: 240.,
    max: 640.,
};
const SELECTED_KEY: &str = "amber.erd.selected";
const VIEW_KEY: &str = "amber.erd.view";
const PANEL_KEY: &str = "amber.erd.panel";
const PANEL_MIN: f32 = 360.;
const STAGES: [&str; 3] = ["aurora", "silk", "caustic"];

// resin/viewer/view.ts 의 PanZoom
const MIN: f32 = 0.1;
const MAX: f32 = 4.;
/// 누른 뒤 이만큼 움직여야 끌기
const DRAG_THRESHOLD: f32 = 4.;
/// 움직임을 붙인 이동, 확대(확대 단추, 맞춤, 표 보여 주기)
const MOVE_MS: f32 = 200.;

/// 앱의 --ease-out: cubic-bezier(0.23, 1, 0.32, 1)
fn ease_out(t: f32) -> f32 {
    let (x1, y1, x2, y2) = (0.23_f32, 1_f32, 0.32_f32, 1_f32);
    let bez = |a: f32, b: f32, s: f32| {
        let u = 1. - s;
        3. * u * u * s * a + 3. * u * s * s * b + s * s * s
    };
    // x(s) = t 인 s 를 이분법으로
    let (mut lo, mut hi) = (0_f32, 1_f32);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.;
        if bez(x1, x2, mid) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bez(y1, y2, (lo + hi) / 2.)
}

/// 보기 — 지금 앱 ViewPrefs(amber.erd.view). 그리기 옵션은 resin 이 받는다
#[derive(Debug, Clone, PartialEq)]
pub struct ViewPrefs {
    pub columns: Columns,
    pub audit: AuditView,
    pub edges: Edges,
    pub direction: Direction,
    pub hide: Vec<CardPart>,
    pub grid: bool,
    pub stage: String,
}

impl Default for ViewPrefs {
    fn default() -> Self {
        ViewPrefs {
            columns: Columns::All,
            audit: AuditView::Collapse,
            edges: Edges::Angular,
            direction: Direction::Down,
            hide: Vec::new(),
            grid: true,
            stage: "aurora".into(),
        }
    }
}

impl ViewPrefs {
    /// 저장된 보기 — 모르는 값은 기본으로(지금 앱의 읽기와 같은 너그러움)
    fn load(db: &amber_gpui::db::Db) -> Self {
        let d = ViewPrefs::default();
        let Some(v) = amber_gpui::prefs::get(db, VIEW_KEY)
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        else {
            return d;
        };
        let s = |k: &str| {
            v.get(k)
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string()
        };
        ViewPrefs {
            columns: match s("columns").as_str() {
                "keys" => Columns::Keys,
                "none" => Columns::None,
                _ => Columns::All,
            },
            audit: if s("audit") == "expand" {
                AuditView::Expand
            } else {
                AuditView::Collapse
            },
            edges: if s("edges") == "curved" {
                Edges::Curved
            } else {
                Edges::Angular
            },
            direction: if s("direction") == "right" {
                Direction::Right
            } else {
                Direction::Down
            },
            hide: v
                .get("hide")
                .and_then(|h| serde_json::from_value::<Vec<serde_json::Value>>(h.clone()).ok())
                .map(|h| {
                    CARD_PARTS
                        .iter()
                        .copied()
                        .filter(|p| {
                            h.iter()
                                .any(|x| serde_json::to_value(p).ok().as_ref() == Some(x))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            grid: v.get("grid").and_then(|g| g.as_bool()).unwrap_or(true),
            stage: Some(s("stage"))
                .filter(|x| STAGES.contains(&x.as_str()))
                .unwrap_or(d.stage),
        }
    }

    fn save(&self, db: &amber_gpui::db::Db) {
        let v = serde_json::json!({
            "columns": match self.columns { Columns::All => "all", Columns::Keys => "keys", Columns::None => "none" },
            "audit": if self.audit == AuditView::Expand { "expand" } else { "collapse" },
            "edges": if self.edges == Edges::Curved { "curved" } else { "angular" },
            "direction": if self.direction == Direction::Right { "right" } else { "down" },
            "hide": self.hide,
            "grid": self.grid,
            "stage": self.stage,
        });
        amber_gpui::prefs::set(db, VIEW_KEY, &v.to_string());
    }
}

/// 끌어 옮기기 — GPUI 의 끌기 값
#[derive(Clone)]
pub struct ErdGrab;

pub struct ErdDrag;

impl Render for ErdDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// 표 패널의 왼쪽 가장자리 끌기
#[derive(Clone)]
pub struct ErdPanelGrab;

/// 다 그린 것 — 뒤에서 만들어 넘긴다
struct Drawing {
    tree: Arc<usvg::Tree>,
    size: (f32, f32),
    background: Option<String>,
    drawn: Arc<Drawn>,
    model: Arc<Model>,
}

/// 그린 뒤에 할 일 — 다른 스키마의 표를 트리에서 골랐거나, 숨은 컬럼을 패널에서 골랐다
#[derive(Debug, Clone, PartialEq)]
enum AfterDraw {
    Open(String),
    Pick(String, String),
}

/// ⌘F 찾기 — 그린 표 이름과 컬럼 이름에서(ErdCanvas.tsx runFind)
pub struct ErdFind {
    pub input: Entity<InputState>,
    pub query: String,
    pub hits: Vec<FindHit>,
    pub idx: usize,
    /// 찾느라 초점 흐림을 걷었나 — 비우거나 닫을 때 한 번만 되돌린다
    lifted: bool,
}

/// 손이 올라간 곳 — 머리글과 행은 옅게 칠하고, 카드 위면 손가락 모양
#[derive(Debug, Clone, Copy, PartialEq)]
enum Over {
    Head(usize),
    Row(usize, usize),
    Body(usize),
}

#[derive(Default)]
pub struct ErdCanvas {
    /// 누른 곳을 찾고 초점을 칠할 그림의 뼈대, 패널과 정보창이 읽는 모델
    pub drawn: Option<Arc<Drawn>>,
    pub model: Option<Arc<Model>>,
    /// 초점 없는 그림 — 초점을 걷을 때 다시 읽지 않는다
    plain: Option<Arc<usvg::Tree>>,
    /// 초점: 표, 컬럼
    focus: Option<(String, Option<String>)>,
    /// 지금 얹은(얹는 중인) CSS — 격자, 초점, 찾기를 합친 것
    css: String,
    focus_task: Option<Task<()>>,
    focus_gen: u64,
    /// [보기] 판이 열렸나, [보기] 단추와 찾기 바의 자리(부딪히면 찾기 바가 한 줄 내려간다)
    pub view_open: bool,
    view_bounds: Rc<Cell<Bounds<Pixels>>>,
    find_w: Rc<Cell<f32>>,
    pub find: Option<ErdFind>,
    dark: bool,
    /// 오른쪽 패널의 표, 정보창의 컬럼
    pub inspected: Option<String>,
    pub popped: Option<(String, String)>,
    after_draw: Option<AfterDraw>,
    over: Option<Over>,
    /// 움직임 — 시작 시각과 시작 자리(x, y, 배율). 목표는 x, y, scale
    anim: Option<(Instant, (f32, f32, f32))>,
    /// 마지막으로 그리라고 한 그림(스키마, 구조) — 바뀌면 패널과 정보창을 닫는다
    drawing_id: String,
    /// 패널 폭(amber.erd.panel)과 끄는 중의 처음 자리
    pub panel_w: f32,
    panel_grab: Option<(f32, f32)>,
    /// 정보창의 잰 크기 — 다음 그리기에서 자리를 잡는다
    pop_size: Rc<Cell<(f32, f32)>>,
    /// 지금 보이는 그림의 키(모델과 보기를 함께 말한다)
    pub key: String,
    /// 그리는 중인 키
    pending: Option<String>,
    pub tree: Option<Arc<usvg::Tree>>,
    pub size: (f32, f32),
    background: Option<String>,
    pub error: Option<String>,
    /// 이동, 확대 — 그림 왼쪽 위의 판 안 자리와 배율
    pub scale: f32,
    pub x: f32,
    pub y: f32,
    /// 손대기 전까지는 판 크기가 바뀌면 다시 맞춘다
    auto: bool,
    fit_pending: bool,
    pub viewport: Rc<Cell<Bounds<Pixels>>>,
    dpr: Rc<Cell<f32>>,
    press: Option<(Point<Pixels>, (f32, f32))>,
    /// 끄는 중 — 누른 자리, 그때의 x, y, 문턱을 넘었나
    grab: Option<(Point<Pixels>, (f32, f32), bool)>,
    pub raster: crate::raster::Raster,
    task: Option<Task<()>>,
    /// 다른 스키마이거나 방향이 바뀌면 맞춘다 — 같은 그림의 보기만 바뀌면 보던 자리를 지킨다
    shown_id: String,
    shown_direction: Option<Direction>,
    measured: (f32, f32),
}

impl ErdCanvas {
    fn view(&self) -> (f32, f32) {
        let b = self.viewport.get();
        (f32::from(b.size.width), f32::from(b.size.height))
    }

    /// 아주 큰 그림은 10% 아래가 필요하다 — 그 뒤의 확대가 그 한계로 튀지 않게
    fn fit_scale(&self) -> f32 {
        let (vw, vh) = self.view();
        if self.size.0 <= 0. || self.size.1 <= 0. {
            return MIN;
        }
        ((vw - 48.).max(1.) / self.size.0)
            .min((vh - 48.).max(1.) / self.size.1)
            .min(1.25)
    }

    fn clamp(&self, s: f32) -> f32 {
        MAX.min(MIN.min(self.fit_scale()).max(s))
    }

    /// 초점 없는 그림(시험이 칠한 그림과 견준다)
    #[cfg(test)]
    pub fn plain_tree(&self) -> Option<Arc<usvg::Tree>> {
        self.plain.clone()
    }

    /// 칠한 초점(시험용)
    #[cfg(test)]
    pub fn focus_target(&self) -> Option<(String, Option<String>)> {
        self.focus.clone()
    }

    /// 지금 보이는 자리와 배율 — 움직이는 중이면 그 사이(x, y, scale 은 목표)
    pub fn shown(&self) -> (f32, f32, f32) {
        match self.anim {
            Some((start, (x0, y0, s0))) => {
                let t = (start.elapsed().as_secs_f32() * 1000. / MOVE_MS).min(1.);
                let k = ease_out(t);
                (
                    x0 + (self.x - x0) * k,
                    y0 + (self.y - y0) * k,
                    s0 + (self.scale - s0) * k,
                )
            }
            None => (self.x, self.y, self.scale),
        }
    }

    /// 목표를 바꾸기 전에 — 움직임을 붙이면 지금 보이는 자리에서 출발한다
    fn begin(&mut self, animate: bool) {
        self.anim = animate.then(|| (Instant::now(), self.shown()));
    }

    /// 움직임이 끝났으면 걷는다. 아직이면 true
    fn tick(&mut self) -> bool {
        match self.anim {
            Some((start, _)) if start.elapsed() < Duration::from_millis(MOVE_MS as u64) => true,
            Some(_) => {
                self.anim = None;
                false
            }
            None => false,
        }
    }

    pub fn fit(&mut self, animate: bool) {
        self.auto = true;
        let (vw, vh) = self.view();
        if self.size.0 <= 0. || vw <= 0. || vh <= 0. {
            self.fit_pending = true;
            return;
        }
        self.begin(animate && !self.fit_pending);
        self.scale = self.fit_scale();
        self.x = (vw - self.size.0 * self.scale) / 2.;
        self.y = (vh - self.size.1 * self.scale) / 2.;
        self.fit_pending = false;
        self.measured = (vw, vh);
        // 전체 그림의 해상도가 맞춘 배율에 달려 있다
        self.raster.full = None;
        self.raster.sharp = None;
    }

    /// 판 안의 점(기본은 가운데)을 붙잡고
    pub fn zoom_by(&mut self, factor: f32, at: Option<(f32, f32)>, animate: bool) {
        if self.tree.is_none() {
            return;
        }
        self.auto = false;
        self.begin(animate);
        let (vw, vh) = self.view();
        let (px_, py_) = at.unwrap_or((vw / 2., vh / 2.));
        let next = self.clamp(self.scale * factor);
        let k = next / self.scale;
        self.x = px_ - (px_ - self.x) * k;
        self.y = py_ - (py_ - self.y) * k;
        self.scale = next;
    }

    pub fn actual_size(&mut self) {
        let s = self.scale;
        self.zoom_by(1. / s, None, true);
    }

    /// 그림의 한 점을 보이는 곳(오른쪽 right 만큼은 패널이 가린다)의 가운데로
    fn center_on(&mut self, cx: f32, cy: f32, right: f32, animate: bool) {
        self.auto = false;
        self.begin(animate);
        let (vw, vh) = self.view();
        self.x = (vw - right) / 2. - cx * self.scale;
        self.y = vh / 2. - cy * self.scale;
    }

    /// 패널이 가리는 폭 — 캔버스가 120px 은 남는다
    pub fn covered(&self) -> f32 {
        if self.inspected.is_none() {
            return 0.;
        }
        self.panel_w
            .max(PANEL_MIN)
            .min(self.view().0 - 120.)
            .max(0.)
    }

    /// 표를 보이는 곳 가운데로. only_if_covered 면 패널에 안 가린 표는 그대로 둔다
    fn reveal(&mut self, table: &str, only_if_covered: bool) {
        let Some(b) = self.drawn.as_ref().and_then(|d| d.card(table)).cloned() else {
            return;
        };
        let covered = self.covered();
        let right = self.x + (b.x + b.w) as f32 * self.scale;
        if only_if_covered && right <= self.view().0 - covered - 16. {
            return;
        }
        self.center_on(
            (b.x + b.w / 2.) as f32,
            (b.y + b.h / 2.) as f32,
            covered,
            true,
        );
    }

    /// 컬럼 행을 보이는 곳 가운데로 — 이미 다 보이면 그대로(revealElement)
    fn reveal_row(&mut self, table: &str, column: &str) {
        if let Some(r) = self.row_rect(table, column) {
            self.reveal_area(r, true);
        }
    }

    /// 찾기에 걸린 것(머리글이나 행)을 보이게
    fn reveal_hit(&mut self, h: FindHit, animate: bool) {
        let Some(card) = self
            .drawn
            .as_ref()
            .and_then(|d| d.cards.get(h.card))
            .cloned()
        else {
            return;
        };
        let (y0, y1) = match h.row.and_then(|j| card.rows.get(j)) {
            Some(r) => (r.y, r.y + r.h),
            None => (card.y, card.y + card.head()),
        };
        let (x, y, s) = self.shown();
        let r = (
            x + card.x as f32 * s,
            y + y0 as f32 * s,
            x + (card.x + card.w) as f32 * s,
            y + y1 as f32 * s,
        );
        self.reveal_area(r, animate);
    }

    /// 판 안의 구간(왼쪽, 위, 오른쪽, 아래)이 다 보이면 그대로, 아니면 가운데로
    fn reveal_area(&mut self, (x0, y0, x1, y1): (f32, f32, f32, f32), animate: bool) {
        let (vw, vh) = self.view();
        let covered = self.covered();
        if x0 >= 8. && x1 <= vw - covered - 8. && y0 >= 8. && y1 <= vh - 8. {
            return;
        }
        let (x, y, s) = self.shown();
        self.center_on(
            ((x0 + x1) / 2. - x) / s,
            ((y0 + y1) / 2. - y) / s,
            covered,
            animate,
        );
    }

    /// 컬럼 행이 판 안 어디에 보이나(왼쪽, 위, 오른쪽, 아래)
    fn row_rect(&self, table: &str, column: &str) -> Option<(f32, f32, f32, f32)> {
        let card = self.drawn.as_ref()?.card(table)?;
        let r = card.rows.iter().find(|r| r.column == column)?;
        let (x, y, s) = self.shown();
        Some((
            x + (card.x + 1.) as f32 * s,
            y + r.y as f32 * s,
            x + (card.x + card.w - 1.) as f32 * s,
            y + (r.y + r.h) as f32 * s,
        ))
    }

    /// 판 안의 점에 무엇이 그려져 있나 — 위에 그린 카드부터
    fn over_at(&self, p: (f32, f32)) -> Option<Over> {
        let d = self.drawn.as_ref()?;
        let (x, y, s) = self.shown();
        let (gx, gy) = (((p.0 - x) / s) as f64, ((p.1 - y) / s) as f64);
        let (i, c) = d
            .cards
            .iter()
            .enumerate()
            .rev()
            .find(|(_, c)| gx >= c.x && gx <= c.x + c.w && gy >= c.y && gy <= c.y + c.h)?;
        if gx >= c.x + 1.
            && gx <= c.x + c.w - 1.
            && let Some(j) = c.rows.iter().position(|r| gy >= r.y && gy < r.y + r.h)
        {
            return Some(Over::Row(i, j));
        }
        Some(if gy < c.y + c.head() {
            Over::Head(i)
        } else {
            Over::Body(i)
        })
    }

    pub fn pct(&self) -> u32 {
        (self.scale * 100.).round() as u32
    }

    /// 휠 — 줄, 쪽 단위는 픽셀로, 한 번에 20% 넘게 키우지 않는다. 트랙패드 핀치(ctrl)는 더 세게
    fn wheel(&mut self, e: &ScrollEvent) {
        let (dy, unit) = match e.delta {
            ScrollDelta::Pixels(p) => (f32::from(p.y), 1.),
            ScrollDelta::Lines(l) => (l.y, 16.),
        };
        // DOM 의 deltaY 는 GPUI 의 반대 방향이다
        let delta = (-dy * unit).clamp(-120., 120.);
        let o = self.viewport.get().origin;
        let p = (f32::from(e.position.x - o.x), f32::from(e.position.y - o.y));
        let k = if e.modifiers.control { 0.01 } else { 0.0018 };
        self.zoom_by((-delta * k).exp(), Some(p), false);
    }
}

#[derive(Default)]
pub struct ErdState {
    /// None = 아직 안 읽었다
    pub connections: Option<Vec<DbConnection>>,
    /// 스키마 폴더 → 스냅샷(없으면 None)
    pub snaps: HashMap<String, Option<Arc<SchemaSnapshot>>>,
    pub selected: Option<(String, String)>,
    pub expanded: HashSet<String>,
    pub view: ViewPrefs,
    pub canvas: ErdCanvas,
    pub width: f32,
    pub error: Option<String>,
    /// [개요] 보기 — 캔버스는 그대로 두고(시점을 잃지 않게) 몸통만 바꾼다
    pub overview: bool,
    pub ov_input: Option<Entity<InputState>>,
    pub ov_query: String,
    /// 트리에서 찾기(⌘F, 트리에 포커스가 있을 때)
    pub tree_find: Option<crate::note_find::TreeFindState>,
    /// 동기화 — 도는 중, 차례를 기다리는 스키마 폴더, 바뀐 것(띠), 이번 실행에 자동으로 한 번 해 본 것
    pub syncing: HashSet<String>,
    pub queued: HashSet<String>,
    pub diffs: HashMap<String, amber_gpui::schema_snapshot::SchemaDiff>,
    pub attempted: HashSet<String>,
    /// DB 와 키체인 — None 이면 amber-core. 시험과 찍기는 가짜를 꽂는다
    pub db: Option<Rc<dyn crate::erd_sync::DbBackend>>,
    /// DB 연결 모달(새 연결, 편집)
    pub conn: Option<crate::erd_conn::ConnModal>,
}

/// 트리를 찾기로 거른 모양 — 남길 경로(없으면 다), 임시로 펼칠 경로, 칠할 검색어, 지금 것
#[derive(Default)]
struct TreeView {
    kept: Option<HashSet<String>>,
    expand: HashSet<String>,
    query: Option<String>,
    current: Option<String>,
}

impl TreeView {
    fn keeps(&self, path: &str) -> bool {
        self.kept.as_ref().is_none_or(|k| k.contains(path))
    }
}

fn conn_path(c: &DbConnection) -> String {
    format!("c{}", c.id)
}
fn schema_path(c: &DbConnection, schema: &str) -> String {
    format!("c{}/{schema}", c.id)
}
fn is_audit(name: &str) -> bool {
    name.ends_with("_aud") || name == "revinfo"
}
/// 즐겨찾기 스키마는 제 연결 안에서 맨 위로
fn sorted_schemas(c: &DbConnection) -> Vec<&DbSchemaPref> {
    let mut prefs = enabled_schemas(c);
    prefs.sort_by(|a, b| {
        (b.starred == Some(true))
            .cmp(&(a.starred == Some(true)))
            .then_with(|| cmp(&a.name, &b.name))
    });
    prefs
}
/// 도메인 표 먼저, 감사 표는 뒤로
fn sorted_tables(snap: &SchemaSnapshot) -> Vec<&amber_gpui::erd_gen::SnapshotTable> {
    let mut tables: Vec<_> = snap.tables.iter().collect();
    tables.sort_by(|a, b| {
        is_audit(&a.name)
            .cmp(&is_audit(&b.name))
            .then_with(|| cmp(&a.name, &b.name))
    });
    tables
}
fn cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

impl Spike {
    /// 처음 들어올 때 — 연결과 스냅샷을 읽고, 고른 스키마를 되살린다
    pub(crate) fn load_erd(&mut self) {
        let list: Vec<DbConnection> = match amber_gpui::dbconn::list_connections(&self.db) {
            Ok(rows) => rows
                .into_iter()
                .filter_map(|r| serde_json::from_value(r).ok())
                .collect(),
            Err(e) => {
                self.erd.error = Some(e.to_string());
                Vec::new()
            }
        };
        let tree = amber_gpui::vault::Tree::new(&self.db, &amber_gpui::vault::ERD);
        let mut snaps = HashMap::new();
        for c in &list {
            for p in enabled_schemas(c) {
                let snap = tree
                    .read_file_if_exists(&snapshot_path(c, &p.name))
                    .ok()
                    .flatten()
                    .and_then(|body| amber_gpui::schema_snapshot::parse_snapshot(&body))
                    .map(Arc::new);
                snaps.insert(schema_folder(c, &p.name), snap);
            }
        }
        let e = &mut self.erd;
        if e.selected.is_none() {
            e.selected = amber_gpui::prefs::get(&self.db, SELECTED_KEY)
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|v| {
                    Some((
                        v.get("ulid")?.as_str()?.to_string(),
                        v.get("schema")?.as_str()?.to_string(),
                    ))
                });
        }
        e.view = ViewPrefs::load(&self.db);
        e.snaps = snaps;
        // 고른 스키마의 연결은 펼쳐 둔다 — 다시 열었을 때 트리에서 어디인지 보이게
        if let Some((ulid, _)) = &e.selected
            && let Some(c) = list.iter().find(|c| &c.ulid == ulid)
        {
            e.expanded.insert(conn_path(c));
        }
        e.connections = Some(list);
    }

    /// 고른 스키마 — (연결, 설정)
    fn erd_current(&self) -> Option<(&DbConnection, &DbSchemaPref)> {
        let (ulid, schema) = self.erd.selected.as_ref()?;
        let c = self
            .erd
            .connections
            .as_ref()?
            .iter()
            .find(|c| &c.ulid == ulid)?;
        let p = enabled_schemas(c).into_iter().find(|p| &p.name == schema)?;
        Some((c, p))
    }

    pub(crate) fn open_erd_schema(&mut self, ulid: &str, schema: &str, cx: &mut Context<Self>) {
        let next = Some((ulid.to_string(), schema.to_string()));
        // 다른 스키마로 간다 — 예약해 둔 일도 버린다
        if self.erd.selected != next {
            self.erd.canvas.after_draw = None;
        }
        self.erd.selected = next;
        amber_gpui::prefs::set(
            &self.db,
            SELECTED_KEY,
            &serde_json::json!({"ulid": ulid, "schema": schema}).to_string(),
        );
        if let Some(c) = self
            .erd
            .connections
            .as_ref()
            .and_then(|l| l.iter().find(|c| c.ulid == ulid))
        {
            let p = conn_path(c);
            self.erd.expanded.insert(p);
            // 열 때 한 번, 스냅샷이 오래됐으면(10분) 다시 읽는다 — 폴링하지 않는다
            let synced = self
                .erd
                .snaps
                .get(&schema_folder(c, schema))
                .cloned()
                .flatten()
                .map(|s| s.synced_at);
            if amber_gpui::dbconn::is_stale(synced, self.db.now()) {
                let c = c.clone();
                if let Some(task) = self.erd_sync_one(&c, schema, cx) {
                    task.detach();
                }
            }
        }
    }

    pub(crate) fn set_erd_stage(&mut self, stage: &str) {
        self.erd.view.stage = stage.to_string();
        self.erd.view.save(&self.db);
    }

    /// render 에서 — 처음이면 읽고, 그릴 키가 바뀌었으면 뒤에서 그리고, 판을 재서 맞추고 굽는다
    pub(crate) fn prepare_erd(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.erd.connections.is_none() {
            self.erd_enter(cx);
        }
        let dark = Tokens::get(cx).dark;
        self.draw_erd(dark, cx);
        let c = &mut self.erd.canvas;
        let v = c.view();
        if c.tree.is_some() && v.0 > 0. && (c.fit_pending || (c.auto && c.measured != v)) {
            c.fit(false);
        }
        if c.tick() {
            window.request_animation_frame();
        }
        if let Some(tree) = c.tree.clone()
            && !c.fit_pending
            && c.scale > 0.
        {
            let (x, y, scale) = c.shown();
            let g = crate::raster::Geometry {
                size: c.size,
                scale,
                pan: (x, y),
                view: v,
                dpr: c.dpr.get(),
                full_base: c.fit_scale(),
            };
            crate::raster::schedule(&mut c.raster, tree, g, |s| &mut s.erd.canvas.raster, cx);
        }
    }

    /// 스냅샷 → resin → SVG → 트리. 같은 키면 다시 그리지 않는다
    fn draw_erd(&mut self, dark: bool, cx: &mut Context<Self>) {
        let Some((conn, pref)) = self.erd_current() else {
            return;
        };
        let folder = schema_folder(conn, &pref.name);
        let Some(snap) = self.erd.snaps.get(&folder).cloned().flatten() else {
            return;
        };
        let audit = pref_audit(pref);
        let drawing_id = format!("{}/{}/{}/{}", conn.ulid, pref.name, snap.fingerprint, audit);
        let view = self.erd.view.clone();
        let look = format!("{}-{}", view.stage, if dark { "dark" } else { "light" });
        let key = format!("{drawing_id}|{view:?}|{look}");
        let c = &mut self.erd.canvas;
        // 다른 스키마(또는 구조) — 그 표가 거기엔 없으니 패널과 정보창을 닫는다
        if c.drawing_id != drawing_id {
            c.drawing_id = drawing_id.clone();
            c.inspected = None;
            c.popped = None;
        }
        c.dark = dark;
        if c.key == key || c.pending.as_deref() == Some(key.as_str()) {
            return;
        }
        c.pending = Some(key.clone());
        let options = SvgOptions {
            columns: view.columns,
            audit: view.audit,
            look,
            stage: Some(true),
            edges: view.edges,
            direction: view.direction,
            hide: view.hide.clone(),
            id_prefix: Some("erd-".into()),
            ..Default::default()
        };
        let direction = view.direction;
        let task = cx.spawn(async move |this, cx| {
            let out = cx
                .background_executor()
                .spawn(async move {
                    let src = amber_gpui::erd_gen::generate_erd(&snap, Some(audit)).source;
                    let r = amber_gpui::resin::compile(&src);
                    let Some(model) = r.model else {
                        let e = r
                            .diagnostics
                            .iter()
                            .find(|d| d.severity == amber_gpui::resin::Severity::Error)
                            .map(|d| format!("{}:{} {}", d.span.line, d.span.col, d.message))
                            .unwrap_or_default();
                        return Err(e);
                    };
                    let svg = amber_gpui::resin::svg::to_svg(&model, &options)?;
                    let drawn = Drawn::read(&svg.svg, &svg.boxes)?;
                    let tree = crate::figure::parse(&drawn.svg, "#333333")?;
                    Ok(Drawing {
                        tree: Arc::new(tree),
                        size: (svg.width as f32, svg.height as f32),
                        background: svg.background,
                        drawn: Arc::new(drawn),
                        model: Arc::new(model),
                    })
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                let c = &mut s.erd.canvas;
                if c.pending.as_deref() != Some(key.as_str()) {
                    return;
                }
                c.pending = None;
                c.key = key;
                match out {
                    Ok(d) => {
                        let fit = c.shown_id != drawing_id || c.shown_direction != Some(direction);
                        c.tree = Some(d.tree.clone());
                        c.plain = Some(d.tree);
                        c.drawn = Some(d.drawn);
                        c.model = Some(d.model);
                        c.focus = None;
                        c.css = String::new();
                        c.focus_task = None;
                        c.over = None;
                        c.size = d.size;
                        c.background = d.background;
                        c.error = None;
                        c.raster.reset();
                        c.shown_id = drawing_id;
                        c.shown_direction = Some(direction);
                        if fit || c.scale <= 0. {
                            c.fit_pending = true;
                            c.anim = None;
                        }
                        s.erd_redrawn(cx);
                    }
                    Err(e) => c.error = Some(e),
                }
                cx.notify();
            });
        });
        self.erd.canvas.task = Some(task);
    }

    pub(crate) fn erd_pan_to(&mut self, pos: Point<Pixels>) -> bool {
        let c = &mut self.erd.canvas;
        let Some((start, (x0, y0), dragged)) = c.grab else {
            return false;
        };
        let (dx, dy) = (f32::from(pos.x - start.x), f32::from(pos.y - start.y));
        // 4px 를 넘기 전엔 누르기다 — 넘으면 처음 자리부터 한 번에 따라간다
        if !dragged && dx.hypot(dy) < DRAG_THRESHOLD {
            return false;
        }
        c.grab = Some((start, (x0, y0), true));
        c.auto = false;
        c.anim = None;
        c.over = None;
        c.x = x0 + dx;
        c.y = y0 + dy;
        true
    }

    pub(crate) fn erd_drop(&mut self) {
        self.erd.canvas.grab = None;
    }

    /// 패널 왼쪽 가장자리를 끈다 — 캔버스 오른쪽 끝에서 마우스까지가 폭, 캔버스는 120px 은 남는다
    pub(crate) fn erd_panel_to(&mut self, pos: Point<Pixels>) -> bool {
        let c = &mut self.erd.canvas;
        if c.panel_grab.is_none() {
            return false;
        }
        let b = c.viewport.get();
        let right = f32::from(b.origin.x + b.size.width);
        let max = PANEL_MIN.max(f32::from(b.size.width) - 120.);
        c.panel_w = (right - f32::from(pos.x)).round().clamp(PANEL_MIN, max);
        true
    }

    pub(crate) fn erd_panel_drop(&mut self) {
        if self.erd.canvas.panel_grab.take().is_some() {
            amber_gpui::prefs::set(&self.db, PANEL_KEY, &self.erd.canvas.panel_w.to_string());
        }
    }

    /// 지금 고른 스키마의 그림을 말하는 값(draw_erd 와 같은 것)
    fn erd_drawing_id(&self) -> Option<String> {
        let (conn, pref) = self.erd_current()?;
        let snap = self
            .erd
            .snaps
            .get(&schema_folder(conn, &pref.name))
            .cloned()
            .flatten()?;
        Some(format!(
            "{}/{}/{}/{}",
            conn.ulid,
            pref.name,
            snap.fingerprint,
            pref_audit(pref)
        ))
    }

    /// 초점을 바꾼다(None = 걷는다). 그림에 없는 표면 걷는다(createFocus)
    fn erd_focus(&mut self, target: Option<(String, Option<String>)>, cx: &mut Context<Self>) {
        let c = &mut self.erd.canvas;
        let Some(drawn) = c.drawn.clone() else {
            return;
        };
        let target = target.filter(|(t, _)| drawn.card(t).is_some());
        if c.focus == target {
            return;
        }
        c.focus = target;
        self.erd_restyle(cx);
    }

    /// 그림에 얹을 CSS — 격자 끄기, 초점, 찾기. 바뀌었으면 뒤에서 다시 읽는다(없으면 칠하지 않은 그림으로)
    fn erd_restyle(&mut self, cx: &mut Context<Self>) {
        let grid = self.erd.view.grid;
        let c = &mut self.erd.canvas;
        let Some(drawn) = c.drawn.clone() else {
            return;
        };
        let mut css: Vec<String> = Vec::new();
        // .rz-viewer:not(.is-grid) .rz-grid { display: none }
        if !grid {
            css.push(".rz-grid { display: none; }".into());
        }
        if let Some((t, col)) = &c.focus
            && let Some(f) = drawn.focus(t, col.as_deref())
        {
            css.push(drawn.focus_css(&f, crate::erd_details::Ink::accent_hex(c.dark)));
        }
        if let Some(f) = &c.find {
            css.push(drawn.find_css(&f.hits, f.idx, c.dark));
        }
        let css = css.join(" ");
        if c.css == css {
            return;
        }
        c.css = css.clone();
        c.focus_gen += 1;
        let generation = c.focus_gen;
        if css.is_empty() {
            c.focus_task = None;
            if let Some(p) = c.plain.clone() {
                c.tree = Some(p);
                c.raster.stale = true;
            }
            return;
        }
        c.focus_task = Some(cx.spawn(async move |this, cx| {
            let tree = cx
                .background_executor()
                .spawn(async move { crate::figure::parse_styled(&drawn.svg, "#333333", Some(css)) })
                .await;
            let _ = this.update(cx, |s, cx| {
                let c = &mut s.erd.canvas;
                if c.focus_gen != generation {
                    return;
                }
                match tree {
                    Ok(t) => {
                        c.tree = Some(Arc::new(t));
                        c.raster.stale = true;
                    }
                    Err(e) => eprintln!("ERD 칠하기: {e}"),
                }
                cx.notify();
            });
        }));
    }

    /// 아무것도 고르지 않았을 때의 초점 — 패널에 표가 있으면 그 표, 없으면 없음
    fn erd_settle(&mut self, cx: &mut Context<Self>) {
        let t = self.erd.canvas.inspected.clone();
        self.erd_focus(t.map(|t| (t, None)), cx);
    }

    /// 표 패널을 연다 — 정보창은 닫고, 그 표에 초점, 패널에 가렸으면 보이게
    fn erd_open_table(&mut self, name: &str, only_if_covered: bool, cx: &mut Context<Self>) {
        let c = &mut self.erd.canvas;
        if c.drawn.is_none() {
            return;
        }
        c.popped = None;
        c.inspected = Some(name.to_string());
        self.erd_focus(Some((name.to_string(), None)), cx);
        self.erd.canvas.reveal(name, only_if_covered);
    }

    /// 컬럼 정보창 — 같은 걸 다시 고르면 닫는다. 키만/이름만 보기라 안 그려진 컬럼이면 컬럼을 다 펼쳐 그린 뒤에 연다
    fn erd_pick(&mut self, table: &str, column: &str, cx: &mut Context<Self>) {
        let c = &mut self.erd.canvas;
        let Some(drawn) = c.drawn.clone() else {
            return;
        };
        if c.popped.as_ref() == Some(&(table.to_string(), column.to_string())) {
            c.popped = None;
            self.erd_settle(cx);
            return;
        }
        let drawn_row = drawn
            .card(table)
            .is_some_and(|card| card.rows.iter().any(|r| r.column == column));
        if !drawn_row {
            // 다 그렸는데도 없다 — 펼쳐도 나오지 않는다
            if self.erd.view.columns == Columns::All {
                return;
            }
            c.after_draw = Some(AfterDraw::Pick(table.to_string(), column.to_string()));
            self.erd.view.columns = Columns::All;
            self.erd.view.save(&self.db);
            return;
        }
        c.popped = Some((table.to_string(), column.to_string()));
        self.erd_focus(Some((table.to_string(), Some(column.to_string()))), cx);
        self.erd.canvas.reveal_row(table, column);
    }

    /// 새 그림이 올라왔다 — 열려 있던 것을 다시 잡고, 예약해 둔 일을 한다
    fn erd_redrawn(&mut self, cx: &mut Context<Self>) {
        let c = &mut self.erd.canvas;
        let (Some(drawn), Some(model)) = (c.drawn.clone(), c.model.clone()) else {
            return;
        };
        // 모델에 없는 표의 패널은 닫는다(감사 표를 접었을 때)
        if c.inspected
            .as_ref()
            .is_some_and(|t| !model.tables.iter().any(|x| &x.name == t))
        {
            c.inspected = None;
        }
        let pop = c.popped.clone().filter(|(t, col)| {
            drawn
                .card(t)
                .is_some_and(|card| card.rows.iter().any(|r| &r.column == col))
        });
        match pop {
            Some((t, col)) => self.erd_focus(Some((t, Some(col))), cx),
            None => {
                self.erd.canvas.popped = None;
                self.erd_settle(cx);
            }
        }
        // 찾기를 새 그림에 다시(찾는 중이면 초점을 다시 걷는다)
        self.erd_run_find(false, cx);
        self.erd_restyle(cx);
        match self.erd.canvas.after_draw.take() {
            Some(AfterDraw::Open(t)) => self.erd_open_table(&t, false, cx),
            Some(AfterDraw::Pick(t, col)) => self.erd_pick(&t, &col, cx),
            None => {}
        }
    }

    /// 판을 눌렀다(4px 안에서 놓았다) — hitOf 그대로
    fn erd_tap(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let c = &self.erd.canvas;
        let Some(drawn) = c.drawn.clone() else {
            return;
        };
        let o = c.viewport.get().origin;
        let (x, y, s) = c.shown();
        let hit = drawn.hit(
            ((f32::from(pos.x - o.x) - x) / s) as f64,
            ((f32::from(pos.y - o.y) - y) / s) as f64,
        );
        match (hit.table, hit.column) {
            (Some(t), Some(col)) => self.erd_pick(&t, &col, cx),
            (Some(t), None) if hit.header => self.erd_open_table(&t, true, cx),
            (t, _) => {
                self.erd.canvas.popped = None;
                match t {
                    Some(t) => self.erd_focus(Some((t, None)), cx),
                    None => self.erd_settle(cx),
                }
            }
        }
    }

    /// 패널, 정보창 안의 링크와 행
    pub(crate) fn erd_act(&mut self, act: &Act, _: &mut Window, cx: &mut Context<Self>) {
        match act {
            Act::Open(t) => self.erd_open_table(t, false, cx),
            Act::Pick(t, col) => self.erd_pick(t, col, cx),
            // amber 의 ERD 엔 서비스가 없다
            Act::Service(_) => {}
        }
    }

    /// Esc — 안쪽부터: [보기] 판, 정보창, 그다음 패널
    pub(crate) fn erd_escape(&mut self, cx: &mut Context<Self>) -> bool {
        let c = &mut self.erd.canvas;
        if std::mem::take(&mut c.view_open) {
            return true;
        }
        if c.popped.take().is_some() {
            self.erd_settle(cx);
            return true;
        }
        if c.inspected.take().is_some() {
            self.erd_focus(None, cx);
            return true;
        }
        false
    }

    fn erd_close_panel(&mut self, cx: &mut Context<Self>) {
        let c = &mut self.erd.canvas;
        c.inspected = None;
        c.popped = None;
        self.erd_focus(None, cx);
    }

    /// ⌘F — 찾기 바를 열거나(이미 열렸으면 글자를 고르고) 입력칸에 포커스
    pub(crate) fn erd_open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.erd_current().is_none() {
            return;
        }
        if let Some(f) = &self.erd.canvas.find {
            f.input.update(cx, |s, cx| {
                s.select_all(window, cx);
                s.focus(window, cx);
            });
            return;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(t("erd.canvas.findPh")));
        cx.subscribe_in(
            &input,
            window,
            |this, input, ev: &InputEvent, _, cx| match ev {
                InputEvent::Change => {
                    let q = input.read(cx).value().to_string();
                    let Some(f) = this.erd.canvas.find.as_mut() else {
                        return;
                    };
                    // Enter 뒤에도 값이 그대로인 Change 가 온다 — 검색어가 바뀔 때만
                    if f.query == q {
                        return;
                    }
                    f.query = q;
                    this.erd_run_find(true, cx);
                    cx.notify();
                }
                InputEvent::PressEnter { shift, .. } => {
                    this.erd_find_step(if *shift { -1 } else { 1 }, false, cx);
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();
        input.update(cx, |s, cx| s.focus(window, cx));
        self.erd.canvas.find = Some(ErdFind {
            input,
            query: String::new(),
            hits: Vec::new(),
            idx: 0,
            lifted: false,
        });
    }

    /// 다시 찾는다 — move 면 첫 결과로 간다(움직임 없이: 키보드 동작). 찾는 동안은 초점 흐림을 걷고, 비우면 되돌린다
    pub(crate) fn erd_run_find(&mut self, move_: bool, cx: &mut Context<Self>) {
        let c = &mut self.erd.canvas;
        let (Some(drawn), Some(f)) = (c.drawn.clone(), c.find.as_mut()) else {
            return;
        };
        f.hits = drawn.find(&f.query);
        f.idx = 0;
        let first = f.hits.first().copied();
        let term = !f.query.trim().is_empty();
        if term {
            f.lifted = true;
            c.focus = None;
        } else if f.lifted {
            f.lifted = false;
            self.erd_restore_focus();
        }
        if let (Some(h), true) = (first, move_) {
            self.erd.canvas.reveal_hit(h, false);
        }
        self.erd_restyle(cx);
    }

    /// 다음, 이전 — 찾는 사이 표를 눌러 다시 흐려졌을 수 있어 다시 걷는다
    fn erd_find_step(&mut self, delta: i64, animate: bool, cx: &mut Context<Self>) {
        let c = &mut self.erd.canvas;
        let Some(f) = c.find.as_mut() else {
            return;
        };
        let n = f.hits.len() as i64;
        if n == 0 {
            return;
        }
        f.idx = ((f.idx as i64 + delta).rem_euclid(n)) as usize;
        f.lifted = true;
        let h = f.hits[f.idx];
        c.focus = None;
        c.reveal_hit(h, animate);
        self.erd_restyle(cx);
    }

    pub(crate) fn erd_close_find(&mut self, cx: &mut Context<Self>) {
        if let Some(f) = self.erd.canvas.find.take()
            && f.lifted
        {
            self.erd_restore_focus();
        }
        self.erd_restyle(cx);
    }

    /// 패널과 정보창이 가리키는 것으로 초점을 되돌린다(칠하기는 부른 쪽이)
    fn erd_restore_focus(&mut self) {
        let c = &self.erd.canvas;
        let drawn = c.drawn.clone();
        let card = |t: &str| drawn.as_ref().and_then(|d| d.card(t));
        let target = match &c.popped {
            Some((t, col)) if card(t).is_some_and(|k| k.rows.iter().any(|r| &r.column == col)) => {
                Some((t.clone(), Some(col.clone())))
            }
            _ => c
                .inspected
                .clone()
                .filter(|t| card(t).is_some())
                .map(|t| (t, None)),
        };
        self.erd.canvas.focus = target;
    }

    /// 트리 — 연결 → 스키마(별표 먼저) → 표(감사 표는 뒤로). 찾기가 이 모양을 거른다
    pub(crate) fn erd_nodes(&self) -> Vec<amber_gpui::vault::Node> {
        use amber_gpui::vault::Node;
        let Some(list) = &self.erd.connections else {
            return Vec::new();
        };
        list.iter()
            .map(|c| Node {
                name: c.name.clone(),
                path: conn_path(c),
                is_dir: true,
                children: Some(
                    sorted_schemas(c)
                        .into_iter()
                        .map(|p| Node {
                            name: p.name.clone(),
                            path: schema_path(c, &p.name),
                            is_dir: true,
                            children: Some(
                                self.erd
                                    .snaps
                                    .get(&schema_folder(c, &p.name))
                                    .cloned()
                                    .flatten()
                                    .map(|snap| {
                                        sorted_tables(&snap)
                                            .into_iter()
                                            .map(|tb| Node {
                                                name: tb.name.clone(),
                                                path: format!(
                                                    "{}/{}",
                                                    schema_path(c, &p.name),
                                                    tb.name
                                                ),
                                                is_dir: false,
                                                children: None,
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                            ),
                        })
                        .collect(),
                ),
            })
            .collect()
    }

    /// 찾기의 Enter — 연결, 스키마는 펼치고(스키마면 그 ERD 도 연다) 찾기는 그대로, 표면 그 표를 열고 찾기를 닫는다
    pub(crate) fn erd_find_pick(&mut self, path: &str, is_dir: bool, cx: &mut Context<Self>) {
        let mut parts = path.splitn(3, '/');
        let (Some(conn), schema, table) = (parts.next(), parts.next(), parts.next()) else {
            return;
        };
        let Some(c) = self
            .erd
            .connections
            .as_ref()
            .and_then(|l| l.iter().find(|c| conn_path(c) == conn))
            .cloned()
        else {
            return;
        };
        if is_dir {
            self.erd.expanded.insert(path.to_string());
            if let Some(schema) = schema {
                self.open_erd_schema(&c.ulid, schema, cx);
            }
            return;
        }
        if let (Some(schema), Some(table)) = (schema, table) {
            self.erd.overview = false;
            self.erd_reveal_table(&c.ulid, schema, table, cx);
        }
        self.erd.tree_find = None;
    }

    /// 찾기가 열려 있으면 트리를 거른다
    fn erd_tree_view(&self) -> TreeView {
        let Some(f) = &self.erd.tree_find else {
            return TreeView::default();
        };
        let found = amber_gpui::tree_find::filter_tree(&self.erd_nodes(), &f.query);
        let active = !f.query.trim().is_empty();
        let mut kept = HashSet::new();
        fn walk(n: &[amber_gpui::vault::Node], kept: &mut HashSet<String>) {
            for x in n {
                kept.insert(x.path.clone());
                walk(x.children.as_deref().unwrap_or_default(), kept);
            }
        }
        walk(&found.nodes, &mut kept);
        TreeView {
            kept: active.then_some(kept),
            expand: found.expand.into_iter().collect(),
            query: active.then(|| f.query.clone()),
            current: found.matches.get(f.idx).map(|m| m.0.clone()),
        }
    }

    /// [개요] 로 — 캔버스가 안 보이니 찾기와 [보기] 판은 닫는다
    pub(crate) fn erd_show_overview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.erd.overview = true;
        self.erd.canvas.view_open = false;
        self.erd_close_find(cx);
        self.erd_overview_input(window, cx);
    }

    /// 개요의 테이블 행 — ERD 보기로 돌아가 그 표의 패널을 연다
    pub(crate) fn erd_pick_from_overview(&mut self, table: &str, cx: &mut Context<Self>) {
        let Some((ulid, schema)) = self.erd.selected.clone() else {
            return;
        };
        self.erd.overview = false;
        self.erd_reveal_table(&ulid, &schema, table, cx);
    }

    /// [보기] 판 — 보기를 바꾸면 기억하고 다시 그린다
    fn set_erd_view(&mut self, f: impl FnOnce(&mut ViewPrefs), cx: &mut Context<Self>) {
        let before = self.erd.view.grid;
        f(&mut self.erd.view);
        self.erd.view.save(&self.db);
        if self.erd.view.grid != before {
            self.erd_restyle(cx);
        }
    }

    /// 트리에서 표를 골랐다 — 그림이 이 스키마의 것이면 바로, 아니면 그린 다음에(그림에 없는 표도 패널은 연다)
    fn erd_reveal_table(&mut self, ulid: &str, schema: &str, table: &str, cx: &mut Context<Self>) {
        // 지금 스키마가 아닐 때만 연다(여는 일에 오래된 스냅샷 다시 읽기가 딸려 있다)
        if self.erd.selected.as_ref() != Some(&(ulid.to_string(), schema.to_string())) {
            self.open_erd_schema(ulid, schema, cx);
        }
        let shown = self
            .erd_drawing_id()
            .is_some_and(|id| self.erd.canvas.shown_id == id && self.erd.canvas.drawn.is_some());
        if shown {
            self.erd_open_table(table, false, cx);
        } else {
            self.erd.canvas.after_draw = Some(AfterDraw::Open(table.to_string()));
        }
    }

    pub(crate) fn erd_body(
        &self,
        body_w: f32,
        k: &Tokens,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let width = crate::pane::effective(self.erd.width, LIST_PANE.min, body_w);
        div()
            .size_full()
            .flex()
            .child(self.erd_tree(width, body_w, k, cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .pt(px(8.))
                    .pb(px(8.))
                    .pr(px(8.))
                    .pl(px(4.))
                    .child(
                        div()
                            .relative()
                            .size_full()
                            .overflow_hidden()
                            .border_1()
                            .border_color(k.glass_edge)
                            .rounded(px(12.))
                            .bg(crate::note_ai_view::glass(k.glass_fill))
                            .shadow(k.glass_shadow(false))
                            .child(self.erd_detail(k, window, cx)),
                    ),
            )
    }

    fn erd_tree(&self, width: f32, body_w: f32, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let (bg, text) = (k.surface_2, k.text);
        let head = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(12.))
            .py(px(10.))
            .child(
                div()
                    .flex_1()
                    .text_size(px(13.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(k.text)
                    .child(t("erd.tree.title")),
            )
            .child(
                div()
                    .id("erd-tree-find")
                    .tag("erd-tree-find")
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(bg).text_color(text))
                    .tooltip(tip(t("common.find.treeTip")))
                    .child(icon("search", 14.).text_color(k.text_2))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.start_tree_find(crate::note_find::FindIn::Erd, window, cx);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("erd-add-conn")
                    .tag("erd-add-conn")
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(bg).text_color(text))
                    .tooltip(tip(t("erd.db.tooltip.add")))
                    .child(icon("plus", 14.).text_color(k.text_2))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_conn_modal(None, window, cx);
                        cx.notify();
                    })),
            );
        let view = self.erd_tree_view();
        let count = self.tree_find_matches(crate::note_find::FindIn::Erd).len();
        let body: AnyElement = match &self.erd.connections {
            None => div().into_any_element(),
            Some(list) if list.is_empty() => div()
                .p(px(14.))
                .flex()
                .flex_col()
                .items_start()
                .gap(px(10.))
                .text_size(px(12.5))
                .text_color(k.text_3)
                .child(t("erd.empty.noConnection"))
                .child(
                    crate::day_card::primary_button("erd-connect", k, false)
                        .child(icon("database", 14.).text_color(k.surface))
                        .child(t("erd.empty.connect"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_conn_modal(None, window, cx);
                            cx.notify();
                        })),
                )
                .into_any_element(),
            Some(_) if view.kept.as_ref().is_some_and(|k| k.is_empty()) => div()
                .px(px(14.))
                .py(px(10.))
                .text_size(px(12.5))
                .text_color(k.text_3)
                .child(t("common.find.treeEmpty"))
                .into_any_element(),
            Some(list) => div()
                .id("erd-tree")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(px(6.))
                .pb(px(8.))
                .children(
                    list.iter()
                        .filter(|c| view.keeps(&conn_path(c)))
                        .flat_map(|c| self.erd_conn_rows(c, &view, k, cx)),
                )
                .into_any_element(),
        };
        let focus = self.etree_focus.clone();
        div()
            .relative()
            .w(px(width))
            .flex_shrink_0()
            .h_full()
            .pt(px(8.))
            .pb(px(8.))
            .pl(px(8.))
            .pr(px(4.))
            .child(
                // 트리 판 — 누르면 포커스를 가져가 그 뒤의 ⌘F 가 트리 찾기로 온다(캔버스의 ⌘F 와 안 겹치게)
                div()
                    .track_focus(&focus)
                    .size_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .border_1()
                    .border_color(k.glass_edge)
                    .rounded(px(12.))
                    .bg(crate::note_ai_view::glass(k.glass_fill))
                    .shadow(k.glass_shadow(false))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            let f = this.etree_focus.clone();
                            if !f.contains_focused(window, cx) {
                                f.focus(window, cx);
                            }
                        }),
                    )
                    .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                        if this.etree_focus.is_focused(window) {
                            this.focus.focus(window, cx);
                        }
                    }))
                    .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                        let m = &e.keystroke.modifiers;
                        if m.platform && !m.alt && e.keystroke.key == "f" {
                            this.start_tree_find(crate::note_find::FindIn::Erd, window, cx);
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }))
                    .child(head)
                    .children(self.tree_find_bar(crate::note_find::FindIn::Erd, count, k, cx))
                    .child(body),
            )
            .child(self.pane_resizer(LIST_PANE, width, body_w, k, cx))
    }

    fn erd_row(id: String, depth: usize, selected: bool, k: &Tokens) -> Stateful<Div> {
        let fill = |pair: (Hsla, Hsla)| {
            linear_gradient(
                150.,
                linear_color_stop(pair.0, 0.),
                linear_color_stop(pair.1, 1.),
            )
        };
        div()
            .id(SharedString::from(id.clone()))
            .tag(id)
            .group("tree-row")
            .relative()
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(8. + depth as f32 * 14.))
            .pr(px(6.))
            .py(px(4.))
            .rounded(px(6.))
            .border_1()
            .border_color(transparent_black())
            .cursor_pointer()
            .text_size(px(13.5))
            .when(!selected, |d| {
                let bg = fill(k.glass_fill);
                d.hover(move |s| s.bg(bg))
            })
            .when(selected, |d| {
                let mut shadow = k.glass_shadow(true)[..1].to_vec();
                shadow.push(BoxShadow {
                    color: k.text,
                    offset: point(px(2.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(0.),
                    inset: true,
                });
                d.bg(fill(k.glass_fill_accent))
                    .border_color(k.glass_edge_accent)
                    .shadow(shadow)
            })
    }

    fn erd_conn_rows(
        &self,
        c: &DbConnection,
        view: &TreeView,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let path = conn_path(c);
        let open = self.erd.expanded.contains(&path) || view.expand.contains(&path);
        let current = view.current.as_deref() == Some(path.as_str());
        let dot = conn_status_dot(conn_status(c));
        let folders: Vec<String> = enabled_schemas(c)
            .iter()
            .map(|p| schema_folder(c, &p.name))
            .collect();
        let running = folders
            .iter()
            .filter(|f| self.erd.syncing.contains(*f) || self.erd.queued.contains(*f))
            .count();
        let progress = (running > 0).then(|| (folders.len() - running, folders.len()));
        let dot_color = match dot {
            "ok" => k.success,
            "warn" => k.warning,
            "bad" => k.danger,
            _ => k.text_3,
        };
        let p = path.clone();
        let mut out = vec![
            Self::erd_row(format!("erd-{path}"), 0, false, k)
                .text_color(k.text)
                .font_weight(FontWeight::SEMIBOLD)
                .when(current, |d| {
                    d.shadow(vec![Tokens::ring(k.border_strong, 1.)])
                })
                .child(
                    icon(
                        if open {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        },
                        13.,
                    )
                    .text_color(k.text_3),
                )
                .child(icon("database", 14.).text_color(k.text_2))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(crate::note_find::label(
                            &c.name,
                            view.query.as_deref(),
                            current,
                            k,
                        )),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .text_size(px(11.))
                        .font_weight(FontWeight::NORMAL)
                        .text_color(k.text_3)
                        // 동기화가 도는 동안은 상태 점 대신 진행 — 기다리는 이유가 여기 보인다
                        .when_some(progress, |d, (done, total)| {
                            d.child(t_with(
                                "erd.db.tree.progress",
                                &[("done", &done.to_string()), ("total", &total.to_string())],
                            ))
                        })
                        .when(progress.is_none(), |d| {
                            d.child(
                                div()
                                    .id(SharedString::from(format!("erd-dot-{path}")))
                                    .size(px(7.))
                                    .rounded_full()
                                    .border_1()
                                    .border_color(dot_color)
                                    .when(!dot.is_empty(), |d| d.bg(dot_color))
                                    .tooltip(tip(amber_gpui::dbconn::conn_status_label(
                                        conn_status(c),
                                    ))),
                            )
                            .child(env_label(&c.env))
                        }),
                )
                .child({
                    let c = c.clone();
                    let edit = c.clone();
                    crate::note_names::row_actions(k)
                        .child(
                            crate::note_names::row_act(
                                format!("erd-sync-all-{path}"),
                                "refresh",
                                false,
                                k,
                            )
                            .tooltip(tip(t("erd.db.tree.syncAll")))
                            .when(progress.is_some(), |d| d.opacity(0.45))
                            .when(progress.is_none(), |d| {
                                d.on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    let todo = enabled_schemas(&c)
                                        .iter()
                                        .map(|p| p.name.clone())
                                        .collect();
                                    this.erd_sync_many(&c, todo, cx).detach();
                                    cx.notify();
                                }))
                            }),
                        )
                        .child(
                            crate::note_names::row_act(
                                format!("erd-edit-{path}"),
                                "settings",
                                false,
                                k,
                            )
                            .tooltip(tip(t("erd.db.tree.edit")))
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.open_conn_modal(Some(edit.clone()), window, cx);
                                    cx.notify();
                                },
                            )),
                        )
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.erd.expanded.remove(&p) {
                        this.erd.expanded.insert(p.clone());
                    }
                    cx.notify();
                }))
                .into_any_element(),
        ];
        if !open {
            return out;
        }
        for p in sorted_schemas(c) {
            if view.keeps(&schema_path(c, &p.name)) {
                out.extend(self.erd_schema_rows(c, p, view, k, cx));
            }
        }
        out
    }

    fn erd_schema_rows(
        &self,
        c: &DbConnection,
        p: &DbSchemaPref,
        view: &TreeView,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let path = schema_path(c, &p.name);
        let open = self.erd.expanded.contains(&path) || view.expand.contains(&path);
        let current = view.current.as_deref() == Some(path.as_str());
        let folder = schema_folder(c, &p.name);
        let snap = self.erd.snaps.get(&folder).cloned().flatten();
        let selected = self.erd.selected.as_ref() == Some(&(c.ulid.clone(), p.name.clone()));
        let syncing = self.erd.syncing.contains(&folder);
        let queued = !syncing && self.erd.queued.contains(&folder);
        let starred = p.starred == Some(true);
        let (ulid, name, toggle) = (c.ulid.clone(), p.name.clone(), path.clone());
        let caret_path = path.clone();
        let mut out = vec![
            Self::erd_row(format!("erd-{path}"), 1, selected, k)
                .text_color(k.text)
                .font_weight(FontWeight::SEMIBOLD)
                .when(current && !selected, |d| {
                    d.shadow(vec![Tokens::ring(k.border_strong, 1.)])
                })
                .child(
                    div()
                        .id(SharedString::from(format!("erd-caret-{path}")))
                        .child(
                            icon(
                                if open {
                                    "chevron-down"
                                } else {
                                    "chevron-right"
                                },
                                13.,
                            )
                            .text_color(k.text_3),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.erd.expanded.remove(&caret_path) {
                                this.erd.expanded.insert(caret_path.clone());
                            }
                            cx.stop_propagation();
                            cx.notify();
                        })),
                )
                .child(icon(if syncing { "refresh" } else { "table" }, 14.).text_color(k.text_2))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(crate::note_find::label(
                            &p.name,
                            view.query.as_deref(),
                            current,
                            k,
                        )),
                )
                .when(!p.label.is_empty(), |d| {
                    d.child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(k.text_3)
                            .child(p.label.clone()),
                    )
                })
                .child(div().flex_1())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .text_size(px(11.))
                        .font_weight(FontWeight::NORMAL)
                        .text_color(k.text_3)
                        .when(syncing, |d| d.child(t("erd.db.tree.syncing")))
                        .when(queued, |d| d.child(t("erd.db.tree.queued")))
                        .when(!syncing && !queued, |d| {
                            d.when(starred, |d| {
                                d.child(icon("star-fill", 11.).text_color(k.text_2))
                            })
                            .children(snap.as_ref().map(|s| s.tables.len().to_string()))
                        }),
                )
                .child({
                    let (id, c2, schema) = (c.id, c.clone(), p.name.clone());
                    let schema2 = schema.clone();
                    let star_key = if starred {
                        "erd.db.tree.unstar"
                    } else {
                        "erd.db.tree.star"
                    };
                    crate::note_names::row_actions(k)
                        .child(
                            crate::note_names::row_act(
                                format!("erd-star-{path}"),
                                if starred { "star-fill" } else { "star" },
                                false,
                                k,
                            )
                            .tooltip(tip(t(star_key)))
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.erd_toggle_star(id, &schema);
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            crate::note_names::row_act(
                                format!("erd-sync-{path}"),
                                "refresh",
                                false,
                                k,
                            )
                            .tooltip(tip(t("erd.db.tree.syncHere")))
                            .when(syncing, |d| d.opacity(0.45))
                            .when(!syncing, |d| {
                                d.on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    if let Some(task) = this.erd_sync_one(&c2, &schema2, cx) {
                                        task.detach();
                                    }
                                    cx.notify();
                                }))
                            }),
                        )
                })
                // 스키마는 누르면 ERD, 펼침은 셰브론이 맡는다(처음 누를 땐 함께 펼친다)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_erd_schema(&ulid, &name, cx);
                    this.erd.expanded.insert(toggle.clone());
                    cx.notify();
                }))
                .into_any_element(),
        ];
        if !open {
            return out;
        }
        let Some(snap) = snap else {
            return out;
        };
        for tb in sorted_tables(&snap) {
            let leaf = format!("{path}/{}", tb.name);
            if !view.keeps(&leaf) {
                continue;
            }
            let current = view.current.as_deref() == Some(leaf.as_str());
            let (ulid, name) = (c.ulid.clone(), p.name.clone());
            out.push(
                Self::erd_row(format!("erd-{leaf}"), 2, false, k)
                    .text_color(k.text_2)
                    .when(current, |d| {
                        d.shadow(vec![Tokens::ring(k.border_strong, 1.)])
                    })
                    .child(div().w(px(13.)))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(crate::note_find::label(
                                &tb.name,
                                view.query.as_deref(),
                                current,
                                k,
                            )),
                    )
                    .when(!tb.comment.is_empty(), |d| {
                        d.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(11.5))
                                .text_color(k.text_3)
                                .child(tb.comment.clone()),
                        )
                    })
                    .on_click({
                        let table = tb.name.clone();
                        cx.listener(move |this, _, _, cx| {
                            this.erd_reveal_table(&ulid, &name, &table, cx);
                            cx.notify();
                        })
                    })
                    .into_any_element(),
            );
        }
        out
    }

    fn erd_detail(&self, k: &Tokens, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let Some((conn, pref)) = self.erd_current() else {
            let none = self.erd.connections.as_ref().is_some_and(|l| l.is_empty());
            return div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.))
                .child(icon("database", 30.).text_color(k.text_3))
                .child(
                    div()
                        .text_size(px(17.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(k.text)
                        .child(t("erd.title")),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(k.text_3)
                        .child(t(if none {
                            "erd.empty.noConnection"
                        } else {
                            "erd.empty.pick"
                        })),
                )
                .when(none, |d| {
                    d.child(
                        crate::day_card::primary_button("erd-connect-detail", k, false)
                            .child(icon("database", 14.).text_color(k.surface))
                            .child(t("erd.empty.connect"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_conn_modal(None, window, cx);
                                cx.notify();
                            })),
                    )
                })
                .into_any_element();
        };
        let folder = schema_folder(conn, &pref.name);
        let snap = self.erd.snaps.get(&folder).cloned().flatten();
        let chip = |text: String, filled: bool| {
            div()
                .px(px(7.))
                .py(px(1.))
                .rounded(px(4.))
                .border_1()
                .border_color(k.border_strong)
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .when(filled, |d| d.bg(k.text).text_color(k.surface))
                .when(!filled, |d| d.text_color(k.text_2))
                .child(text)
        };
        let head = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(16.))
            .py(px(10.))
            .border_b_1()
            .border_color(k.glass_edge)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .text_size(px(12.))
                            .text_color(k.text_3)
                            .child(icon("database", 12.).text_color(k.text_3))
                            .child(conn.name.clone()),
                    )
                    .child(
                        div()
                            .tag("erd-title")
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(15.))
                            .font_weight(FontWeight::EXTRA_BOLD)
                            .text_color(k.text)
                            .child(pref.name.clone())
                            .when(!pref.label.is_empty(), |d| {
                                d.child(chip(pref.label.clone(), false))
                            })
                            .child(chip(env_label(&conn.env), conn.env == "prod")),
                    ),
            )
            .child(div().flex_1())
            .child(
                crate::note_ai_view::segmented(k)
                    .mr(px(4.))
                    .child(
                        crate::note_ai_view::seg_tab(
                            "erd-mode-erd",
                            t("erd.mode.erd"),
                            !self.erd.overview,
                            k,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.erd.overview = false;
                            cx.notify();
                        })),
                    )
                    .child(
                        crate::note_ai_view::seg_tab(
                            "erd-mode-overview",
                            t("erd.mode.overview"),
                            self.erd.overview,
                            k,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.erd_show_overview(window, cx);
                            cx.notify();
                        })),
                    ),
            )
            .child({
                // 동기화 — 도는 중, 마지막 동기화 시각, 처음
                let syncing = self.erd.syncing.contains(&folder);
                let label = if syncing {
                    t("erd.db.syncingShort")
                } else if let Some(s) = &snap {
                    t_with(
                        "erd.db.syncAgo",
                        &[(
                            "ago",
                            &crate::notes_view::time_ago(s.synced_at, self.db.now()),
                        )],
                    )
                } else {
                    t("erd.db.sync")
                };
                let (c, p) = (conn.clone(), pref.name.clone());
                crate::day_card::button("erd-sync", k, false, syncing)
                    .child(icon("refresh", 14.).text_color(k.text_2))
                    .child(label)
                    .when(!syncing, |d| {
                        d.on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(task) = this.erd_sync_one(&c, &p, cx) {
                                task.detach();
                            }
                            cx.notify();
                        }))
                    })
            });
        let syncing = self.erd.syncing.contains(&folder);
        let body: AnyElement = match &snap {
            _ if self.erd.overview => self.erd_overview(snap.as_deref(), k, cx),
            Some(_) => self.erd_canvas(k, window, cx).into_any_element(),
            None => div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .when(syncing, |d| {
                    d.child(crate::spinner::spinner(k, cx.reduce_motion()))
                })
                .when(!syncing, |d| {
                    d.child(icon("table", 22.).text_color(k.text_3))
                })
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(k.text_3)
                        .child(t(if syncing {
                            "erd.db.syncingShort"
                        } else {
                            "erd.noSnapshot"
                        })),
                )
                .into_any_element(),
        };
        // 바뀐 것 띠 — 동기화가 구조 변화를 찾았을 때. 닫으면 사라진다
        let banner = self.erd.diffs.get(&folder).map(|d| {
            let f = folder.clone();
            div()
                .tag("erd-diff")
                .flex()
                .items_center()
                .gap(px(10.))
                .px(px(14.))
                .py(px(8.))
                .bg(k.surface_2)
                .border_b_1()
                .border_color(k.border)
                .text_size(px(12.5))
                .text_color(k.text_2)
                .child(icon("database", 14.).text_color(k.text_2))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(k.text)
                        .child(t("erd.db.banner.changed")),
                )
                .child(t_with(
                    "erd.db.banner.detail",
                    &[
                        ("ta", &d.tables_added.len().to_string()),
                        ("tr", &d.tables_removed.len().to_string()),
                        ("ca", &d.columns_added.len().to_string()),
                        ("cr", &d.columns_removed.len().to_string()),
                        (
                            "cc",
                            &(d.columns_changed.len() + d.constraint_tables.len()).to_string(),
                        ),
                    ],
                ))
                .child(div().flex_1())
                .child(
                    crate::day_card::button("erd-diff-close", k, false, false)
                        .child(t("common.close"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.erd.diffs.remove(&f);
                            cx.notify();
                        })),
                )
        });
        let meta = div()
            .tag("erd-meta")
            .px(px(16.))
            .py(px(8.))
            .border_t_1()
            .border_color(k.glass_edge)
            .text_size(px(11.5))
            .text_color(k.text_3)
            .child(format!(
                "vault/erd/{folder}{}",
                snap.as_ref()
                    .map(|s| format!(
                        " {}",
                        t_with(
                            "erd.db.overview.snapshot",
                            &[("time", &header_time(s.synced_at)), ("server", &s.server)]
                        )
                    ))
                    .unwrap_or_default()
            ));
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(head)
            .when_some(self.erd.error.clone(), |d, e| {
                d.child(
                    crate::note_ai_view::note_box(crate::note_ai_view::Note::Error, k)
                        .tag("erd-error")
                        .mx(px(16.))
                        .mt(px(10.))
                        .child(e),
                )
            })
            .when_some(self.erd.canvas.error.clone(), |d, e| {
                d.child(
                    crate::note_ai_view::note_box(crate::note_ai_view::Note::Error, k)
                        .mx(px(16.))
                        .mt(px(10.))
                        .child(t_with("erd.compileError", &[("msg", &e)])),
                )
            })
            .children(banner)
            .child(body)
            .child(meta)
            .into_any_element()
    }

    /// 캔버스(.erd-stage) — 그림 층 둘, 손이 올라간 머리글/행의 옅은 칠, 오른쪽 아래 확대, 왼쪽 아래 배경,
    /// 오른쪽 표 패널, 컬럼 정보창
    fn erd_canvas(&self, k: &Tokens, window: &Window, cx: &mut Context<Self>) -> Div {
        let c = &self.erd.canvas;
        let (viewport, dpr) = (c.viewport.clone(), c.dpr.clone());
        let bg = c
            .background
            .as_deref()
            .and_then(|h| u32::from_str_radix(h.trim_start_matches('#'), 16).ok())
            .map(|v| rgb(v).into())
            .unwrap_or(k.surface);
        let (sx, sy, ss) = c.shown();
        let over_card = matches!(c.over, Some(Over::Head(_) | Over::Row(..) | Over::Body(_)));
        let mut stage = div()
            .id("erd-canvas")
            .tag("erd-canvas")
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(bg)
            .cursor(if c.grab.is_some_and(|g| g.2) {
                CursorStyle::ClosedHand
            } else if over_card {
                CursorStyle::PointingHand
            } else {
                CursorStyle::OpenHand
            })
            .child(
                canvas(
                    move |b, window, _| {
                        if viewport.get() != b {
                            viewport.set(b);
                            window.request_animation_frame();
                        }
                        dpr.set(window.scale_factor());
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            );
        if c.tree.is_some() && !c.fit_pending {
            stage = stage.children(c.raster.layers(c.size, (sx, sy), ss));
            // 머리글과 행은 손이 올라가면 옅게(.rz-hit 0.07) — 고른 행(초점의 강조색)은 그대로
            if let (Some(d), Some(over)) = (&c.drawn, c.over)
                && c.grab.is_none()
            {
                let ink = k.text.opacity(0.07);
                let rect = |x: f64, y: f64, w: f64, h: f64| {
                    div()
                        .absolute()
                        .left(px(sx + x as f32 * ss))
                        .top(px(sy + y as f32 * ss))
                        .w(px(w as f32 * ss))
                        .h(px(h as f32 * ss))
                        .bg(ink)
                };
                match over {
                    Over::Head(i) => {
                        let card = &d.cards[i];
                        let r = (CARD_RADIUS as f32 * ss).max(0.);
                        let bare = card.rows.is_empty() && card.h <= card.head();
                        let mut head = rect(card.x, card.y, card.w, card.head())
                            .rounded_tl(px(r))
                            .rounded_tr(px(r));
                        if bare {
                            head = head.rounded_bl(px(r)).rounded_br(px(r));
                        }
                        stage = stage.child(head);
                    }
                    Over::Row(i, j) => {
                        let card = &d.cards[i];
                        let row = &card.rows[j];
                        let on = c.popped.is_some()
                            && c.focus.as_ref().is_some_and(|(t, col)| {
                                col.is_some()
                                    && d.focus(t, col.as_deref()).is_some_and(|f| {
                                        f.rows.contains(&(card.table.clone(), row.column.clone()))
                                    })
                            });
                        if !on {
                            stage = stage.child(rect(card.x + 1., row.y, card.w - 2., row.h));
                        }
                    }
                    Over::Body(_) => {}
                }
            }
        }
        let stage = stage
            .on_scroll(cx.listener(|this, e: &ScrollEvent, _, cx| {
                let c = &mut this.erd.canvas;
                c.anim = None;
                c.wheel(e);
                cx.stop_propagation();
                cx.notify();
            }))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                let c = &mut this.erd.canvas;
                let o = c.viewport.get().origin;
                let p = (f32::from(e.position.x - o.x), f32::from(e.position.y - o.y));
                let over = if c.grab.is_some() || e.pressed_button.is_some() {
                    None
                } else {
                    c.over_at(p)
                };
                if over != c.over {
                    c.over = over;
                    cx.notify();
                }
            }))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if !hovered && this.erd.canvas.over.take().is_some() {
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    this.focus.focus(window, cx);
                    let c = &mut this.erd.canvas;
                    c.press = Some((e.position, (c.x, c.y)));
                }),
            )
            .on_drag(ErdGrab, {
                let this = cx.entity().downgrade();
                move |_, _, _, cx: &mut App| {
                    let _ = this.update(cx, |s, _| {
                        let c = &mut s.erd.canvas;
                        c.grab = c.press.take().map(|(p, xy)| (p, xy, false));
                    });
                    cx.new(|_| ErdDrag)
                }
            })
            .on_click(cx.listener(|this, e: &ClickEvent, _, cx| {
                // 4px 안에서 놓았으면 누르기 — 더블클릭은 누르기 둘 뒤에 맞춤(움직임 붙여)
                let tap = match e {
                    ClickEvent::Mouse(m) => {
                        let d = m.up.position - m.down.position;
                        f32::from(d.x).hypot(f32::from(d.y)) < DRAG_THRESHOLD
                    }
                    _ => false,
                };
                if !tap {
                    return;
                }
                this.erd_tap(e.position(), cx);
                if e.click_count() == 2 {
                    this.erd.canvas.fit(true);
                }
                cx.notify();
            }));

        let float = || {
            div()
                .absolute()
                .flex()
                .items_center()
                .gap(px(2.))
                .p(px(3.))
                .rounded(px(9.))
                .border_1()
                .border_color(k.glass_edge)
                .bg(k.surface.opacity(0.96))
                .child(glass_layer(k, 9.))
                .shadow(k.glass_shadow(false))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        let btn = |id: &str, on: bool| {
            div()
                .id(SharedString::from(id.to_string()))
                .tag(id)
                .min_w(px(28.))
                .h(px(28.))
                .px(px(8.))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .rounded(px(4.))
                .text_size(px(11.5))
                .font_weight(FontWeight::SEMIBOLD)
                .whitespace_nowrap()
                .cursor_pointer()
                .when(on, |d| d.bg(k.text).text_color(k.surface))
                .when(!on, |d| {
                    let (t1, hov) = (k.text, k.text_mix(0.07));
                    d.text_color(k.text_2)
                        .hover(move |s| s.bg(hov).text_color(t1))
                })
        };
        // 패널이 열리면 오른쪽 아래 조작은 패널 왼쪽으로 비켜서고, 남은 폭이 좁으면 배경 고르기는 견본만, 더 좁으면 숨는다
        let covered = c.covered();
        let free = c.view().0 - covered;
        let sep = || div().w(px(1.)).h(px(16.)).mx(px(3.)).bg(k.border_strong);
        let grid = self.erd.view.grid;
        let zoom = float()
            .right(px(covered + 10.))
            .bottom(px(10.))
            .child(
                btn("erd-fit", false)
                    .tooltip(tip(t("erd.canvas.fit")))
                    .child(icon("fit-screen", 16.).text_color(k.text_2))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.erd.canvas.fit(true);
                        cx.notify();
                    })),
            )
            .child(
                btn("erd-zoom-out", false)
                    .tooltip(tip(t("erd.canvas.zoomOut")))
                    .child(icon("zoom-out", 16.).text_color(k.text_2))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.erd.canvas.zoom_by(0.8, None, true);
                        cx.notify();
                    })),
            )
            .child(
                btn("erd-pct", false)
                    .min_w(px(48.))
                    .tooltip(tip(t("erd.canvas.actualSize")))
                    .child(format!("{}%", c.pct()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.erd.canvas.actual_size();
                        cx.notify();
                    })),
            )
            .child(
                btn("erd-zoom-in", false)
                    .tooltip(tip(t("erd.canvas.zoomIn")))
                    .child(icon("zoom-in", 16.).text_color(k.text_2))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.erd.canvas.zoom_by(1.25, None, true);
                        cx.notify();
                    })),
            )
            .child(sep())
            .child(
                btn("erd-grid", grid)
                    .tooltip(tip(t("erd.canvas.grid")))
                    .child(icon("grid", 15.).text_color(if grid { k.surface } else { k.text_2 }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_erd_view(|v| v.grid = !v.grid, cx);
                        cx.notify();
                    })),
            );
        let cur = self.erd.view.stage.clone();
        let labels = free >= 560.;
        let stages = float()
            .left(px(10.))
            .bottom(px(10.))
            .children(STAGES.map(|s| {
                btn(&format!("erd-stage-{s}"), cur == s)
                    .tooltip(tip(t(&format!("erd.canvas.stage.{s}"))))
                    .children(stage_swatch(s).map(|i| img(i).w(px(14.)).h(px(10.)).flex_none()))
                    .when(labels, |d| d.child(t(&format!("erd.canvas.stage.{s}"))))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_erd_stage(s);
                        cx.notify();
                    }))
            }));
        let busy = c.pending.is_some() || (c.tree.is_none() && c.error.is_none());
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(stage)
            .child(self.erd_view_float(k, cx))
            .child(zoom)
            .when(free >= 350., |d| d.child(stages))
            .children(self.erd_find_bar(covered, k, cx))
            .when(busy, |d| {
                d.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .tag("erd-status")
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .px(px(14.))
                                .py(px(10.))
                                .border_1()
                                .border_color(k.glass_edge)
                                .rounded(px(8.))
                                .bg(k.surface.opacity(0.96))
                                .child(glass_layer(k, 8.))
                                .shadow(k.glass_shadow(false))
                                .text_size(px(12.5))
                                .text_color(k.text_2)
                                .child(crate::spinner::spinner(k, cx.reduce_motion()))
                                .child(t("erd.canvas.laying")),
                        ),
                )
            })
            .children(self.erd_pop(k, window, cx))
            .children(self.erd_view_panel(k, cx))
            .children(self.erd_panel(covered, k, window, cx))
    }

    /// 좌상단 [보기] — 지금 보기(행, 방향, 뺀 항목 수)를 요약해 보이고, 누르면 판을 연다
    fn erd_view_float(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let v = &self.erd.view;
        let open = self.erd.canvas.view_open;
        let shown = CARD_PARTS.len() - v.hide.len();
        let summary = [
            Some(t(columns_key(v.columns))),
            Some(t(direction_key(v.direction))),
            (shown < CARD_PARTS.len()).then(|| {
                t_with(
                    "erd.canvas.partsCount",
                    &[
                        ("n", &shown.to_string()),
                        ("total", &CARD_PARTS.len().to_string()),
                    ],
                )
            }),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
        let bounds = self.erd.canvas.view_bounds.clone();
        let (hov, t1) = (k.text_mix(0.07), k.text);
        div()
            .absolute()
            .top(px(10.))
            .left(px(10.))
            .p(px(3.))
            .rounded(px(9.))
            .border_1()
            .border_color(k.glass_edge)
            .bg(k.surface.opacity(0.96))
            .child(glass_layer(k, 9.))
            .shadow(k.glass_shadow(false))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .child(
                div()
                    .id("erd-view")
                    .tag("erd-view")
                    .h(px(28.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(4.))
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .when(open, |d| d.bg(k.text).text_color(k.surface))
                    .when(!open, |d| {
                        d.text_color(k.text_2)
                            .hover(move |s| s.bg(hov).text_color(t1))
                    })
                    .child(t("erd.canvas.view"))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .when(open, |d| d.opacity(0.6))
                            .when(!open, |d| d.text_color(k.text_3))
                            .child(summary),
                    )
                    .child(icon("chevron-down", 14.).text_color(if open {
                        k.surface
                    } else {
                        k.text_2
                    }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        let c = &mut this.erd.canvas;
                        c.view_open = !c.view_open;
                        cx.notify();
                    })),
            )
    }

    /// [보기] 판 — 행, 항목(resin 의 견본 행), 그리기. 드롭다운이라 바깥을 누르면 닫힌다(DESIGN §8 의 예외)
    fn erd_view_panel(&self, k: &Tokens, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let c = &self.erd.canvas;
        if !c.view_open {
            return None;
        }
        let v = self.erd.view.clone();
        let ink = crate::erd_details::Ink::of(k);
        let (hov, t1) = (k.text_mix(0.07), k.text);
        let pill = |id: String, on: bool| {
            div()
                .id(SharedString::from(id.clone()))
                .tag(&id)
                .min_w(px(28.))
                .h(px(28.))
                .px(px(8.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .text_size(px(11.5))
                .font_weight(FontWeight::SEMIBOLD)
                .whitespace_nowrap()
                .cursor_pointer()
                .when(on, |d| d.bg(k.text).text_color(k.surface))
                .when(!on, |d| {
                    d.text_color(k.text_2)
                        .hover(move |s| s.bg(hov).text_color(t1))
                })
        };
        let title = |label: String, note: Option<String>| {
            div()
                .flex()
                .justify_between()
                .items_baseline()
                .gap(px(16.))
                .pt(px(12.))
                .px(px(2.))
                .pb(px(6.))
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(k.text_3)
                .child(label)
                .children(note.map(|n| div().font_weight(FontWeight::NORMAL).child(n)))
        };
        let sep = || div().w(px(1.)).h(px(16.)).mx(px(3.)).bg(k.border_strong);
        let rows = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(2.))
            .children(
                [
                    (Columns::All, "all"),
                    (Columns::Keys, "keys"),
                    (Columns::None, "none"),
                ]
                .map(|(col, n)| {
                    pill(format!("erd-view-rows-{n}"), v.columns == col)
                        .tooltip(tip(t(&format!("erd.canvas.columns.{n}Hint"))))
                        .child(t(&format!("erd.canvas.columns.{n}")))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.set_erd_view(|v| v.columns = col, cx);
                            cx.notify();
                        }))
                }),
            );
        // 견본 행 — 카드의 순서(키, 이름, 나머지). 이름은 늘 남아 단추가 아니다
        let locked = v.columns == Columns::None;
        let part = |p: CardPart| {
            let hidden = v.hide.contains(&p);
            let (cap, val): (&str, Div) = match p {
                CardPart::Keys => (
                    "Key",
                    div()
                        .font_family(crate::markdown::MONO)
                        .text_size(px(9.5))
                        .font_weight(FontWeight::BOLD)
                        .text_color(ink.fk)
                        .child("FK"),
                ),
                CardPart::Tags => (
                    "Tags",
                    div()
                        .font_family(crate::markdown::MONO)
                        .text_size(px(9.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(ink.faint)
                        .child("IX"),
                ),
                CardPart::Descriptions => (
                    "Description",
                    div()
                        .text_size(px(12.))
                        .text_color(ink.muted)
                        .child("Customer"),
                ),
                CardPart::Types => (
                    "Type",
                    div()
                        .font_family(crate::markdown::MONO)
                        .text_size(px(11.5))
                        .text_color(ink.muted)
                        .child("bigint"),
                ),
                CardPart::Nulls => (
                    "Null",
                    div()
                        .px(px(5.))
                        .border_1()
                        .border_color(ink.rule)
                        .rounded(px(3.))
                        .bg(ink.code)
                        .font_family(crate::markdown::MONO)
                        .text_size(px(9.))
                        .font_weight(FontWeight::MEDIUM)
                        .line_height(px(16.))
                        .text_color(ink.muted)
                        .child("NULL"),
                ),
            };
            let what = match p {
                CardPart::Keys => "the key labels",
                CardPart::Tags => "the tags",
                CardPart::Descriptions => "the descriptions",
                CardPart::Types => "the types",
                CardPart::Nulls => "NULL",
            };
            let name = serde_json::to_value(p)
                .ok()
                .and_then(|x| x.as_str().map(str::to_string))
                .unwrap_or_default();
            let active = ink.active_line;
            div()
                .id(SharedString::from(format!("erd-part-{name}")))
                .tag(&format!("erd-part-{name}"))
                .flex()
                .flex_col()
                .justify_between()
                .items_start()
                .gap(px(6.))
                .px(px(8.))
                .pt(px(6.))
                .pb(px(7.))
                .rounded(px(6.))
                .when(!locked, |d| d.cursor_pointer().hover(move |s| s.bg(active)))
                .tooltip(tip(format!(
                    "{} {what}",
                    if hidden { "Show" } else { "Leave out" }
                )))
                .child(div().text_size(px(10.)).text_color(ink.faint).child(cap))
                .child(val.line_height(px(18.)).when(hidden, |d| d.opacity(0.2)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.erd.view.columns == Columns::None {
                        return;
                    }
                    this.set_erd_view(
                        |v| {
                            v.hide = CARD_PARTS
                                .iter()
                                .copied()
                                .filter(|x| (*x == p) != v.hide.contains(x))
                                .collect()
                        },
                        cx,
                    );
                    cx.notify();
                }))
        };
        let name = div()
            .flex()
            .flex_col()
            .justify_between()
            .items_start()
            .gap(px(6.))
            .px(px(8.))
            .pt(px(6.))
            .pb(px(7.))
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(ink.faint)
                    .child("Column"),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .line_height(px(18.))
                    .text_color(ink.ink)
                    .child("customer_id"),
            );
        let parts = div()
            .tag("erd-view-parts")
            .flex()
            .items_stretch()
            .p(px(4.))
            .border_1()
            .border_color(ink.rule)
            .rounded(px(8.))
            .when(locked, |d| d.opacity(0.45))
            .child(part(CardPart::Keys))
            .child(name)
            .children(
                CARD_PARTS
                    .iter()
                    .copied()
                    .filter(|p| *p != CardPart::Keys)
                    .map(part),
            );
        let dirs = [(Direction::Down, "down"), (Direction::Right, "right")].map(|(d, n)| {
            pill(format!("erd-view-dir-{n}"), v.direction == d)
                .tooltip(tip(t(&format!("erd.canvas.direction.{n}Hint"))))
                .child(t(&format!("erd.canvas.direction.{n}")))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_erd_view(|v| v.direction = d, cx);
                    cx.notify();
                }))
        });
        let edges = [(Edges::Angular, "angular"), (Edges::Curved, "curved")].map(|(e, n)| {
            pill(format!("erd-view-edges-{n}"), v.edges == e)
                .child(t(&format!("erd.canvas.edges.{n}")))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_erd_view(|v| v.edges = e, cx);
                    cx.notify();
                }))
        });
        let audit = pill("erd-view-audit".into(), v.audit == AuditView::Expand)
            .tooltip(tip(t("erd.canvas.auditHint")))
            .child(t("erd.canvas.audit"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.set_erd_view(
                    |v| {
                        v.audit = if v.audit == AuditView::Expand {
                            AuditView::Collapse
                        } else {
                            AuditView::Expand
                        }
                    },
                    cx,
                );
                cx.notify();
            }));
        let bounds = c.view_bounds.clone();
        Some(
            div()
                .id("erd-view-panel")
                .tag("erd-view-panel")
                .absolute()
                .top(px(48.))
                .left(px(10.))
                .max_w(px((c.view().0 - 20.).max(0.)))
                .pt(px(4.))
                .px(px(12.))
                .pb(px(12.))
                .rounded(px(6.))
                .border_1()
                .border_color(k.glass_edge)
                .bg(k.surface.opacity(0.96))
                .child(glass_layer(k, 6.))
                .shadow(k.glass_shadow(false))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down_out(cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    // [보기] 단추는 스스로 여닫는다
                    if bounds.get().contains(&e.position) {
                        return;
                    }
                    this.erd.canvas.view_open = false;
                    cx.notify();
                }))
                .child(title(t("erd.canvas.rows"), None))
                .child(rows)
                .child(title(
                    t("erd.canvas.parts"),
                    Some(t(if locked {
                        "erd.canvas.partsLocked"
                    } else {
                        "erd.canvas.partsHint"
                    })),
                ))
                .child(parts)
                .child(title(t("erd.canvas.drawing"), None))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(2.))
                        .children(dirs)
                        .child(sep())
                        .children(edges)
                        .child(sep())
                        .child(audit),
                ),
        )
    }

    /// ⌘F 찾기 바(.page-find.erd-find) — 오른쪽 위, 패널이 열리면 패널 왼쪽으로. [보기] 와 부딪히면 한 줄 내려간다
    fn erd_find_bar(&self, covered: f32, k: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let c = &self.erd.canvas;
        let f = c.find.as_ref()?;
        let count = f.hits.len();
        let label = if count > 0 {
            format!("{}/{count}", f.idx + 1)
        } else if !f.query.trim().is_empty() {
            "0".into()
        } else {
            String::new()
        };
        let o = c.viewport.get().origin.x;
        let vb = c.view_bounds.get();
        let tl_right = f32::from(vb.origin.x + vb.size.width - o);
        let free = c.view().0 - covered;
        let low = tl_right + 8. > free - 10. - c.find_w.get();
        let nav = |id: &'static str, name: &'static str, enabled: bool| {
            let (bg, fg) = (k.surface_2, k.text_2);
            div()
                .id(id)
                .tag(id)
                .group(id)
                .flex()
                .p(px(3.))
                .rounded(px(5.))
                .when(!enabled, |d| d.opacity(0.5))
                .when(enabled, |d| d.cursor_pointer().hover(move |s| s.bg(bg)))
                .child(
                    icon(name, 14.)
                        .text_color(k.text_3)
                        .group_hover(id, move |s| s.text_color(fg)),
                )
        };
        let w = c.find_w.clone();
        Some(
            div()
                .absolute()
                .top(px(if low { 54. } else { 10. }))
                .right(px(covered + 10.))
                .max_w(px((free - 20.).max(0.)))
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(8.))
                .py(px(5.))
                .rounded(px(9.))
                .border_1()
                .border_color(k.glass_edge)
                .bg(k.surface.opacity(0.96))
                .child(glass_layer(k, 9.))
                .shadow(k.glass_shadow(false))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                    // 정보창과 패널을 닫는 Esc 까지 가지 않게
                    if e.keystroke.key == "escape" {
                        this.erd_close_find(cx);
                        cx.stop_propagation();
                        cx.notify();
                    }
                }))
                .child(
                    canvas(
                        move |b, _, _| w.set(f32::from(b.size.width)),
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                )
                .child(icon("search", 14.).text_color(k.text_3))
                .child(
                    div()
                        .tag("erd-find-input")
                        .w(px(200.))
                        .min_w_0()
                        .flex_shrink(1.)
                        .text_size(px(13.))
                        .child(Input::new(&f.input).appearance(false).px_0()),
                )
                .child(
                    div()
                        .tag("erd-find-count")
                        .min_w(px(34.))
                        .flex()
                        .justify_end()
                        .text_size(px(11.5))
                        .text_color(k.text_3)
                        .child(label),
                )
                .child(
                    nav("erd-find-prev", "chevron-left", count > 0)
                        .tooltip(tip(t("common.find.prev")))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.erd_find_step(-1, true, cx);
                            cx.notify();
                        })),
                )
                .child(
                    nav("erd-find-next", "chevron-right", count > 0)
                        .tooltip(tip(t("common.find.next")))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.erd_find_step(1, true, cx);
                            cx.notify();
                        })),
                )
                .child(
                    nav("erd-find-close", "x", true)
                        .tooltip(tip(t("common.close")))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.erd_close_find(cx);
                            cx.notify();
                        })),
                ),
        )
    }

    /// 컬럼 정보창 — 그 행 아래(자리가 없으면 위)에 붙는 작은 카드. 행이 안 보이면 숨는다
    fn erd_pop(&self, k: &Tokens, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let c = &self.erd.canvas;
        let (table, column) = c.popped.as_ref()?;
        let model = c.model.as_ref()?;
        let el = column_details(model, table, column)?;
        let (vw, vh) = c.view();
        let covered = c.covered();
        let (w, h) = c.pop_size.get();
        let (x0, y0, x1, y1) = c.row_rect(table, column)?;
        let left = x0.min(vw - covered - w - 8.).max(8.);
        let below = y1 + 6.;
        let above = y0 - h - 6.;
        let top = if below + h <= vh - 8. || above < 8. {
            below
        } else {
            above
        };
        let top = top.min(vh - h - 8.).max(8.);
        let seen = y1 > 0. && y0 < vh && x1 > 0. && x0 < vw - covered;
        let measured = w > 0.;
        let size = c.pop_size.clone();
        let ctx = crate::erd_details::Ctx {
            ink: crate::erd_details::Ink::of(k),
            on: None,
            id: "erd-pop",
        };
        let max_w = 320_f32.min(vw - 16.);
        Some(
            div()
                .id("erd-pop")
                .tag("erd-pop")
                .absolute()
                .left(px(left))
                .top(px(top))
                .max_w(px(max_w))
                .max_h(px(vh - 16.))
                .overflow_y_scroll()
                .px(px(12.))
                .py(px(10.))
                .border_1()
                .border_color(k.border_strong)
                .rounded(px(8.))
                .bg(k.surface)
                .shadow(k.glass_shadow(false))
                .text_size(px(12.5))
                .line_height(relative(1.45))
                .text_color(k.text)
                .occlude()
                .when(!seen || !measured, |d| d.invisible())
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_scroll(|_, _, cx| cx.stop_propagation())
                .child(crate::erd_details::render(
                    &el,
                    max_w - 26.,
                    &ctx,
                    window,
                    cx,
                ))
                .child(
                    canvas(
                        move |b, window, _| {
                            let s = (f32::from(b.size.width), f32::from(b.size.height));
                            if size.get() != s {
                                size.set(s);
                                window.refresh();
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                )
                .into_any_element(),
        )
    }

    /// 오른쪽 표 패널 — 머리(TABLE, 이름, X)와 resin 의 표 설명. 왼쪽 가장자리를 끌어 폭을 바꾼다
    fn erd_panel(
        &self,
        width: f32,
        k: &Tokens,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let c = &self.erd.canvas;
        let name = c.inspected.as_ref()?;
        let model = c.model.as_ref()?;
        let el = table_details(model, name)?;
        let on = c
            .popped
            .as_ref()
            .filter(|(t, _)| t == name)
            .map(|(_, col)| col.as_str());
        let ctx = crate::erd_details::Ctx {
            ink: crate::erd_details::Ink::of(k),
            on,
            id: "erd-panel",
        };
        let (hov, t1) = (k.text_mix(0.07), k.text);
        Some(
            div()
                .id("erd-panel")
                .tag("erd-panel")
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .w(px(width))
                .flex()
                .flex_col()
                .border_l_1()
                .border_color(k.border_strong)
                .bg(k.surface)
                .shadow(k.glass_shadow(false))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .id("erd-panel-resize")
                        .tag("erd-panel-resize")
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(-4.))
                        .w(px(8.))
                        .cursor_col_resize()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                                let c = &mut this.erd.canvas;
                                c.panel_grab = Some((f32::from(e.position.x), width));
                                cx.stop_propagation();
                            }),
                        )
                        .on_drag(ErdPanelGrab, |_, _, _, cx: &mut App| cx.new(|_| ErdDrag))
                        // 놓는 곳은 대개 패널 위라 뿌리의 on_drop 이 가려진다 — 창 전체의 놓기로 끝낸다
                        .when(c.panel_grab.is_some(), |d| {
                            let this = cx.entity().downgrade();
                            d.child(
                                canvas(
                                    |_, _, _| {},
                                    move |_, _, window, _| {
                                        let this = this.clone();
                                        window.on_mouse_event(
                                            move |_: &MouseUpEvent, phase, _, cx| {
                                                if phase == DispatchPhase::Capture {
                                                    let _ = this.update(cx, |s, cx| {
                                                        s.erd_panel_drop();
                                                        cx.notify();
                                                    });
                                                }
                                            },
                                        );
                                    },
                                )
                                .absolute()
                                .inset_0(),
                            )
                        }),
                )
                .child(
                    div()
                        .h(px(40.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .pl(px(14.))
                        .pr(px(6.))
                        .border_b_1()
                        .border_color(k.border)
                        .child(
                            div()
                                .text_size(px(11.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(k.text_3)
                                .child(t("erd.canvas.table").to_uppercase()),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .font_family(crate::markdown::MONO)
                                .text_size(px(12.5))
                                .text_color(k.text_2)
                                .child(name.clone()),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .id("erd-panel-close")
                                .tag("erd-panel-close")
                                .size(px(28.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.))
                                .cursor_pointer()
                                .hover(move |s| s.bg(hov).text_color(t1))
                                .child(icon("x", 15.).text_color(k.text_2))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.erd_close_panel(cx);
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .id("erd-panel-body")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .pt(px(16.))
                        .px(px(16.))
                        .pb(px(28.))
                        .text_color(k.text)
                        .child(crate::erd_details::render(
                            &el,
                            width - 32.,
                            &ctx,
                            window,
                            cx,
                        )),
                )
                .into_any_element(),
        )
    }
}

/// 떠 있는 유리의 결(--glass-fill) — 웹은 바탕 86% 에 뒤 흐림이라 뒤의 글이 안 읽힌다. GPUI 엔 흐림이 없어
/// menu.rs 처럼 바탕을 96% 로 올리고 그 위에 결을 깐다(맨 앞 자식으로)
fn glass_layer(k: &Tokens, radius: f32) -> Div {
    div()
        .absolute()
        .inset_0()
        .rounded(px(radius))
        .bg(crate::note_ai_view::glass(k.glass_fill))
}

fn columns_key(c: Columns) -> &'static str {
    match c {
        Columns::All => "erd.canvas.columns.all",
        Columns::Keys => "erd.canvas.columns.keys",
        Columns::None => "erd.canvas.columns.none",
    }
}

fn direction_key(d: Direction) -> &'static str {
    match d {
        Direction::Down => "erd.canvas.direction.down",
        Direction::Right => "erd.canvas.direction.right",
    }
}

/// 배경 견본(.erd-swatch-*) — 14x10, 모서리 2, 안쪽 회색 테. CSS 그라디언트를 같은 기하로 SVG 에 옮겨 한 번 굽는다
fn stage_swatch(stage: &str) -> Option<Arc<Image>> {
    thread_local! {
        static CACHE: std::cell::RefCell<HashMap<String, Arc<Image>>> = Default::default();
    }
    if let Some(i) = CACHE.with(|c| c.borrow().get(stage).cloned()) {
        return Some(i);
    }
    // 각도 a 의 선형 그라디언트: 중심을 지나고 길이는 |w sin a| + |h cos a|
    let line = |deg: f32| {
        let a = deg.to_radians();
        let (dx, dy) = (a.sin(), -a.cos());
        let half = (14. * a.sin().abs() + 10. * a.cos().abs()) / 2.;
        (
            7. - dx * half,
            5. - dy * half,
            7. + dx * half,
            5. + dy * half,
        )
    };
    let lin = |id: &str, deg: f32, stops: &str| {
        let (x1, y1, x2, y2) = line(deg);
        format!(
            "<linearGradient id='{id}' gradientUnits='userSpaceOnUse' x1='{x1}' y1='{y1}' x2='{x2}' y2='{y2}'>{stops}</linearGradient>"
        )
    };
    let stop = |o: f32, c: &str| format!("<stop offset='{o}' stop-color='{c}'/>");
    let (defs, fills) = match stage {
        "aurora" => (
            lin(
                "a",
                180.,
                &[
                    stop(0., "#05060c"),
                    stop(0.45, "#5a5ff0"),
                    stop(0.75, "#22c7a9"),
                    stop(1., "#05060c"),
                ]
                .concat(),
            ),
            vec!["url(#a)"],
        ),
        "silk" => (
            lin(
                "a",
                135.,
                &[
                    stop(0., "#0a0708"),
                    stop(0.4, "#5b2c7a"),
                    stop(0.75, "#d9893a"),
                    stop(1., "#f1b6c4"),
                ]
                .concat(),
            ),
            vec!["url(#a)"],
        ),
        _ => {
            // radial-gradient(circle at 35% 30%, #9fe7f2 0, transparent 35%) 위에 linear-gradient(160deg, #0e5e70, #031018)
            let (cx_, cy_) = (14_f32 * 0.35, 10_f32 * 0.3);
            let r = ((14. - cx_).powi(2) + (10. - cy_).powi(2)).sqrt();
            (
                lin(
                    "a",
                    160.,
                    &[stop(0., "#0e5e70"), stop(1., "#031018")].concat(),
                ) + &format!(
                    "<radialGradient id='b' gradientUnits='userSpaceOnUse' cx='{cx_}' cy='{cy_}' r='{r}'><stop offset='0' stop-color='#9fe7f2'/><stop offset='0.35' stop-color='#9fe7f2' stop-opacity='0'/></radialGradient>"
                ),
                vec!["url(#a)", "url(#b)"],
            )
        }
    };
    let rects: String = fills
        .iter()
        .map(|f| format!("<rect width='14' height='10' rx='2' fill='{f}'/>"))
        .collect();
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='14' height='10' viewBox='0 0 14 10'><defs>{defs}</defs>{rects}<rect x='0.5' y='0.5' width='13' height='9' rx='1.5' fill='none' stroke='rgb(128,128,128)' stroke-opacity='0.4'/></svg>"
    );
    let tree = crate::figure::parse(&svg, "#000").ok()?;
    let image = crate::figure::raster(&tree, (0., 0., 14., 10.), 4.).ok()?;
    CACHE.with(|c| c.borrow_mut().insert(stage.to_string(), image.clone()));
    Some(image)
}

/// 표 패널 폭(amber.erd.panel) — 360 보다 작거나 없으면 560
pub fn panel_width(db: &amber_gpui::db::Db) -> f32 {
    amber_gpui::prefs::get(db, PANEL_KEY)
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| *v >= PANEL_MIN)
        .unwrap_or(560.)
}

/// 지금 앱 formatHeaderTime — 'YYYY-MM-DD HH:mm'(내 시간대)
pub(crate) fn header_time(ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, point, px};

    use super::{ErdCanvas, MAX, MIN};

    fn canvas(view: (f32, f32), size: (f32, f32)) -> ErdCanvas {
        let c = ErdCanvas::default();
        c.viewport.set(Bounds::new(
            point(px(0.), px(0.)),
            gpui_kit::size(px(view.0), px(view.1)),
        ));
        ErdCanvas {
            size,
            tree: None,
            ..c
        }
    }

    #[test]
    fn fit_leaves_a_margin_and_caps_at_125() {
        let mut c = canvas((1048., 648.), (500., 300.));
        c.fit(false);
        assert_eq!(c.scale, 1.25, "작은 그림도 125% 까지만");
        let mut c = canvas((1048., 648.), (5000., 1000.));
        c.fit(false);
        assert!((c.scale - 0.2).abs() < 1e-6, "(1048-48)/5000");
        assert!((c.x - (1048. - 1000.) / 2.).abs() < 1e-3);
    }

    #[test]
    fn zoom_clamps_and_keeps_the_point() {
        let mut c = canvas((1000., 600.), (20000., 1000.));
        c.tree = Some(std::sync::Arc::new(
            crate::figure::parse(
                "<svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'/>",
                "#000",
            )
            .unwrap(),
        ));
        c.fit(false);
        let fit = c.scale;
        assert!(fit < MIN, "아주 큰 그림은 10% 아래로 맞춘다");
        c.zoom_by(0.5, None, false);
        assert_eq!(c.scale, fit, "맞춘 배율 아래로는 안 내려간다");
        c.zoom_by(1000., Some((100., 100.)), false);
        assert_eq!(c.scale, MAX);
        let before = ((100. - c.x) / c.scale, (100. - c.y) / c.scale);
        c.zoom_by(0.8, Some((100., 100.)), false);
        let after = ((100. - c.x) / c.scale, (100. - c.y) / c.scale);
        assert!((before.0 - after.0).abs() < 1e-2 && (before.1 - after.1).abs() < 1e-2);
    }
}
