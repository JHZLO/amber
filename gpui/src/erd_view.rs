// ERD 탭 — 지금 앱 ErdView.tsx(연결 → 스키마 → 표 트리, 머리, 캔버스)와 resin/viewer 의 PanZoom(view.ts).
// 그림은 resin 이 WebGL2 없이 그리는 정지 SVG(stage: true, `<무대>-<어둠>` 룩) — 유리 배경은 D4g.
// 스냅샷(vault/erd/<연결>/<스키마>/schema.json) → erd_gen → resin::compile → svg::to_svg → usvg 를 뒤에서 돌리고,
// 같은 그림(키)이면 다시 그리지 않는다. 이동/확대는 resin 과 같다: 휠 = 포인터 자리, 끌기 = 이동, 더블클릭 = 맞춤.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use amber_gpui::dbconn::{
    DbConnection, DbSchemaPref, conn_status, conn_status_dot, enabled_schemas, env_label,
    pref_audit, schema_folder, snapshot_path,
};
use amber_gpui::erd_gen::SchemaSnapshot;
use amber_gpui::i18n::{t, t_with};
use amber_gpui::resin::svg::{
    AuditView, CARD_PARTS, CardPart, Columns, Direction, Edges, SvgOptions,
};
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
const STAGES: [&str; 3] = ["aurora", "silk", "caustic"];

// resin/viewer/view.ts 의 PanZoom
const MIN: f32 = 0.1;
const MAX: f32 = 4.;

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

/// 다 그린 것 — 뒤에서 만들어 넘긴다
struct Drawing {
    tree: Arc<usvg::Tree>,
    size: (f32, f32),
    background: Option<String>,
}

#[derive(Default)]
pub struct ErdCanvas {
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
    grab: Option<(Point<Pixels>, (f32, f32))>,
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

