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

---

## 4. Delivery record (2026-07-13)

**DELIVERED** (relay-approved iteration 1 — 2 cycles + 2 ratifications). EY3-ROUTING:
shape-based routing ratified (the packet's "category-routed" premise was a TS-mirror
assumption; Rust's categorizer assigns object-method to self.*). EY3-FUNNEL: live proof
required and delivered — identical 16k corpus, promoted 623 → 630 (+7 cited internal-type
conversions, zero lost), −266 gate-8 / +267 gate-4: the locator resolves the FIELD's type,
which is usually external — the admission converts false-edge RISK into honest attribution,
exactly the EY1-C corrected cell's intent. False-edge integration test (Inner-not-Outer)
green against real rust-analyzer. The ENRICH-YIELD arc (1: measure → 2: safe levers →
3: locator) is complete.
