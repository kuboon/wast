# compiler — wast → wasm Component コンパイラ

**現状: v0.16 完了**。numeric / control flow / calls / option / result / string / list / record まで端から端まで動く。以降は variant (一般ケース) / tuple / resource / nested compound / ListLiteral。

## 全体アーキテクチャ (v0.11 以降)

```
WastDb + 合成 WIT world
    ↓
  emit_core_module  (core-only WAT)
    ↓  wat::parse_str
  core .wasm バイナリ
    ↓  wit_component::embed_component_metadata (WIT 世界を custom section に埋込)
    ↓  wit_component::ComponentEncoder::module(...).encode()
  Component .wasm バイナリ
```

**外殻 (`(component …)`, `canon lift`, `canon lower`, memory option, with-instance 配線) は全部 wit-component に任せる**。こちらは core module だけを作る。

wit-component / wit-parser は 0.219 に pin (wasmtime 27 と wasmparser バージョンを合わせるため)。

## Core module の中身

- `(memory (export "memory") 1)` 1 ページ
- `(global $heap_end (mut i32) (i32.const X))` bump allocator のトップ。X は static data (literal) の終端
- `(func $cabi_realloc (export "cabi_realloc") …)` bump allocator。`memory.copy` で realloc-grow に対応
- `(data (i32.const OFF) "\HH…")` StringLiteral を連続配置 (重複排除、`STATIC_DATA_BASE=1024` 開始)
- `(import "$root" "NAME" (func $NAME …))` FuncSource::Imported のぶん
- `(func $UID (export "NAME") …)` 各 exported / internal 関数
  - core 署名: 各 WIT 型の flat 形を連結 (primitive=1スロット、compound=2+スロット)
  - compound 戻り値 (flat > `MAX_FLAT_RESULTS=1`) は indirect return: `(result i32)` (ポインタ返し)

## IR (pattern-analyzer) と emit 対応表

| IR | 意味 | core emit |
|---|---|---|
| `Nop` / `Return` | | `nop` / `return` |
| `Const { value }` | i64 リテラル、型はコンテキスト推論 | `<core>.const <value>` |
| `LocalGet { uid }` | param/local 読み出し | 1+ slots ぶん `local.get N` |
| `LocalSet { uid, value }` | local 書き込み (compound 不可) | `<value>; local.set N` |
| `Arithmetic` / `Compare` | 型推論から signed/float を切替 | `<t>.add` / `<t>.lt_s` 等 |
| `Call { func_uid, args }` | args を callee param 順に並べる | `<args>; call $func_uid` |
| `If / Block / Loop / Br / BrIf` | 制御フロー | `if / block / loop` + ラベル |
| `Some / None / Ok / Err` | (return-position のみ) indirect return wrap | alloc 8B + disc + payload store |
| `IsErr { LocalGet }` | result param の disc スロットを読み出し | `local.get disc_idx` |
| `MatchOption / MatchResult` | 分岐 + binding | `local.set binding; if`で分岐 |
| `StringLiteral { bytes }` | データセグメントに埋込んだ定数 | `i32.const <offset>; i32.const <len>` |
| `StringLen { LocalGet | StringLiteral }` | byte 長取得 (literal は compile-time fold) | `local.get len_idx` |
| `ListLen { LocalGet }` | element 数取得 | `local.get len_idx` |
| `RecordGet { LocalGet, field }` | フィールド読み出し | `local.get base+slot_offset` |
| `RecordLiteral { fields }` | (return-position のみ) コンストラクタ | alloc + per-field store |

## 型解決 (`ResolvedType`)

```rust
enum ResolvedType {
    Primitive(String),          // u32/u64/i32/i64/f32/f64/bool/char
    String,                     // (ptr, len)   size/align (8, 4)
    List(String),               // (ptr, len)   size/align (8, 4)
    Option(String),             // 2 slots (disc, payload)
    Result(String, String),     // 2 slots (disc, join<ok,err>)
    Record(Vec<(String, String)>), // concat of fields' flats
}
```

`flat_slots` / `size_align` / `format_wit_type` / `lifted_type_wat` などが `ResolvedType` で分岐。`TypeMap` (`HashMap<uid, &WitType>`) を compile 開始時に構築。

## 指示 return の wrap

関数戻り値が indirect の時、`emit_body` は body の最後の命令を切り出して wrap する:

