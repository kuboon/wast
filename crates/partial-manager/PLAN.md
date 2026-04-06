# partial-manager — Extract/Merge Component

## Purpose

Manages extraction of partial WastComponents from full components and merging partials back.

## Interfaces

Exports: `partial-manager` (extract, merge)

## Key Responsibilities

### extract
- Cut out funcs/types/syms related to target symbols from full component
- `include-caller: false` → callers become `imported(uid)`
- `include-caller: true` → callers remain `internal(uid)`
- WIT exported functions always stay `exported(uid)`
- Handles post-WIT-change signature mismatches gracefully (outputs invalid type refs as-is)

### merge
- Match partial's `imported(uid)` / `exported(uid)` against full component
- Verify signature compatibility
- Add new `internal(uid)` entries to full
- Error on missing dependencies or UID conflicts

## Dependencies

- `wit/wast-core.wit`

## Testing Strategy

- Extract with/without include-caller
- Merge happy path
- Merge error cases: signature_mismatch, missing_dependency, uid_conflict

## Status

Not started.
