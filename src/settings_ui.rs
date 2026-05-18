//! 設定パネルの view + click handler。 4 トリガ (Mouse / Right / Double / Key)
//! を タブで切り替え、 各 trigger の 9 param を +/- ボタン (数値) や ◀▶ ボタン
//! (enum) で編集する。 drag slider を実装しないので click だけで全機能完結する
//! 構造。 派手さは無いが配線が極端に単純。
//!
//! View 関数は HanabiFx の view() から `--edit` モード時のみ呼ばれる。 click
//! handler は HanabiFx::on_click から id プレフィクスで dispatch される。

use sabitori::{div, text, Color, Element, Px};

use crate::config::{Config, FxPalette, FxParams, FxShape};

/// 現在表示中の trigger タブ。 view 構築側が main 構造体に持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Mouse,
    Right,
    Double,
    Key,
}

impl Trigger {
    pub fn label(self) -> &'static str {
        match self {
            Trigger::Mouse => "Mouse",
            Trigger::Right => "Right",
            Trigger::Double => "Double",
            Trigger::Key => "Key",
        }
    }
    pub fn id_slug(self) -> &'static str {
        match self {
            Trigger::Mouse => "mouse",
            Trigger::Right => "right",
            Trigger::Double => "double",
            Trigger::Key => "key",
        }
    }
    pub fn params<'a>(self, cfg: &'a Config) -> &'a FxParams {
        match self {
            Trigger::Mouse => &cfg.mouse,
            Trigger::Right => &cfg.right,
            Trigger::Double => &cfg.double,
            Trigger::Key => &cfg.key,
        }
    }
    pub fn params_mut<'a>(self, cfg: &'a mut Config) -> &'a mut FxParams {
        match self {
            Trigger::Mouse => &mut cfg.mouse,
            Trigger::Right => &mut cfg.right,
            Trigger::Double => &mut cfg.double,
            Trigger::Key => &mut cfg.key,
        }
    }
}

const PANEL_W: f32 = 360.0;
const PANEL_H: f32 = 560.0;
const BG: Color = Color { r: 0.10, g: 0.10, b: 0.12, a: 0.95 };
const PANEL_BG: Color = Color { r: 0.14, g: 0.14, b: 0.16, a: 1.0 };
const TEXT: Color = Color { r: 0.92, g: 0.92, b: 0.94, a: 1.0 };
const MUTED: Color = Color { r: 0.55, g: 0.55, b: 0.60, a: 1.0 };
const ACCENT: Color = Color { r: 0.45, g: 0.85, b: 0.55, a: 1.0 };
const BTN_BG: Color = Color { r: 0.22, g: 0.22, b: 0.25, a: 1.0 };
const BTN_ACTIVE_BG: Color = Color { r: 0.30, g: 0.65, b: 0.45, a: 1.0 };
const BTN_HOVER_BG: Color = Color { r: 0.30, g: 0.30, b: 0.34, a: 1.0 };

pub fn panel_size() -> (f32, f32) {
    (PANEL_W, PANEL_H)
}

pub fn view(cfg: &Config, active: Trigger) -> Element {
    let p = active.params(cfg);

    let tabs = div().flex_row().gap(4.0).children(
        [Trigger::Mouse, Trigger::Right, Trigger::Double, Trigger::Key]
            .into_iter()
            .map(|t| tab_button(t, t == active))
            .collect::<Vec<_>>(),
    );

    let enabled_row = div()
        .flex_row()
        .items_center()
        .gap(8.0)
        .children([
            text("Enabled").font_size(13.0).color(MUTED).w(Px(70.0)),
            toggle_button("enabled", cfg.enabled),
        ]);

    let rows: Vec<Element> = vec![
        param_int_row("Count", "count", p.count as i32, 1, 50, 1, active),
        param_f32_row("Speed", "speed", p.speed, 50.0, 800.0, 25.0, active),
        param_f32_row("Spread", "spread", p.spread, 0.0, 360.0, 15.0, active),
        param_f32_row("Gravity", "gravity", p.gravity, -500.0, 1500.0, 50.0, active),
        param_f32_row("Lifetime", "lifetime", p.lifetime, 0.2, 3.0, 0.1, active),
        param_f32_row("Radius", "radius", p.radius, 2.0, 18.0, 0.5, active),
        twinkle_row(p.twinkle, active),
        shape_row(p.shape, active),
        palette_row(p.palette, active),
    ];

    let footer = div()
        .flex_row()
        .gap(6.0)
        .mt(Px(8.0))
        .children([
            action_button("reset", "Reset trigger"),
            action_button("done", "Done"),
            action_button("quit", "Quit"),
        ]);

    div()
        .w(Px(PANEL_W))
        .h(Px(PANEL_H))
        .bg(BG)
        .child(
            div()
                .w_full()
                .h_full()
                .pl(Px(16.0)).pr(Px(16.0))
                .py(Px(16.0))
                .flex_col()
                .gap(10.0)
                .children([
                    text("hanabifx").font_size(18.0).color(TEXT),
                    text("press keys / click to preview")
                        .font_size(11.0)
                        .color(MUTED),
                    enabled_row,
                    tabs,
                    div()
                        .w_full()
                        .bg(PANEL_BG)
                        .rounded_px(8.0)
                        .pl(Px(12.0)).pr(Px(12.0))
                        .py(Px(12.0))
                        .flex_col()
                        .gap(8.0)
                        .children(rows),
                    footer,
                ]),
        )
}