- **PtrLen wrap** (string / list): alloc 8B → `(ptr, len)` を offset 0/4 に store → buffer ptr を返す。値源: LocalGet(string/list) または StringLiteral
- **Record wrap**: alloc size(record) → 各フィールドを byte offset に store → buffer ptr を返す。値源: RecordLiteral のみ

## ret_ptr_slot

Compound 戻り or body 内に Some/None/Ok/Err があるとき、**param+local の末尾に `i32` local を 1 個予約**して buffer pointer 保持に使う。`emit_core_func` で判定。

## WIT world 合成 (`synthesize_world`)

- `package wast:generated;` + `world generated { … }`
- Record 型は `record NAME { fields }` を world 内で宣言
- 他の型参照 (option/result/list/string) は use site でインライン展開
- Func source に応じて `export NAME: func(…) -> …;` / `import NAME: func(…) -> …;` を emit
- Internal func は WIT に出さない (core module 内部のみ)
- `synthesize_world` → `wit_parser::Resolve::push_str` → `embed_component_metadata` → `ComponentEncoder`

## マイルストーン表

| ver | スコープ | 追加 |
|---|---|---|
| v0 | WASI CLI 空 run 固定 WAT | 定数 WAT 返すだけ |
| v0.1 | identity(u32) -> u32 | 動的 core WAT 生成 |
| v0.2 | Arithmetic / Compare 全数値型 | signedness 対応 |
| v0.3 | internal Call | FuncMap、引数並べ替え |
| v0.4 | If/Block/Loop/Br/BrIf + LocalSet | 関数スコープ local 自動登録 |
| v0.5 | imported Call | canon lower 経由 (当時は手書き; v0.11 で wit-component 任せに) |
| v0.6 | option/result **param** + IsErr | flat layout、2 スロット展開 |
| v0.7 | memory + cabi_realloc 基盤 | bump allocator |
| v0.8 | option/result **return** | indirect-return wrap、variant layout |
| v0.9 | MatchOption / MatchResult | binding を local に `local.set/local.tee` |
| v0.10 | **wit-component spike** | 手書き外殻が不要と判明 |
| v0.11 | **core-only emit + wit-component wrap** | 大幅に行数減、canon lower 循環参照も解決 |
| v0.12 | string **param** + StringLen | flat 2 slot |
| v0.13 | StringLiteral + data segments | 定数配置、compile-time fold |
| v0.14 | string **return** | ptr-len 汎用 wrap |
| v0.15 | list<T> param/return + ListLen | string と同じ構造を再利用 |
| v0.16 | record (primitive fields) | flat 連結、バイトオフセット配置 |
| v0.17 | 一般 variant (N ケース、payload optional) | VariantCtor / MatchVariant、option/result は専用 IR のまま |
| v0.18 | tuple (位置インデックスの無名 record) | TupleGet / TupleLiteral、layout は record と同じ、WIT は inline |
| v0.19 | char / enum / flags | char は既存 primitive で動作確認、enum = payload-less variant (VariantCtor/MatchVariant 流用)、flags は bitmask (FlagsCtor、≤32 bits → i32) |

## 残タスク (優先順)

1. **nested compound** — record のフィールドが string/list/record/option、option<string>、list<record>、等。`emit_record_return_wrap` で primitive 以外の field store を扱えるようにする
2. **ListLiteral** — 実行時に list を構築。要素数ぶん realloc + 各要素 store ループ。cabi_realloc の grow パスを本格に使う初ケース
3. **resource** — ハンドル型。最大の難物。`own<T>`/`borrow<T>` + resource テーブル + ドロップ関数

## 設計原則 (変わらず)

- IR は**高レベル意味表現**を保つ (core opcode 列ではない)。syntax plugin との往復のため
- 型定義は Component Model spec の構造に寄せる、body IR は寄せない (memory: WAST binary alignment — `project_wast_binary_spec_alignment.md`)
- wit-component で済むことは wit-component に任せる、手書き WAT は core module の body 命令列のみ (memory: `project_moonbit_rejected.md`)

## 外部依存

- `wat = "1"` — WAT テキスト → core wasm バイナリ
- `wit-component = "0.219"` — Component 外殻合成 + custom section 埋込
- `wit-parser = "0.219"` — WIT 世界文字列のパース
- `wasmtime = "27"` (dev-dependency のみ) — テストで component を実行
- `wasmtime-wasi = "27"` (dev-dependency) — WASI CLI smoke test 用
