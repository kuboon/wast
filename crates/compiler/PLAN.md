# compiler — wast → wasm Component コンパイラ

**このドキュメントはセッション引き継ぎ用**。前回までの議論の結論と v0 実装計画をまとめている。

## 現状

- **未実装**。`crates/compiler/` ディレクトリも未作成
- compiler は最優先タスク。wast プロジェクトの**一番やりたいこと**
- 他の crate（file-manager, partial-manager, pattern-analyzer）は実装済み
- 直前のセッションで `wast.db` → `wast.json` のリネーム + JSON 構造の row 指向への再設計が完了（している想定 — 未完了なら先にそれを終わらせる）

## 方針（確定事項）

### compilation target / toolchain

- **出力**: WASM Component バイナリ（`.wasm`）
- **手段**: Component WAT テキストを自前で組み立て → [`wat`](https://crates.io/crates/wat) crate で `.wasm` に変換
- **`wit-component` は不使用**。Component 外殻（`(component ...)` + `canon lift` + 内側 component wrapping）も自前で WAT に書き出す
- **`wasm-encoder` も不使用**（当面）。WAT テキスト経由のほうが可読性・デバッグ性が高い

### 採用しない選択肢とその理由

- ❌ core WASM → `wit-component` で Component 化: 外部ツールに依存するが `wit-component` は validator/packager に近く、Canonical ABI の実装は結局 compiler 側でやる必要がある。透明性のため自前で書く
- ❌ `wasm-encoder` 直接: 型安全だがバイナリ出力を後から `wasm-tools print` で WAT 化してデバッグする必要あり。最初から WAT で書くほうが開発効率が良い
- ❌ LLVM バックエンド: overkill、起動コスト大

### crate の位置づけ

- **v0: plain Rust library crate**（`rlib`）
- `file-manager`（および `file-manager-hosted`）から関数コールで呼ばれる純粋関数
- **将来的に wasm component 化予定**（swap 不要なので機械的ラップで済む）
- `-hosted` suffix は付けない（syntax-plugin のようにプラグイン差し替えする予定がないため）

### Component Model 基礎知識（次セッション向け）

- **core WASM と WASM Component は別フォーマット**（ファイルヘッダで区別）
- core WASM: `i32/i64/f32/f64` のみ、関数・メモリ・テーブル等のプリミティブ
- Component: core モジュールを `(component ...)` で包み、WIT 型（string/list/record/variant/option/result/resource）を公開
- **Canonical ABI** = WIT 型 ↔ core WASM メモリ表現の変換規約。core module 側が従う必要がある
- compiler の作業 = **Canonical ABI 規約に従った core .wasm の生成 + Component 外殻の組み立て**

### pattern-analyzer の役割（次セッション向け）

`crates/syntax-plugin/internal/pattern-analyzer/` は名前に反して**プラグイン非依存の共通ライブラリ**:
1. body の IR 定義（`Instruction` enum）
2. body の serialize/deserialize（postcard バイナリ）
3. 高レベル制御構文の検出（text 復元用）

compiler は (1) と (2) を利用する。(3) は不要（WASM 生成時は `Loop + BrIf` のまま WAT 命令にマップするだけ）。

## 前提となる作業（compiler 開始前に必要）

### 案 B: `wast-types` crate の抽出（⚠️ 未着手・必須）

現状、serde 型（`WastDb`, `WastFunc`, `WastTypeDef`, `WitType`, `PrimitiveType`, `FuncSource`, `TypeSource`, `Syms`, `SymEntry`）が以下 **2 箇所で重複**している:

- `crates/file-manager/src/serde_types.rs`
- `crates/file-manager-hosted/src/serde_types.rs`

compiler も同じ型を扱うので、3 箇所目を作らないため **事前に `crates/wast-types/` を新設**し、両既存 crate を移行する。

**手順**:
1. `crates/wast-types/` 新設（plain rlib）
2. 既存 2 crate の `serde_types.rs` の内容を移動
3. 既存 2 crate を `wast-types` 依存に切り替え、`use wast_types::WastDb;` 等に書き換え
4. WIT bindings ↔ native types の変換関数（`db_to_binding` など）は各 crate に残す
5. workspace テスト全通過を確認

## v0 スコープ: WASI CLI empty run

### 入力
- `world.wit`: WASI CLI 0.2.0 を include する world
- `WastComponent`: `wasi:cli/run@0.2.0` interface の `run` 関数を 1 つ export、body は `Return` のみ（`result<_, _>` で常に ok）

### 出力（期待する Component WAT）

```wat
(component
  (core module $Mod
    (func (export "mod-main") (result i32)
      (i32.const 0)))
  (core instance $m (instantiate $Mod))
  (func $main_lifted (result (result))
    (canon lift (core func $m "mod-main")))
  (component $Comp
    (import "main" (func $g (result (result))))
    (export "run" (func $g)))
  (instance $c (instantiate $Comp
      (with "main" (func $main_lifted))))
  (export "wasi:cli/run@0.2.0" (instance $c)))
```

### 検証
- `wat::parse_str(...)` → `.wasm` バイナリ生成成功
- `wasmtime run output.wasm` → exit code 0
- 統合テスト `tests/v0_smoke.rs`

### 学習ポイント
- `(component ...)` 外殻の構造
- `canon lift` 宣言（unit 型なので最小構成）
- WASI CLI 向け内側 component wrapping パターン

### 学ばないこと（v0 ではスコープ外）
- Canonical ABI の実体（unit 型同士なので実質 passthrough）
- core 命令列の本格生成（`i32.const 0` 固定）

## v0.1: `u32 -> u32` 恒等関数

### 入力
- `world.wit`:
  ```wit
  package example:foo@0.1.0;
  world t {
    export identity: func(x: u32) -> u32;
  }
  ```
- `WastComponent`: `identity` 関数、body は param を `LocalGet` して `Return`

### 期待出力
```wat
(component
  (core module $Mod
    (func (export "identity") (param i32) (result i32)
      local.get 0))
  (core instance $m (instantiate $Mod))
  (func $identity_lifted (param "x" u32) (result u32)
    (canon lift (core func $m "identity")))
  (export "identity" (func $identity_lifted)))
```

### 検証
- Rust テストハーネスで `wasmtime::component::Component` をロード → `identity(42)` 呼び出し → `42` が返る

### 学習ポイント
- IR `LocalGet { uid }` → core `local.get N`（param index への変換）
- プリミティブ型の canonical ABI（ほぼ passthrough）
- Rust からの Component インスタンス化と呼び出し

## 以降のロードマップ

v0 / v0.1 完了後、以下を 1 つずつ段階的に実装・検証:

1. 数値型拡張: `i32/i64/f32/f64/u32/u64/bool` の `Const`, `Arithmetic`, `Compare`
2. `Call`（Internal / Imported / Exported）
3. 制御フロー: `If`, `Loop`, `Block`, `Br`, `BrIf`
4. `Option` / `Result` 型（variant の単純ケース）
5. `string` — realloc + memory エクスポートが必要。`cabi_realloc` 実装
6. `list<T>`
7. `record`
8. `variant`（一般ケース）
9. `tuple`
10. `resource` 型（handle）

各ステップで **wasmtime で実際に動かして検証**。シンプルな型から順に積み上げ、回帰テストで退行を防ぐ。

## crate スケルトン

```
crates/compiler/
  Cargo.toml
  PLAN.md          ← このドキュメント
  src/
    lib.rs         # pub fn compile(...) -> Result<Vec<u8>, Error>
    emit.rs        # Component WAT 文字列組み立て
    core_emit.rs   # core module 内の命令列生成（IR → core WAT）
    error.rs       # CompileError
  tests/
    v0_smoke.rs    # WASI CLI empty run 統合テスト
```

### Cargo.toml 案

```toml
[package]
name = "wast-compiler"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
wast-types = { path = "../wast-types" }
wast-pattern-analyzer = { path = "../syntax-plugin/internal/pattern-analyzer" }
wat = "1"
postcard = { version = "1", features = ["alloc"] }

[dev-dependencies]
wasmtime = { version = "...", features = ["component-model"] }
```

### API 案

```rust
pub fn compile(
    component: &wast_types::WastComponent,
    world_wit: &str,
) -> Result<Vec<u8>, CompileError>;
```

## 次セッションの着手順

1. **（前作業確認）** `wast.db` → `wast.json` リネームと JSON row 指向化が完了しているか確認
2. **`crates/wast-types/` 新設**（案 B 実施）
3. **`crates/compiler/` 骨組み作成**（Cargo.toml, lib.rs, emit.rs の空枠）
4. **v0 emitter**: 固定 WAT を返すだけの `compile()` を実装（入力はまだ見ない）
5. **v0 統合テスト**: `wasmtime run` で exit 0 を確認
6. **v0.1 に進む**: `u32 -> u32` を入力 IR から動的生成できるように拡張

各ステップで必ずテストを書き、緑のまま進む。

## 参考: Canonical ABI のポイント（深掘り時）

- プリミティブ型（i32/u32/i64/u64/f32/f64/bool）: core 型に直接マップ、ABI 変換なし
- `char`: core `i32`（Unicode scalar value）
- `string`: `(i32 ptr, i32 len)` 2 つの引数/戻り値に展開、memory とリンク
- `list<T>`: `(i32 ptr, i32 len)`、T が fixed-size でないと再帰的に計算
- `record`: 各フィールドを平坦化、またはメモリ経由
- `variant`: `i32 discriminant` + ペイロード
- `option<T>`: `variant { none, some(T) }` として扱う
- `result<T, E>`: `variant { ok(T), err(E) }`
- Returning non-primitive: realloc を通じて呼び出し側のメモリに書き込む

詳細は [Component Model Canonical ABI spec](https://github.com/WebAssembly/component-model/blob/main/design/mvp/CanonicalABI.md) を参照。

## コンテキスト: なぜ WAT 直接か

前セッションでの結論:

- WAT と wasm-encoder はどちらも core WASM 生成手段。どちらも WIT ネイティブではない
- 最終的な Component 化の作業（`canon lift` / 外殻） は WAT テキストで直接表現できる
- `wit-component` はほぼ packager なので、自前で WAT を組み立てれば不要
- WAT 出力は目視・diff・スナップショットテストしやすい
- デバッグ時に「このバイト列は何？」を悩まなくて済む