fn tab_button(t: Trigger, active: bool) -> Element {
    let bg = if active { BTN_ACTIVE_BG } else { BTN_BG };
    div()
        .id(format!("tab-{}", t.id_slug()))
        .h(Px(28.0))
        .pl(Px(12.0)).pr(Px(12.0))
        .bg(bg)
        .rounded_px(6.0)
        .flex_row()
        .items_center()
        .justify_center()
        .child(text(t.label()).font_size(12.0).color(TEXT))
        .pressable(BTN_HOVER_BG)
}

fn param_int_row(
    label: &str,
    field: &str,
    value: i32,
    min: i32,
    max: i32,
    _step: i32,
    active: Trigger,
) -> Element {
    let id_base = format!("p-{}-{}", active.id_slug(), field);
    row_layout(
        label,
        format!("{value}"),
        value > min,
        value < max,
        &id_base,
    )
}

fn param_f32_row(
    label: &str,
    field: &str,
    value: f32,
    min: f32,
    max: f32,
    _step: f32,
    active: Trigger,
) -> Element {
    let id_base = format!("p-{}-{}", active.id_slug(), field);
    row_layout(
        label,
        format!("{value:.1}"),
        value > min + 1e-3,
        value < max - 1e-3,
        &id_base,
    )
}

fn row_layout(
    label: &str,
    value_str: String,
    can_dec: bool,
    can_inc: bool,
    id_base: &str,
) -> Element {
    div()
        .flex_row()
        .items_center()
        .gap(8.0)
        .children([
            text(label.to_string()).font_size(13.0).color(MUTED).w(Px(80.0)),
            small_button(&format!("{id_base}-dec"), "−", can_dec),
            div()
                .w(Px(72.0))
                .flex_row()
                .justify_center()
                .child(text(value_str).font_size(13.0).color(TEXT)),
            small_button(&format!("{id_base}-inc"), "+", can_inc),
        ])
}

fn twinkle_row(value: bool, active: Trigger) -> Element {
    div()
        .flex_row()
        .items_center()
        .gap(8.0)
        .children([
            text("Twinkle").font_size(13.0).color(MUTED).w(Px(80.0)),
            toggle_button(&format!("p-{}-twinkle", active.id_slug()), value),
        ])
}

fn shape_row(value: FxShape, active: Trigger) -> Element {
    let id_base = format!("p-{}-shape", active.id_slug());
    enum_row(
        "Shape",
        shape_label(value),
        &id_base,
    )
}

fn palette_row(value: FxPalette, active: Trigger) -> Element {
    let id_base = format!("p-{}-palette", active.id_slug());
    enum_row(
        "Palette",
        palette_label(value),
        &id_base,
    )
}

fn enum_row(label: &str, value_label: &'static str, id_base: &str) -> Element {
    div()
        .flex_row()
        .items_center()
        .gap(8.0)
        .children([
            text(label.to_string()).font_size(13.0).color(MUTED).w(Px(80.0)),
            small_button(&format!("{id_base}-prev"), "◀", true),
            div()
                .w(Px(80.0))
                .flex_row()
                .justify_center()
                .child(text(value_label).font_size(12.0).color(ACCENT)),
            small_button(&format!("{id_base}-next"), "▶", true),
        ])
}

fn small_button(id: &str, label: &str, enabled: bool) -> Element {
    let bg = if enabled { BTN_BG } else { Color { r: 0.16, g: 0.16, b: 0.18, a: 1.0 } };
    let fg = if enabled { TEXT } else { MUTED };
    let mut el = div()
        .id(id.to_string())
        .w(Px(28.0))
        .h(Px(24.0))
        .bg(bg)
        .rounded_px(5.0)
        .flex_row()
        .items_center()
        .justify_center()
        .child(text(label.to_string()).font_size(14.0).color(fg));
    if enabled {
        el = el.pressable(BTN_HOVER_BG);
    }
    el
}

fn action_button(id: &str, label: &str) -> Element {
    div()
        .id(id.to_string())
        .h(Px(28.0))
        .pl(Px(12.0)).pr(Px(12.0))
        .bg(BTN_BG)
        .rounded_px(6.0)
        .flex_row()
        .items_center()
        .justify_center()
        .child(text(label.to_string()).font_size(12.0).color(TEXT))
        .pressable(BTN_HOVER_BG)
}

fn toggle_button(id: &str, value: bool) -> Element {
    let bg = if value { BTN_ACTIVE_BG } else { BTN_BG };
    let label = if value { "ON" } else { "OFF" };
    div()
        .id(id.to_string())
        .h(Px(24.0))
        .pl(Px(10.0)).pr(Px(10.0))
        .bg(bg)
        .rounded_px(5.0)
        .flex_row()
        .items_center()
        .justify_center()
        .child(text(label.to_string()).font_size(12.0).color(TEXT))
        .pressable(BTN_HOVER_BG)
}