    pub fn fit(&mut self) {
        self.auto = true;
        let (vw, vh) = self.view();
        if self.size.0 <= 0. || vw <= 0. || vh <= 0. {
            self.fit_pending = true;
            return;
        }
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
    pub fn zoom_by(&mut self, factor: f32, at: Option<(f32, f32)>) {
        if self.tree.is_none() {
            return;
        }
        self.auto = false;
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
        self.zoom_by(1. / s, None);
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
        self.zoom_by((-delta * k).exp(), Some(p));
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
        self.erd.selected = Some((ulid.to_string(), schema.to_string()));
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
    pub(crate) fn prepare_erd(&mut self, cx: &mut Context<Self>) {
        if self.erd.connections.is_none() {
            self.load_erd();
        }
        let dark = Tokens::get(cx).dark;
        self.draw_erd(dark, cx);
        let c = &mut self.erd.canvas;
        let v = c.view();
        if c.tree.is_some() && v.0 > 0. && (c.fit_pending || (c.auto && c.measured != v)) {
            c.fit();
        }
        if let Some(tree) = c.tree.clone()
            && !c.fit_pending
            && c.scale > 0.
        {
            let g = crate::raster::Geometry {
                size: c.size,
                scale: c.scale,
                pan: (c.x, c.y),
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
                    let tree = crate::figure::parse(&svg.svg, "#333333")?;
                    Ok(Drawing {
                        tree: Arc::new(tree),
                        size: (svg.width as f32, svg.height as f32),
                        background: svg.background,
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
                        c.tree = Some(d.tree);
                        c.size = d.size;
                        c.background = d.background;
                        c.error = None;
                        c.raster.reset();
                        c.shown_id = drawing_id;
                        c.shown_direction = Some(direction);
                        if fit || c.scale <= 0. {
                            c.fit_pending = true;
                        }
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
        let Some((start, (x0, y0))) = c.grab else {
            return false;
        };
        c.auto = false;
        c.x = x0 + f32::from(pos.x - start.x);
        c.y = y0 + f32::from(pos.y - start.y);
        true
    }

    pub(crate) fn erd_drop(&mut self) {
        self.erd.canvas.grab = None;
    }

    pub(crate) fn erd_body(&self, body_w: f32, k: &Tokens, cx: &mut Context<Self>) -> Div {
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
                            .child(self.erd_detail(k, cx)),
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
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_erd_schema(&ulid, &name);
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        out
    }

    fn erd_detail(&self, k: &Tokens, cx: &mut Context<Self>) -> AnyElement {
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
            Some(_) => self.erd_canvas(k, cx).into_any_element(),
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

    /// 캔버스(.erd-stage) — 그림 층 둘, 오른쪽 아래 확대, 왼쪽 아래 배경
    fn erd_canvas(&self, k: &Tokens, cx: &mut Context<Self>) -> Div {
        let c = &self.erd.canvas;
        let (viewport, dpr) = (c.viewport.clone(), c.dpr.clone());
        let bg = c
            .background
            .as_deref()
            .and_then(|h| u32::from_str_radix(h.trim_start_matches('#'), 16).ok())
            .map(|v| rgb(v).into())
            .unwrap_or(k.surface);
        let mut stage = div()
            .id("erd-canvas")
            .tag("erd-canvas")
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(bg)
            .cursor(if c.grab.is_some() {
                CursorStyle::ClosedHand
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
            stage = stage.children(c.raster.layers(c.size, (c.x, c.y), c.scale));
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
                this.erd.canvas.wheel(e);
                cx.stop_propagation();
                cx.notify();
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
                        c.grab = c.press.take();
                    });
                    cx.new(|_| ErdDrag)
                }
            })
            .on_click(cx.listener(|this, e: &ClickEvent, _, cx| {
                // 더블클릭 = 맞춤(4px 넘게 움직였으면 끌기다)
                if e.click_count() == 2 {
                    this.erd.canvas.fit();
                    cx.notify();
                }
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
        let btn = |id: &'static str, on: bool| {
            div()
                .id(id)
                .tag(id)
                .min_w(px(28.))
                .h(px(28.))
                .px(px(8.))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .rounded(px(4.))
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .when(on, |d| d.bg(k.text).text_color(k.surface))
                .when(!on, |d| {
                    let (t1, hov) = (k.text, k.text_mix(0.07));
                    d.text_color(k.text_2)
                        .hover(move |s| s.bg(hov).text_color(t1))
                })
        };
        let zoom = float()
            .right(px(10.))
            .bottom(px(10.))
            .child(
                btn("erd-fit", false)
                    .child(icon("fit-screen", 16.).text_color(k.text_2))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.erd.canvas.fit();
                        cx.notify();
                    })),
            )
            .child(
                btn("erd-zoom-out", false)
                    .child(icon("zoom-out", 16.).text_color(k.text_2))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.erd.canvas.zoom_by(0.8, None);
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
                        this.erd.canvas.zoom_by(1.25, None);
                        cx.notify();
                    })),
            );
        let cur = self.erd.view.stage.clone();
        let stages = float()
            .left(px(10.))
            .bottom(px(10.))
            .children(STAGES.map(|s| {
                let swatch = match s {
                    "aurora" => (0x22c7a9, 0x5a5ff0),
                    "silk" => (0xd9893a, 0x5b2c7a),
                    _ => (0x0e5e70, 0x9fe7f2),
                };
                btn(
                    match s {
                        "aurora" => "erd-stage-aurora",
                        "silk" => "erd-stage-silk",
                        _ => "erd-stage-caustic",
                    },
                    cur == s,
                )
                .child(div().size(px(14.)).rounded_full().bg(linear_gradient(
                    135.,
                    linear_color_stop(rgb(swatch.0), 0.),
                    linear_color_stop(rgb(swatch.1), 1.),
                )))
                .child(t(&format!("erd.canvas.stage.{s}")))
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
            .child(stages)
    }
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
        c.fit();
        assert_eq!(c.scale, 1.25, "작은 그림도 125% 까지만");
        let mut c = canvas((1048., 648.), (5000., 1000.));
        c.fit();
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
        c.fit();
        let fit = c.scale;
        assert!(fit < MIN, "아주 큰 그림은 10% 아래로 맞춘다");
        c.zoom_by(0.5, None);
        assert_eq!(c.scale, fit, "맞춘 배율 아래로는 안 내려간다");
        c.zoom_by(1000., Some((100., 100.)));
        assert_eq!(c.scale, MAX);
        let before = ((100. - c.x) / c.scale, (100. - c.y) / c.scale);
        c.zoom_by(0.8, Some((100., 100.)));
        let after = ((100. - c.x) / c.scale, (100. - c.y) / c.scale);
        assert!((before.0 - after.0).abs() < 1e-2 && (before.1 - after.1).abs() < 1e-2);
    }
}
