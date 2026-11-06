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
use amber_gpui::resin::viewer::Drawn;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use resvg::usvg;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::Tag;
use crate::theme::Tokens;

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
    /// 칠한(칠하는 중인) 초점: 표, 컬럼
    focus: Option<(String, Option<String>)>,
    focus_task: Option<Task<()>>,
    focus_gen: u64,
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
    pub fn plain_tree(&self) -> Option<Arc<usvg::Tree>> {
        self.plain.clone()
    }

    /// 칠한 초점(시험용)
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
        let Some((x0, y0, x1, y1)) = self.row_rect(table, column) else {
            return;
        };
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
            true,
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
                    .and_then(|body| serde_json::from_str::<SchemaSnapshot>(&body).ok())
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

    pub(crate) fn open_erd_schema(&mut self, ulid: &str, schema: &str) {
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
        }
    }

    pub(crate) fn set_erd_stage(&mut self, stage: &str) {
        self.erd.view.stage = stage.to_string();
        self.erd.view.save(&self.db);
    }

    /// render 에서 — 처음이면 읽고, 그릴 키가 바뀌었으면 뒤에서 그리고, 판을 재서 맞추고 굽는다
    pub(crate) fn prepare_erd(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.erd.connections.is_none() {
            self.load_erd();
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

    /// 초점을 칠한다(None = 걷는다). 그림에 없는 표면 걷는다(createFocus). 칠한 그림은 뒤에서 다시 읽는다
    fn erd_focus(&mut self, target: Option<(String, Option<String>)>, cx: &mut Context<Self>) {
        let c = &mut self.erd.canvas;
        let Some(drawn) = c.drawn.clone() else {
            return;
        };
        let target = target.filter(|(t, _)| drawn.card(t).is_some());
        if c.focus == target {
            return;
        }
        c.focus = target.clone();
        c.focus_gen += 1;
        let generation = c.focus_gen;
        let css = target.and_then(|(t, col)| {
            let f = drawn.focus(&t, col.as_deref())?;
            Some(drawn.focus_css(&f, crate::erd_details::Ink::accent_hex(c.dark)))
        });
        let Some(css) = css else {
            c.focus_task = None;
            if let Some(p) = c.plain.clone() {
                c.tree = Some(p);
                c.raster.stale = true;
            }
            return;
        };
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
                    Err(e) => eprintln!("ERD 초점: {e}"),
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

    /// Esc — 안쪽부터: 정보창, 그다음 패널
    pub(crate) fn erd_escape(&mut self, cx: &mut Context<Self>) -> bool {
        let c = &mut self.erd.canvas;
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

    /// 트리에서 표를 골랐다 — 그림이 이 스키마의 것이면 바로, 아니면 그린 다음에(그림에 없는 표도 패널은 연다)
    fn erd_reveal_table(&mut self, ulid: &str, schema: &str, table: &str, cx: &mut Context<Self>) {
        self.open_erd_schema(ulid, schema);
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
            );
        let body: AnyElement = match &self.erd.connections {
            None => div().into_any_element(),
            Some(list) if list.is_empty() => div()
                .p(px(14.))
                .text_size(px(12.5))
                .text_color(k.text_3)
                .child(t("erd.empty.noConnection"))
                .into_any_element(),
            Some(list) => div()
                .id("erd-tree")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(px(6.))
                .pb(px(8.))
                .children(list.iter().flat_map(|c| self.erd_conn_rows(c, k, cx)))
                .into_any_element(),
        };
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
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .border_1()
                    .border_color(k.glass_edge)
                    .rounded(px(12.))
                    .bg(crate::note_ai_view::glass(k.glass_fill))
                    .shadow(k.glass_shadow(false))
                    .child(head)
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
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let path = conn_path(c);
        let open = self.erd.expanded.contains(&path);
        let dot = conn_status_dot(conn_status(c));
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
                        .child(c.name.clone()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .text_size(px(11.))
                        .font_weight(FontWeight::NORMAL)
                        .text_color(k.text_3)
                        .child(
                            div()
                                .size(px(7.))
                                .rounded_full()
                                .border_1()
                                .border_color(dot_color)
                                .when(!dot.is_empty(), |d| d.bg(dot_color)),
                        )
                        .child(env_label(&c.env)),
                )
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
        let mut prefs = enabled_schemas(c);
        // 즐겨찾기 스키마는 제 연결 안에서 맨 위로
        prefs.sort_by(|a, b| {
            (b.starred == Some(true))
                .cmp(&(a.starred == Some(true)))
                .then_with(|| cmp(&a.name, &b.name))
        });
        for p in prefs {
            out.extend(self.erd_schema_rows(c, p, k, cx));
        }
        out
    }

    fn erd_schema_rows(
        &self,
        c: &DbConnection,
        p: &DbSchemaPref,
        k: &Tokens,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let path = schema_path(c, &p.name);
        let open = self.erd.expanded.contains(&path);
        let folder = schema_folder(c, &p.name);
        let snap = self.erd.snaps.get(&folder).cloned().flatten();
        let selected = self.erd.selected.as_ref() == Some(&(c.ulid.clone(), p.name.clone()));
        let (ulid, name, toggle) = (c.ulid.clone(), p.name.clone(), path.clone());
        let caret_path = path.clone();
        let mut out = vec![
            Self::erd_row(format!("erd-{path}"), 1, selected, k)
                .text_color(k.text)
                .font_weight(FontWeight::SEMIBOLD)
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
                .child(icon("table", 14.).text_color(k.text_2))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(p.name.clone()),
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
                .when(snap.is_some() || p.starred == Some(true), |d| {
                    d.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .text_size(px(11.))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(k.text_3)
                            .when(p.starred == Some(true), |d| {
                                d.child(icon("star", 11.).text_color(k.text_3))
                            })
                            .children(snap.as_ref().map(|s| s.tables.len().to_string())),
                    )
                })
                // 스키마는 누르면 ERD, 펼침은 셰브론이 맡는다(처음 누를 땐 함께 펼친다)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_erd_schema(&ulid, &name);
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
        // 도메인 표 먼저, 감사 표는 뒤로
        let mut tables: Vec<&amber_gpui::erd_gen::SnapshotTable> = snap.tables.iter().collect();
        tables.sort_by(|a, b| {
            is_audit(&a.name)
                .cmp(&is_audit(&b.name))
                .then_with(|| cmp(&a.name, &b.name))
        });
        for tb in tables {
            let (ulid, name) = (c.ulid.clone(), p.name.clone());
            out.push(
                Self::erd_row(format!("erd-{path}/{}", tb.name), 2, false, k)
                    .text_color(k.text_2)
                    .child(div().w(px(13.)))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(tb.name.clone()),
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
            );
        let body: AnyElement = match &snap {
            Some(_) => self.erd_canvas(k, window, cx).into_any_element(),
            None => div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .child(icon("table", 22.).text_color(k.text_3))
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(k.text_3)
                        .child(t("erd.noSnapshot")),
                )
                .into_any_element(),
        };
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
            .when_some(self.erd.canvas.error.clone(), |d, e| {
                d.child(
                    crate::note_ai_view::note_box(crate::note_ai_view::Note::Error, k)
                        .mx(px(16.))
                        .mt(px(10.))
                        .child(t_with("erd.compileError", &[("msg", &e)])),
                )
            })
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
                .bg(crate::note_ai_view::glass(k.glass_fill))
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
        let zoom = float()
            .right(px(covered + 10.))
            .bottom(px(10.))
            .child(
                btn("erd-fit", false)
                    .child(icon("fit-screen", 16.).text_color(k.text_2))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.erd.canvas.fit(true);
                        cx.notify();
                    })),
            )
            .child(
                btn("erd-zoom-out", false)
                    .child(icon("zoom-out", 16.).text_color(k.text_2))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.erd.canvas.zoom_by(0.8, None, true);
                        cx.notify();
                    })),
            )
            .child(
                btn("erd-pct", false)
                    .min_w(px(48.))
                    .child(format!("{}%", c.pct()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.erd.canvas.actual_size();
                        cx.notify();
                    })),
            )
            .child(
                btn("erd-zoom-in", false)
                    .child(icon("zoom-in", 16.).text_color(k.text_2))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.erd.canvas.zoom_by(1.25, None, true);
                        cx.notify();
                    })),
            );
        let cur = self.erd.view.stage.clone();
        let labels = free >= 560.;
        let stages = float()
            .left(px(10.))
            .bottom(px(10.))
            .children(STAGES.map(|s| {
                let swatch = match s {
                    "aurora" => (0x22c7a9, 0x5a5ff0),
                    "silk" => (0xd9893a, 0x5b2c7a),
                    _ => (0x0e5e70, 0x9fe7f2),
                };
                btn(&format!("erd-stage-{s}"), cur == s)
                    .child(div().size(px(14.)).rounded_full().bg(linear_gradient(
                        135.,
                        linear_color_stop(rgb(swatch.0), 0.),
                        linear_color_stop(rgb(swatch.1), 1.),
                    )))
                    .when(labels, |d| d.child(t(&format!("erd.canvas.stage.{s}"))))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_erd_stage(s);
                        cx.notify();
                    }))
            }));
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(stage)
            .child(zoom)
            .when(free >= 350., |d| d.child(stages))
            .children(self.erd_pop(k, window, cx))
            .children(self.erd_panel(covered, k, window, cx))
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

/// 표 패널 폭(amber.erd.panel) — 360 보다 작거나 없으면 560
pub fn panel_width(db: &amber_gpui::db::Db) -> f32 {
    amber_gpui::prefs::get(db, PANEL_KEY)
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| *v >= PANEL_MIN)
        .unwrap_or(560.)
}

/// 지금 앱 formatHeaderTime — 'YYYY-MM-DD HH:mm'(내 시간대)
fn header_time(ms: i64) -> String {
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