fn shape_label(s: FxShape) -> &'static str {
    match s {
        FxShape::Default => "default",
        FxShape::Disc => "disc",
        FxShape::Square => "square",
        FxShape::Ring => "ring",
        FxShape::Triangle => "triangle",
        FxShape::Star => "star",
        FxShape::Heart => "heart",
        FxShape::Typed => "typed",
    }
}

fn palette_label(p: FxPalette) -> &'static str {
    match p {
        FxPalette::Default => "default",
        FxPalette::Mono => "mono",
        FxPalette::Pastel => "pastel",
        FxPalette::Neon => "neon",
        FxPalette::Rainbow => "rainbow",
    }
}

const SHAPE_ORDER: [FxShape; 8] = [
    FxShape::Default,
    FxShape::Disc,
    FxShape::Square,
    FxShape::Ring,
    FxShape::Triangle,
    FxShape::Star,
    FxShape::Heart,
    FxShape::Typed,
];

const PALETTE_ORDER: [FxPalette; 5] = [
    FxPalette::Default,
    FxPalette::Mono,
    FxPalette::Pastel,
    FxPalette::Neon,
    FxPalette::Rainbow,
];

fn cycle<T: Copy + PartialEq>(order: &[T], current: T, dir: i32) -> T {
    let n = order.len() as i32;
    let idx = order.iter().position(|x| *x == current).unwrap_or(0) as i32;
    let next = (idx + dir).rem_euclid(n) as usize;
    order[next]
}

/// クリックされたボタン id を解釈して cfg を直接書き換える。
/// 戻り値 `Some(new_trigger)` は「タブが切り替わったので active_trigger
/// をこれに更新せよ」 のサイン。 None は cfg だけ更新したケース。
/// `quit_requested` は明示的にアプリ終了したい時の signal。
#[must_use]
pub fn handle_click(id: &str, cfg: &mut Config, active: Trigger) -> ClickEffect {
    // タブ切替
    if let Some(slug) = id.strip_prefix("tab-") {
        let t = match slug {
            "mouse" => Trigger::Mouse,
            "right" => Trigger::Right,
            "double" => Trigger::Double,
            "key" => Trigger::Key,
            _ => return ClickEffect::None,
        };
        return ClickEffect::SwitchTab(t);
    }
    if id == "enabled" {
        cfg.enabled = !cfg.enabled;
        return ClickEffect::Changed;
    }
    if id == "quit" {
        return ClickEffect::Quit;
    }
    if id == "done" {
        return ClickEffect::HideSettings;
    }
    if id == "reset" {
        *active.params_mut(cfg) = FxParams::default();
        return ClickEffect::Changed;
    }
    if let Some(rest) = id.strip_prefix("p-") {
        // 形式: p-<trigger>-<field>-<dec|inc|prev|next>
        // 例: p-mouse-speed-inc
        let mut parts = rest.splitn(3, '-');
        let trig_slug = parts.next().unwrap_or("");
        let field = parts.next().unwrap_or("");
        let action = parts.next().unwrap_or("");
        let trig = match trig_slug {
            "mouse" => Trigger::Mouse,
            "right" => Trigger::Right,
            "double" => Trigger::Double,
            "key" => Trigger::Key,
            _ => return ClickEffect::None,
        };
        let p = trig.params_mut(cfg);
        let dir: f32 = match action {
            "dec" | "prev" => -1.0,
            "inc" | "next" => 1.0,
            _ => 0.0,
        };
        if dir == 0.0 && action != "twinkle" {
            return ClickEffect::None;
        }
        match field {
            "count" => p.count = (p.count as i32 + dir as i32).clamp(1, 50) as u32,
            "speed" => p.speed = (p.speed + dir * 25.0).clamp(50.0, 800.0),
            "spread" => p.spread = (p.spread + dir * 15.0).clamp(0.0, 360.0),
            "gravity" => p.gravity = (p.gravity + dir * 50.0).clamp(-500.0, 1500.0),
            "lifetime" => p.lifetime = (p.lifetime + dir * 0.1).clamp(0.2, 3.0),
            "radius" => p.radius = (p.radius + dir * 0.5).clamp(2.0, 18.0),
            "twinkle" => {
                if action == "twinkle" {
                    p.twinkle = !p.twinkle;
                }
            }
            "shape" => p.shape = cycle(&SHAPE_ORDER, p.shape, dir as i32),
            "palette" => p.palette = cycle(&PALETTE_ORDER, p.palette, dir as i32),
            _ => return ClickEffect::None,
        }
        return ClickEffect::Changed;
    }
    ClickEffect::None
}

#[must_use]
pub enum ClickEffect {
    None,
    Changed,
    SwitchTab(Trigger),
    /// パネルを `orderOut:` で隠して fx は走らせ続ける。 dock icon が
    /// 残っているので Cmd-Q で完全終了は可能。 v0.1 では再表示の UI は
    /// 持たないので、 また編集したい時は Cmd-Q → 再起動 (`--edit`)。
    HideSettings,
    Quit,
}
