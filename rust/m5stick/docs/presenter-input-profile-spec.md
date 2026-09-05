# M5Stick Presenter 入力・プロファイル仕様

## 1. 目的

M5StickC Plus2 と Mini JoyC HAT を，BLE HID のプレゼンター兼簡易マウスとして使用する．

主な用途は以下とする．

- スライド送り・戻し
- マウスポインタ操作
- 左クリック，右クリック，中クリック
- スクロール
- PC，OS，画面解像度に応じた操作感の切り替え
- 縦持ち，横持ちへの対応

設定可能な項目はできるだけ `presenter.toml` に集約し，ファームウェアを書き換えず設定値を調整できる構造にする．

## 2. 物理入力

使用する入力は以下とする．

- Button A
- Button B
- Mini JoyC X/Y
- Mini JoyC 押し込み
- Button C

Button C は電源専用とする．

- C 約2秒長押し: 電源ON
- C 約6秒長押し: 電源OFF

Button C は通常のHID操作には使用しない．

## 3. 複数プロファイル

複数の操作プロファイルを `presenter.toml` に定義できるようにする．

想定例は以下とする．

- Linux
- Mac
- 横持ち
- 高解像度ディスプレイ
- プロジェクター

プロファイルごとに少なくとも以下を変更可能とする．

- マウス最大速度
- 入力カーブ
- dead zone
- raw入力値の smoothing
- X/Y方向
- 画面回転方向
- キーマッピング
- combo
- スクロール設定

プロファイル順序はTOMLテーブルの出現順に依存せず，`general.profile_order` で明示する．

```toml
[general]
default_profile = "linux"
profile_order = ["linux", "mac", "landscape"]
```

現在選択中のプロファイルはLCDに表示する．

プロファイル切り替え操作は，デフォルトでは以下とする．

```text
A + B 長押し
-> 次のプロファイル
```

選択中プロファイルは将来的にFlashへ保存し，電源再投入後も維持できるようにする．初期実装では，`profile.rs` に選択中プロファイルの管理境界を作り，Flash保存は未実装でもよい．

## 4. 設定の上書き規則

`input` と `scroll` はグローバル既定値を持ち，必要なプロファイルだけ `profiles.<name>` 配下で上書きできるようにする．

上書き規則は以下とする．

- `profiles.<name>.mouse` は必須とする．
- `profiles.<name>.input` がある場合，同じ項目だけグローバル `input` を上書きする．
- `profiles.<name>.scroll` がある場合，同じ項目だけグローバル `scroll` を上書きする．
- combo配列をプロファイルで指定した場合，グローバルcomboを丸ごと置き換える．部分的な配列マージは行わない．
- `general.default_profile` は `general.profile_order` と `profiles` の両方に存在しなければならない．

配列マージを行わない理由は，comboの優先順位とキャンセル対象が曖昧になりやすいためである．

## 5. マルチキー入力

入力処理は単純なGPIO edge detectionではなく，複数入力を同時に扱えるステートマシンとして実装する．

対応対象は以下とする．

- 単押し
- 長押し
- 2キー以上の同時押し
- Button + JoyC操作
- Button + JoyC押し込み
- 将来的な3キーcombo

入力処理は概念的に以下の流れとする．

```text
GPIO / JoyC
    |
    v
InputState
    |
    v
combo判定
    |
    v
tap / hold判定
    |
    v
Action
    |
    v
BLE HID
```

入力状態の例は以下とする．

```rust
pub struct InputState {
    pub button_a: bool,
    pub button_b: bool,
    pub joy_click: bool,
}
```

生成するアクションは，HID usage ID の生値ではなく意味ベースで表現する．

```rust
pub enum Action {
    KeyboardKey(KeyboardKey),
    MouseButton(MouseButton),
    ScrollMode,
    NextProfile,
    None,
}

pub enum KeyboardKey {
    RightArrow,
    LeftArrow,
    PageDown,
    PageUp,
    Space,
    Enter,
    Escape,
    F5,
}

pub enum MouseButton {
    Left,
    Right,
    Middle,
}
```

