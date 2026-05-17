//! hanabifx の永続設定 + 各 fx event の motion/shape/palette。
//!
//! 設定は `~/.config/hanabifx/config.toml` に置く (XDG_CONFIG_HOME 相当)。
//! ファイルが無ければ Default を書き出して起動する。 編集後の live 反映は
//! v0 未対応 — 再起動で読み直す。 3D fx (murakumo マテリアル) は本家 matcha
//! には居るが v0 では 2D に絞ったので FxParams からも 3D フィールドは外して
//! ある。 v0.1 以降で復活させる時に追加すること。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// トリガ別 motion + shape + palette。 各トリガが独立な FxParams を持つので
/// 「クリックは Mono の Disc、 キーは Rainbow の Typed」 のように混在可能。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FxParams {
    /// 1 burst あたりの粒子数。 1..=50。
    #[serde(default = "default_count")]
    pub count: u32,
    /// 初速の大きさ (px/s)。 50..=800。
    #[serde(default = "default_speed")]
    pub speed: f32,
    /// spawn 方向の cone 幅 (度)。 0 = 全粒子が同方向 (真上)、 360 = 円周方向に散る。
    #[serde(default = "default_spread")]
    pub spread: f32,
    /// 重力加速度 (px/s²)。 負 = bubble、 0 = drift、 正 = fall。 -500..=1500。
    #[serde(default = "default_gravity")]
    pub gravity: f32,
    /// 寿命 (秒)。 0.2..=3.0。
    #[serde(default = "default_lifetime")]
    pub lifetime: f32,
    /// 基本半径 (px)。 spawn 時に jitter が乗るので単一値でも variation が出る。 2..=18。
    #[serde(default = "default_radius")]
    pub radius: f32,
    /// 線形 fade に sin alpha pulse を重ねる。 true で sparkle 風になる。
    #[serde(default)]
    pub twinkle: bool,
    /// 粒子形状 override。
    #[serde(default)]
    pub shape: FxShape,
    /// パレット override。
    #[serde(default)]
    pub palette: FxPalette,
}

fn default_count() -> u32 {
    12
}
fn default_speed() -> f32 {
    350.0
}
fn default_spread() -> f32 {
    360.0
}
fn default_gravity() -> f32 {
    900.0
}
fn default_lifetime() -> f32 {
    0.8
}
fn default_radius() -> f32 {
    6.0
}

impl Default for FxParams {
    fn default() -> Self {
        Self {
            count: default_count(),
            speed: default_speed(),
            spread: default_spread(),
            gravity: default_gravity(),
            lifetime: default_lifetime(),
            radius: default_radius(),
            twinkle: false,
            shape: FxShape::default(),
            palette: FxPalette::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FxShape {
    #[default]
    Default,
    Disc,
    Square,
    Ring,
    Triangle,
    Star,
    Heart,
    /// Key burst 用 — 押された文字をグリフ粒子としてばら撒く。 マウスは
    /// 文字を持たないので fallback して Disc になる。
    Typed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FxPalette {
    #[default]
    Default,
    /// 全粒子白。 どんな背景でもうっすら光る。
    Mono,
    /// くすんだパステル 5 色。
    Pastel,
    /// 飽和した electric neon 5 色。
    Neon,
    /// 粒子 index で hue がスライドする虹色。
    Rainbow,
}

/// トップレベル設定。 4 つのトリガ (mouse / right / double / key) が
/// それぞれ独立な FxParams を持つ。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub mouse: FxParams,
    #[serde(default)]
    pub right: FxParams,
    #[serde(default)]
    pub double: FxParams,
    #[serde(default = "default_key_params")]
    pub key: FxParams,
}

fn default_true() -> bool {
    true
}

/// Key 用デフォルト: Typed + Rainbow + spread 60° で 「押した字が花火状に散る」 が
/// 出やすい初期値にしてある。 マウスは Default + Default のまま。
fn default_key_params() -> FxParams {
    FxParams {
        count: 10,
        speed: 280.0,
        spread: 60.0,
        gravity: 1000.0,
        lifetime: 0.9,
        radius: 7.0,
        twinkle: false,
        shape: FxShape::Typed,
        palette: FxPalette::Rainbow,
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            mouse: FxParams::default(),
            right: FxParams {
                palette: FxPalette::Neon,
                ..FxParams::default()
            },
            double: FxParams {
                count: 20,
                speed: 500.0,
                radius: 8.0,
                twinkle: true,
                ..FxParams::default()
            },
            key: default_key_params(),
        }
    }
}

impl Config {
    /// `~/.config/hanabifx/config.toml` を読む。 存在しなければ Default を
    /// 書き出してそれを返す。 パース失敗は eprintln してデフォルトに fall back。
    pub fn load_or_create() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(s) => match toml::from_str::<Config>(&s) {
                Ok(cfg) => cfg,
                Err(e) => {
                    eprintln!("[hanabifx] config parse error ({path:?}): {e} — using defaults");
                    Config::default()
                }
            },
            Err(_) => {
                let cfg = Config::default();
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Ok(s) = toml::to_string_pretty(&cfg) {
                    let _ = std::fs::write(&path, s);
                }
                cfg
            }
        }
    }

    fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("hanabifx")
            .join("config.toml")
    }
}
