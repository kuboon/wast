# partial-manager — Extract/Merge Component

## Purpose

Manages extraction of partial WastComponents from full components and
merging partials back. The partial is the unit a syntax plugin renders
and the user edits.

## Interfaces

Exports: `partial-manager` (extract, merge)

## Key Responsibilities

### extract(full, targets) → partial

Build a partial WastComponent (`B`) from `full` (`A`) for the given
target funcs.

**Inclusion** — which funcs end up in `B`:
- Every `target` exists in `B`.
- If any target has `include_caller: true`, the *direct* callers of that
  target in `A` are pulled in too.
- For each func in `B` that has a body, its callees are added to `B` as
  signature-only stubs (one level — stubs don't pull more callees in).

**Source assignment**:
- Target *without* `include_caller` → forced to **`Exported(uid)`**. The
  partial has no proof that all callers of this func are visible (a
  caller that *happens* to also be a target doesn't establish "no caller
  exists outside `B`"). Locking the signature is the safe default;
  `merge` enforces it against `full`'s callers.
- Target *with* `include_caller` → keep its original source from `A`.
  The flag is the user's promise that all callers have been pulled in,
  so the signature can be edited and the syntax plugin alone can verify
  call-site consistency.
- Pulled-in caller (added because some target had `include_caller`) →
  keep original source from `A`, **keep body**. The body is needed for
  the syntax plugin's type check.
- Pulled-in callee (signature-only stub) → **`Imported(uid)`** with
  `body = None`.

Why `B`-membership is *not* the rule: even if `square` is called by
`poly` inside `B`, that doesn't mean every caller of `square` is in `B`
— `cube` (also in `full`) might also call `square`. Without the
`include_caller` flag, the partial cannot claim the caller list is
complete, so `square`'s signature must stay locked (`Exported`).

**Behaviour on signature-mismatch in `A`** — outputs invalid type refs
as-is (post-WIT-change recovery).

### merge(partial, full) → full | errors

- Match `partial`'s `Imported(uid)` / `Exported(uid)` against `full`,
  verifying signature compatibility.
- For `partial`'s `Internal(uid)`, replace or add the func in `full`
  (uid_conflict if `full` has a non-internal entry under the same uid).
- Error on missing dependencies, signature mismatches, or uid conflicts.

## Dependencies

- `wit/wast-core.wit`

## Testing Strategy

- Extract: source assignment cases (no-caller-in-B → Exported;
  caller-in-B → original source; pulled-in callee → Imported).
- Extract: include_caller pulls callers; transitive type refs.
- Merge: happy path; errors for `signature_mismatch`,
  `missing_dependency`, `uid_conflict`.

## Status

Implemented. The web-demo plugin showcase exercises both extract and
merge end-to-end (see `packages/web-demo/src/main.js`).
