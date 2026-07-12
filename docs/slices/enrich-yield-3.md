# ENRICH-YIELD-3 — Rust receiver-expression locator, then admit self.field.method

Status: SPECIFIED (2026-07-12, queued after ENRICH-YIELD-2) · Track: Resolution & attribution
Origin: EY1-C ratified (operator 2026-07-12, corrected cell): the naive parse_method_name
admission is UNSAFE — the Rust resolver hovers the stored call-expression start (the `self`
token), so it would resolve `self`'s type, not `self.field`'s → FALSE Layer-0 CALLS edges.
The TS path is safe only because tsserver-resolver has a tree-sitter ReceiverLocator.

## 1. Contract

1. Build a Rust receiver-expression locator mirroring the TS ReceiverLocator seam: for
   `self.field.method()` (and `this.field.method`-equivalent shapes routed by category),
   locate the INTERMEDIATE receiver expression (`self.field`), query rust-analyzer at ITS
   position, and use THAT type for promotion.
2. A named test PROVING the field's type (not self's) drives promotion (fixture where they
   differ — the false-edge scenario EY1-C documented).
3. Only THEN admit `self.field.method` in gate 8's parse_method_name. Deep chains
   (`a.b.c.d`) stay rejected — ratified.

## 2. Stop conditions

Locator mirrors the existing TS seam (no new architecture); deep chains stay rejected; the
enrichment pass's batch/cancellation semantics untouched. Do NOT commit.

## 3. Validation

Cargo gates; the false-edge named test; isolated self-dogfood funnel BEFORE/AFTER — the
~140 self.field.method rejections convert to promotions ONLY where the located receiver
type resolves uniquely (cite examples); no other gate's counts regress.
