# rmap-on-rmap modeling experiment — 2026-09-04

See ROADMAP section "rmap-on-rmap modeling experiment" for the verdict, the five
modeling surfaces, and the two live defects. Full model (module table, data-flow map,
access-pattern table, gap table) published at https://claude.ai/code/artifact/a7630d1b-6c7c-41a6-868e-4536a545b471;
captures at /private/tmp/selfmodel/captures (isolated v0.16.0 root) at run time.
Probe corrections: `find --text 'a|b'` alternation works (201 hits) — the gap-table row
G4's "no regex/alternation" was an operator quoting error (`\|`); find's printed cursors
carry shell quotes — strip before passing programmatically.
