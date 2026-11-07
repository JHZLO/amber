// 바탕화면 위젯 — 지금 앱 Widget.tsx. 늘 위에 뜨는 작은 창(320x210, 테두리 없음, 투명 바탕 안에 카드)에서
// 학습 중 개념을 하나씩 넘겨 본다: 이전/다음, 학습완료, 확신 ±, 상세 열기(메인 창의 개념 탭으로).
// 카드가 3초 머물면 노출을 한 번 센다. 메인에서 개념이 바뀌면 다음 이동 때 새로 읽는다(지금 앱의 dirty).
// 같은 프로세스라 창 사이 이벤트 대신 엔티티를 직접 부른다.

use std::time::Duration;

use amber_gpui::concepts;
use amber_gpui::db::{Clock, Db, Row};
use amber_gpui::i18n::t;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::Value as Json;

use crate::Spike;
use crate::assets::icon;
use crate::day_card::Tag;
use crate::theme::Tokens;
use crate::tip::tip;

pub struct Widget {
    db: Db,
    pub queue: Vec<Row>,
    pub idx: usize,
    ready: bool,
    /// 다른 창에서 바뀌었다 — 다음 이동 때 다시 읽는다(메인과 나눠 쥔 깃발)
    pub dirty: std::rc::Rc<std::cell::Cell<bool>>,
    main: WeakEntity<Spike>,
    main_window: AnyWindowHandle,
    focus: FocusHandle,
    seen: Option<Task<()>>,
    seen_id: Option<i64>,
}

fn id_of(r: &Row) -> i64 {
    r.get("id").and_then(Json::as_i64).unwrap_or_default()
}

impl Widget {
    pub fn new(
        db: Db,
        dirty: std::rc::Rc<std::cell::Cell<bool>>,
        main: WeakEntity<Spike>,
        main_window: AnyWindowHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let mut w = Widget {
            db,
            queue: Vec::new(),
            idx: 0,
            ready: false,
            dirty,
            main,
            main_window,
            focus,
            seen: None,
            seen_id: None,
        };
        w.snapshot(None);
        w
    }

    /// 숨기기 — 지금 앱은 창을 숨겼다가 같은 상태, 같은 자리로 다시 보인다. GPUI 는 창 하나만 숨길 수 없어서
    /// 창은 닫고 자리를 메인에 맡긴다(이 엔티티는 메인이 쥐고 있다가 다시 열 때 그대로 쓴다)
    pub(crate) fn hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let b = window.bounds();
        let _ = self.main.update(cx, |s, _| s.widget_bounds = Some(b));
        window.remove_window();
    }

    /// 학습 중 큐를 다시 읽는다 — keep 이 있으면 그 카드에 머문다
    fn snapshot(&mut self, keep: Option<i64>) {
        self.queue = concepts::learning_queue(&self.db).unwrap_or_default();
        self.ready = true;
        self.dirty.set(false);
        self.idx = match keep.and_then(|id| self.queue.iter().position(|r| id_of(r) == id)) {
            Some(i) => i,
            None if self.idx < self.queue.len() => self.idx,
            None => 0,
        };
    }

    fn current(&self) -> Option<&Row> {
        self.queue.get(self.idx)
    }

    pub(crate) fn go(&mut self, delta: i64, cx: &mut Context<Self>) {
        let cur = self.current().map(id_of);
        if self.dirty.get() {
            self.snapshot(cur);
        }
        let n = self.queue.len() as i64;
        if n == 0 {
            return;
        }
        let base = cur
            .and_then(|id| self.queue.iter().position(|r| id_of(r) == id))
            .unwrap_or(self.idx) as i64;
        self.idx = (((base + delta) % n + n) % n) as usize;
        cx.notify();
    }

    /// 메인 창에 알린다 — 목록을 다시 읽게
    fn tell_main(&self, cx: &mut Context<Self>) {
        let _ = self.main.update(cx, |s, cx| {
            s.reload_concepts();
            s.reload();
            cx.notify();
        });
    }

    pub(crate) fn complete(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.current().map(id_of) else {
            return;
        };
        let _ = concepts::set_status(&self.db, id, "learned");
        self.tell_main(cx);
        // 큐에서 빠지고 지금 자리가 다음 카드를 가리킨다
        self.snapshot(None);
        cx.notify();
    }

    pub(crate) fn conf(&mut self, delta: i64, cx: &mut Context<Self>) {
        let Some(id) = self.current().map(id_of) else {
            return;
        };
        let _ = concepts::adjust_confidence(&self.db, id, delta as f64);
        self.tell_main(cx);
        // 순서는 다음 이동부터 — 보이는 점만 바로 고친다
        self.dirty.set(true);
        if let Some(r) = self.queue.get_mut(self.idx) {
            let c = r.get("confidence").and_then(Json::as_i64).unwrap_or(1);
            r.insert("confidence".into(), Json::from((c + delta).clamp(1, 3)));
        }
        cx.notify();
    }

    fn show_main(&self, cx: &mut Context<Self>) {
        let _ = self
            .main_window
            .update(cx, |_, window, _| window.activate_window());
    }

    fn open_detail(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.current().map(id_of) else {
            return;
        };
        let main = self.main.clone();
        let _ = self.main_window.update(cx, |_, window, cx| {
            let _ = main.update(cx, |s, cx| s.go_to_concept(id, window, cx));
            window.activate_window();
        });
    }

    /// 카드가 3초 머물면 노출 한 번
    fn track_seen(&mut self, cx: &mut Context<Self>) {
        let cur = self.current().map(id_of);
        if cur == self.seen_id {
            return;
        }
        self.seen_id = cur;
        self.seen = cur.map(|id| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                let _ = this.update(cx, |w, _| {
                    if w.current().map(id_of) == Some(id) {
                        let _ = concepts::mark_seen(&w.db, id);
                    }
                });
            })
        });
    }

    /// 단축키 — 아래 단추와 1:1. 조합키는 앱/OS 몫
    fn on_key(&mut self, e: &KeyDownEvent, cx: &mut Context<Self>) {
        let m = &e.keystroke.modifiers;
        if m.platform || m.control || m.alt {
            return;
        }
        let conf = self
            .current()
            .and_then(|r| r.get("confidence")?.as_i64())
            .unwrap_or(1);
        match e.keystroke.key.as_str() {
            "left" => self.go(-1, cx),
            "right" => self.go(1, cx),
            "enter" | "d" => self.complete(cx),
            "down" | "[" if conf > 1 => self.conf(-1, cx),
            "up" | "]" if conf < 3 => self.conf(1, cx),
            "o" | "space" => self.open_detail(cx),
            _ => {}
        }
    }
}