`KeyboardKey` や `MouseButton` からHID report上の値への変換は，`ble.rs` またはHID専用モジュール側の責務とする．

## 6. 単押し，長押し，comboの優先順位

単押し，長押し，comboの優先順位は以下とする．

```text
1. 現在押下中の入力集合を更新する
2. combo候補を検出する
3. comboが成立した入力は単体tap/hold対象から除外する
4. hold条件を満たしたcombo/actionを発火する
5. release時に未消費の単体tapを発火する
```

入力判定用の共通設定は以下とする．

```toml
[input]
combo_window_ms = 80
tap_max_ms = 300
default_hold_ms = 600
```

`combo_window_ms` は，完全な同時押しでなくてもcomboとして扱うための猶予時間である．例えば `A` を押してから80ms以内に `Joy Click` が押された場合，`A + Joy Click` comboとして扱える．

comboが成立した場合，構成要素となる単体入力のアクションはキャンセルする．

例えば以下の場合，右クリック操作時に `Next Slide` を送信してはいけない．

```text
A単押し
-> Next Slide

A + Joy Click
-> Right Click
```

## 7. 単押し，長押し

各入力について，必要に応じて短押しと長押しを別アクションへ割り当てられるようにする．

```toml
[input.button_a]
tap = "RightArrow"

[input.button_b]
tap = "LeftArrow"

[input.joy_click]
tap = "MouseLeft"
```

長押しを使用する場合は以下とする．

```toml
[input.joy_click]
tap = "MouseLeft"
hold = "MouseRight"
hold_ms = 600
```

ただし，`tap` と `hold` を同一入力に割り当てた場合，短押しか長押しか確定するまで `tap` を送信できない．そのため，即応性が重要な左クリックについては，長押しよりcomboを優先する．

推奨操作は以下とする．

```text
Joy Click
-> 左クリック

A + Joy Click
-> 右クリック

B + Joy Click
-> 中クリック
```

## 8. Combo

複数入力の組み合わせを設定可能とする．

```toml
[[input.combo]]
buttons = ["button_a", "joy_click"]
action = "MouseRight"

[[input.combo]]
buttons = ["button_b", "joy_click"]
action = "MouseMiddle"

[[input.combo]]
buttons = ["button_a", "button_b"]
hold_ms = 800
action = "NextProfile"
```

comboの優先順位は，より多くの入力を含むcomboを優先し，同じ入力数の場合は設定ファイル上の出現順を優先する．

将来的な3キーcomboでも，同じ規則を使用する．

## 9. JoyCマウス操作

通常時のJoyCはマウスカーソル移動に使用する．

```text
JoyC X/Y
-> Pointer

JoyC押し込み
-> Left Click
```

低速域の滑らかさを確保するため，1px未満の移動量は内部状態として蓄積する．

カーソル速度そのものに方向慣性は持たせない．方向転換への応答が低下し，JoyCを円状に動かす操作などがしにくくなるためである．

`smoothing` はJoyC raw入力値のノイズ除去にのみ使用する．出力カーソル速度や方向ベクトルには慣性を持たせない．

将来的に必要であれば，以下の方式でspeed gainのみを上昇させる．

```text
現在のJoyC方向
-> 即座にカーソル方向へ反映

倒し続けた時間
-> speed gainのみ上昇
```

## 10. 90度回転，横持ち

単純な `invert_x` / `invert_y` だけではなく，X/Y入れ替えを含む4方向回転へ対応する．

使用可能値は以下とする．

```text
normal
right
inverted
left
```

変換は以下とする．

```text
normal
(x, y) -> ( x,  y)

right
(x, y) -> (-y,  x)

inverted
(x, y) -> (-x, -y)

left
(x, y) -> ( y, -x)
```

必要であれば回転後に `invert_x` / `invert_y` で追加反転できるようにする．

```toml
orientation = "right"
invert_x = false
invert_y = false
```

初期仕様では，プロファイルの `orientation` によってJoyC方向とLCD表示方向の両方を回転させる．将来的に分離が必要になった場合は，`display.orientation` で上書きできるようにする．

