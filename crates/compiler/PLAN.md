# compiler — 今後のタスク

現状のアーキテクチャ・カバレッジは [AGENTS.md](../../AGENTS.md) の
"Compiler pipeline" 節を参照。ここには未実装の作業だけを置く。

## 残タスク (優先順)

1. **case payload に nested option/result/variant** — disc-branch copy の
   `case_flat_slot_offsets` は primitive / string / list / record / tuple /
   enum / flags / handle のみ対応。case の payload に option<T> や
   result<T,E> や variant が来た時は disc 内蔵なので別途対応が必要。
2. **WIT 識別子の自動 kebab-case 化** — record の field 名や型 uid に
   underscore があると wit-component が拒否する。`format_wit_type` /
   `synthesize_world` は uid を `wit_name` で kebab-case 化しているが、
   record の field 名は素通し。field 名と uid の両方で `_` → `-` に統一する。
