# hanabifx (花火 fx)

> 押すと花火。 macOS で キー入力 / クリックに反応して全画面に粒子が飛ぶ常駐オーバーレイ。

タイピング中の画面が「無反応」 で退屈なのを直すために作った。 透明な
全画面 NSWindow を 1 枚だけ張って、 グローバル NSEvent モニタで
キーボード / マウス入力を拾い、 カーソル位置から sabitori 描画の粒子が
散る。 入力イベント本体は全部捨てるので keylogger にはならない。

[matcha](https://github.com/Mugendesk/matcha) の fx 機能を独立アプリとして
切り出したもの。 matcha は menu bar / dock / launcher と event loop を
共有していて fx の cadence と他機能の cadence が衝突していたので、
fx 専用のプロセスに分離した。 結果として 120Hz / vsync 整列が制約なく
できるようになり、 fx の動きが滑らかになっている。

## 動作要件

- macOS (Apple Silicon 推奨)
- Rust 1.81+ (Cargo workspace + edition 2024)
- Accessibility 権限 (グローバル入力監視のため、 起動時に macOS が要求する)

## ビルド & 起動

```bash
git clone https://github.com/Mutafika/hanabifx.git
cd hanabifx
cargo run --release
```

初回起動時に `~/.config/hanabifx/config.toml` がデフォルト値で書き出される。
編集後の live 反映は v0 では未対応 — 再起動で読み直す。

## 設定

`~/.config/hanabifx/config.toml`:

```toml
enabled = true

[mouse]   # 左クリック
count = 12
speed = 350.0
spread = 360.0
gravity = 900.0
lifetime = 0.8
radius = 6.0
twinkle = false
shape = "default"     # default / disc / square / ring / triangle / star / heart / typed
palette = "default"   # default / mono / pastel / neon / rainbow

[right]   # 右クリック
# 既定: パレット neon、 他は mouse と同じ

[double]  # ダブルクリック
# 既定: count 20、 speed 500、 radius 8、 twinkle true

[key]     # キー入力
# 既定: shape "typed"、 palette "rainbow"、 spread 60° で押した文字が花火状に散る
count = 10
shape = "typed"
palette = "rainbow"
```

トリガごとの完全な FxParams は [`src/config.rs`](./src/config.rs) を参照。

## 依存

- [sabitori](https://github.com/Mutafika/sabitori) — wgpu の declarative UI ランタイム
- [winit](https://github.com/rust-windowing/winit) / [wgpu](https://github.com/gfx-rs/wgpu) — ウィンドウとレンダラ
- [objc2](https://github.com/madsmtm/objc2) — NSEvent / NSWindow への Obj-C ブリッジ

## ステータス

v0 段階。 単一ディスプレイ + 2D 粒子のみ。 multi-display / 3D マテリアル
(murakumo) / 設定 GUI / live config reload は v0.1+ で予定。

## License

Licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