## 11. スクロール

JoyCをスクロール操作にも利用できるようにする．

推奨デフォルトは以下とする．

```text
A + JoyC Y
-> 縦スクロール
```

将来的には以下にも対応可能とする．

```text
A + JoyC X
-> 横スクロール
```

通常時は以下とする．

```text
JoyC
-> Pointer
```

Aを押してJoyCを操作した場合は以下とする．

```text
A + JoyC
-> Scroll mode
```

Button Aは通常 `A tap -> Next Slide` なので，競合回避のため以下の状態遷移を採用する．

```text
A down
    |
    +-- JoyCを操作せずAを離す
    |      -> Next Slide
    |
    +-- JoyCをscroll.dead_zone超えで操作する
           -> Scroll mode
           -> Next Slideをキャンセル
```

Aを押した瞬間には `RightArrow` を送信しない．

dead zoneの使い分けは以下とする．

```text
Pointer判定
-> profile.mouse.dead_zone

Scroll発火判定
-> active scroll.dead_zone

Scroll modeへ入る判定
-> active scroll.dead_zone
```

## 12. HID Mouse Report

スクロール対応に伴い，Mouse Reportを現在の3 byteから少なくとも4 byteへ拡張する．

現在の構造は以下である．

```text
buttons
X
Y
```

変更後の構造は以下とする．

```text
buttons
X
Y
wheel
```

ボタンビットは以下とする．

```text
bit 0 = Left
bit 1 = Right
bit 2 = Middle
```

値は以下とする．

```text
0x01 = Left
0x02 = Right
0x04 = Middle
```

縦スクロールにはHID `Wheel` を使用する．横スクロールを実装する場合は，別途 `AC Pan` をHID Report Descriptorへ追加する．

`ble.rs` は以下を責務に含める．

- HID Report Descriptorの定義
- Keyboard report送信
- Mouse report送信
- `Wheel` usageを含むMouse report形式の維持

既存ホストとの互換性確認では，Report IDを使っている前提で，Mouse Report IDとKeyboard Report IDの対応を崩さない．

## 13. スクロール設定

スクロール感度もTOMLから調整可能とする．

```toml
[scroll]
dead_zone = 12
speed = 8
horizontal = false
invert_vertical = false
invert_horizontal = false
```

マウスカーソルほど高頻度でreportを送る必要はないため，JoyCの倒し量をスクロール量へ変換する．

```text
弱く倒す
-> 1 step

中程度
-> 2 step

大きく倒す
-> 3から4 step
```

## 14. 画面スリープ

LCDは常時点灯させず，一定時間操作がなければバックライトをOFFにする．

```toml
[display]
timeout_seconds = 15
brightness = 40
```

状態遷移は以下とする．

```text
入力あり
   |
   v
LCD ON
   |
   v
15秒無操作
   |
   v
バックライトOFF
```

以下の入力または状態変化があれば即座に再点灯する．

- Button A
- Button B
- JoyC移動
- JoyC押し込み
- BLE接続状態変化

LCD OFF中もBLE HID，JoyC polling，ボタン入力は通常どおり動作する．画面スリープによってマウスやスライド操作の応答性を低下させない．

## 15. LCD表示

操作説明は常時表示しない．通常は実用的な状態情報を表示する．

```text
M5 PRESENTER

CONNECTED
Mac

Battery 82%
```

最低限表示する情報は以下とする．

- BLE接続状態
- 現在のプロファイル
- バッテリー残量

開発中のみ，必要であれば以下も表示可能とする．

- Mouse speed / gain
- JoyC状態
- BLE状態
- デバッグ情報

## 16. TOML設定の想定形

最終的には以下のような構造を目標とする．