impl Render for Widget {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.track_seen(cx);
        let k = Tokens::get(cx).clone();
        let btn = |id: &'static str, name: &'static str, size: f32, label: String, off: bool| {
            let hover = crate::settings_view::glass(k.glass_fill_hover);
            let text = k.text;
            div()
                .id(id)
                .tag(id)
                .min_w(px(26.))
                .h(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .text_color(k.text_2)
                .tooltip(tip(label))
                .child(icon(name, size).text_color(k.text_2))
                .when(off, |d| d.opacity(0.4))
                .when(!off, |d| {
                    d.cursor_pointer()
                        .hover(move |s| s.bg(hover).text_color(text))
                })
        };
        let head = div()
            .id("widget-head")
            .flex()
            .items_center()
            .gap(px(8.))
            .pt(px(8.))
            .px(px(12.))
            .pb(px(4.))
            .cursor_grab()
            .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
            .child(
                div()
                    .text_size(px(10.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(k.text_3)
                    .child("TIL"),
            )
            .child(
                div()
                    .ml_auto()
                    .text_size(px(11.))
                    .text_color(k.text_3)
                    .when(!self.queue.is_empty(), |d| {
                        d.child(format!(
                            "{} / {}",
                            if self.current().is_some() {
                                self.idx + 1
                            } else {
                                0
                            },
                            self.queue.len()
                        ))
                    }),
            )
            .child(
                div()
                    .id("widget-x")
                    .tag("widget-x")
                    .px(px(4.))
                    .py(px(2.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|s| s.bg(k.surface_2))
                    .tooltip(tip(t("app.widget.hide")))
                    .child(icon("x", 13.).text_color(k.text_3))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, window, cx| this.hide(window, cx))),
            );
        let card = div()
            .absolute()
            .top(px(10.))
            .left(px(10.))
            .right(px(10.))
            .bottom(px(10.))
            .flex()
            .flex_col()
            .bg(k.surface)
            .border_1()
            .border_color(k.border_strong)
            .rounded(px(12.))
            .shadow(k.shadow())
            .overflow_hidden()
            .child(head);
        let body = if self.ready && self.queue.is_empty() {
            card.child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(10.))
                    .p(px(16.))
                    .text_size(px(13.))
                    .text_color(k.text_3)
                    .child(t("app.widget.emptyQueue"))
                    .child(
                        btn(
                            "widget-open-main",
                            "layers",
                            14.,
                            t("app.widget.openMain"),
                            false,
                        )
                        .px(px(10.))
                        .child(t("app.widget.openMain"))
                        .on_click(cx.listener(|w, _, _, cx| w.show_main(cx))),
                    ),
            )
        } else {
            let cur = self.current().cloned();
            let title = cur
                .as_ref()
                .and_then(|r| r.get("title")?.as_str().map(str::to_string))
                .unwrap_or_default();
            let summary = cur
                .as_ref()
                .and_then(|r| r.get("summary")?.as_str().map(str::to_string))
                .unwrap_or_default();
            let conf = cur
                .as_ref()
                .and_then(|r| r.get("confidence")?.as_i64())
                .unwrap_or(1);
            let nav = |id: &'static str, name: &'static str, label: String| {
                let (hover, text) = (k.surface_2, k.text);
                div()
                    .id(id)
                    .tag(id)
                    .flex_none()
                    .w(px(34.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .tooltip(tip(label))
                    .child(
                        div()
                            .size(px(26.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .hover(move |s| s.bg(hover).text_color(text))
                            .child(icon(name, 18.).text_color(k.text_3)),
                    )
            };
            card.child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        nav("widget-prev", "chevron-left", t("app.widget.prev"))
                            .on_click(cx.listener(|w, _, _, cx| w.go(-1, cx))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .pt(px(2.))
                            .px(px(4.))
                            .pb(px(8.))
                            .overflow_hidden()
                            .child(
                                div()
                                    .mb(px(6.))
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::BOLD)
                                    .line_clamp(2)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .line_height(relative(1.5))
                                    .text_color(k.text_2)
                                    .line_clamp(3)
                                    .child(summary),
                            ),
                    )
                    .child(
                        nav("widget-next", "chevron-right", t("app.widget.next"))
                            .on_click(cx.listener(|w, _, _, cx| w.go(1, cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .py(px(7.))
                    .px(px(10.))
                    .border_t_1()
                    .border_color(k.border)
                    .when(cur.is_some(), |d| {
                        d.child(crate::concepts_view::confidence_dots(&k, conf))
                    })
                    .child(div().flex_1())
                    .child(
                        btn(
                            "widget-complete",
                            "check",
                            15.,
                            t("app.widget.complete"),
                            false,
                        )
                        .on_click(cx.listener(|w, _, _, cx| w.complete(cx))),
                    )
                    .child({
                        let off = cur.is_none() || conf <= 1;
                        btn(
                            "widget-conf-down",
                            "minus",
                            15.,
                            t("app.widget.confDown"),
                            off,
                        )
                        .when(!off, |b| {
                            b.on_click(cx.listener(|w, _, _, cx| w.conf(-1, cx)))
                        })
                    })
                    .child({
                        let off = cur.is_none() || conf >= 3;
                        btn("widget-conf-up", "plus", 15., t("app.widget.confUp"), off)
                            .when(!off, |b| {
                                b.on_click(cx.listener(|w, _, _, cx| w.conf(1, cx)))
                            })
                    })
                    .child(
                        btn(
                            "widget-detail",
                            "expand",
                            14.,
                            t("app.widget.openDetail"),
                            false,
                        )
                        .on_click(cx.listener(|w, _, _, cx| w.open_detail(cx))),
                    ),
            )
        };
        let _ = window;
        div()
            .size_full()
            .relative()
            .track_focus(&self.focus)
            .text_color(k.text)
            .font_family("Apple SD Gothic Neo")
            .text_size(px(14.))
            .on_key_down(cx.listener(|w, e: &KeyDownEvent, _, cx| w.on_key(e, cx)))
            .child(body)
    }
}

impl Spike {
    /// 위젯 창을 연다 — 이미 떠 있으면 앞으로
    pub(crate) fn open_widget(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(h) = self.widget.as_ref()
            && h.update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            return;
        }
        let path = self.db.data_dir().join("amber.db");
        let main = cx.entity().downgrade();
        let main_window = window.window_handle();
        let bounds = self
            .widget_bounds
            .unwrap_or_else(|| Bounds::centered(None, size(px(320.), px(210.)), cx));
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            kind: WindowKind::PopUp,
            is_movable: true,
            focus: true,
            show: true,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        };
        let dirty = self.widget_flag.clone();
        // 숨겼던 위젯이면 그 상태 그대로(넘기던 카드), 그 자리에
        let keep = self.widget_view.clone();
        let made: std::rc::Rc<std::cell::RefCell<Option<Entity<Widget>>>> = Default::default();
        let out = made.clone();
        let opened = cx.open_window(options, move |window, cx| {
            let v = match keep {
                Some(v) => {
                    let focus = v.read(cx).focus.clone();
                    focus.focus(window, cx);
                    v
                }
                None => {
                    let db =
                        Db::open(path.to_str().unwrap_or_default(), Clock::System).expect("DB");
                    cx.new(|cx| Widget::new(db, dirty, main, main_window, window, cx))
                }
            };
            *out.borrow_mut() = Some(v.clone());
            v
        });
        if let Ok(h) = opened {
            self.widget = Some(h);
            self.widget_view = made.borrow_mut().take();
        }
    }
}

#[cfg(test)]
mod tests {
    // 큐 이동 규칙 — 처음과 끝에서 돈다(지금 앱 go 의 ((from + delta) % n + n) % n)
    #[test]
    fn moves_wrap_around_the_queue() {
        let wrap = |from: i64, delta: i64, n: i64| ((from + delta) % n + n) % n;
        assert_eq!(wrap(0, -1, 3), 2);
        assert_eq!(wrap(2, 1, 3), 0);
        assert_eq!(wrap(1, 1, 3), 2);
    }
}