```toml
[general]
default_profile = "linux"
profile_order = ["linux", "mac", "landscape"]

[display]
timeout_seconds = 15
brightness = 40
show_battery = true
show_connection = true
show_profile = true

[scroll]
dead_zone = 12
speed = 8
horizontal = false
invert_vertical = false
invert_horizontal = false

[input]
combo_window_ms = 80
tap_max_ms = 300
default_hold_ms = 600

[input.button_a]
tap = "RightArrow"
hold_joy = "Scroll"

[input.button_b]
tap = "LeftArrow"

[input.joy_click]
tap = "MouseLeft"

[[input.combo]]
buttons = ["button_a", "joy_click"]
action = "MouseRight"

[[input.combo]]
buttons = ["button_b", "joy_click"]
action = "MouseMiddle"

[[input.combo]]
buttons = ["button_a", "button_b"]
hold_ms = 800
action = "NextProfile"

[profiles.linux]
label = "Linux"
orientation = "normal"

[profiles.linux.mouse]
poll_hz = 125
dead_zone = 8
base_speed_px_per_sec = 300
max_speed_px_per_sec = 1000
gain_rise_per_sec = 6
gain_fall_per_sec = 6
curve_weight = 0.7
smoothing = 0.5
invert_x = false
invert_y = false

[profiles.mac]
label = "Mac"
orientation = "normal"

[profiles.mac.mouse]
poll_hz = 125
dead_zone = 8
base_speed_px_per_sec = 300
max_speed_px_per_sec = 1800
gain_rise_per_sec = 6
gain_fall_per_sec = 6
curve_weight = 0.7
smoothing = 0.5
invert_x = false
invert_y = false

[profiles.landscape]
label = "Landscape"
orientation = "right"

[profiles.landscape.mouse]
poll_hz = 125
dead_zone = 8
base_speed_px_per_sec = 300
max_speed_px_per_sec = 1200
gain_rise_per_sec = 6
gain_fall_per_sec = 6
curve_weight = 0.7
smoothing = 0.5
invert_x = false
invert_y = false

[profiles.landscape.scroll]
dead_zone = 14
speed = 8
horizontal = false
invert_vertical = false
invert_horizontal = false
```

## 17. 推奨デフォルト操作

初期設定としては以下を採用する．

```text
A tap
-> Next Slide

B tap
-> Previous Slide

JoyC
-> Pointer

Joy Click
-> Left Click

A + Joy Click
-> Right Click

B + Joy Click
-> Middle Click

A + JoyC Y
-> Vertical Scroll

A + B 長押し
-> Next Profile

C 長押し
-> Power ON / OFF
```

この操作体系では，通常使用するA，B，JoyCの即応性を維持しつつ，高度な操作をcomboへ割り当てられる．

## 18. 実装構成

入力処理をBLE処理から分離する．

```text
src/
├── main.rs
├── ble.rs
├── input.rs
├── mouse.rs
├── mini_joyc.rs
├── profile.rs
├── display.rs
├── battery.rs
├── bond_store.rs
└── config.rs
```

役割は以下とする．

```text
input.rs
-> tap / hold / combo / scroll mode判定

mouse.rs
-> JoyCからPointer / Scroll量への変換

profile.rs
-> 現在プロファイルと切り替え

ble.rs
-> HID Report DescriptorとKeyboard / Mouse report送信

display.rs
-> LCD状態表示，現在プロファイル表示，画面スリープ

config.rs
-> presenter.tomlから生成された設定
```

入力レイヤーとHIDレイヤーを分離することで，キー割り当てやcomboを変更してもBLE処理へ影響しにくい構造とする．

## 19. 実装の推奨順序

実装は以下の順に進める．

1. `config.rs` と `build.rs` で，`presenter.toml` の新構造を読み込めるようにする．
2. `profile.rs` を追加し，`general.default_profile` と `general.profile_order` に基づいて現在プロファイルを管理する．
3. `input.rs` を追加し，tap，hold，combo，scroll modeの状態機械を実装する．
4. `mouse.rs` に `orientation` とscroll変換を追加する．
5. `ble.rs` のMouse Reportを4 byte化し，`Wheel` usageをHID Report Descriptorへ追加する．
6. `display.rs` に現在プロファイル表示，LCD回転，画面スリープを追加する．
7. 選択中プロファイルのFlash保存を追加する．

最初からFlash保存まで含める必要はない．入力仕様とプロファイル選択の境界を先に固める方が，後続の変更を小さくできる．
