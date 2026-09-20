# COMPLEXITY-SCOPE-1 — oracle corrections ledger

Text-only corrections to check oracles in `docs/slices/complexity-scope-1.md`, each carried as the next INPUT-n baseline with a document item (procedure: agent-manager docs/MANAGER.md § Oracle corrections). Append-only.

## OC-1 (2026-09-20) — CS-C12 asserted a directory, not the fact (→ INPUT-2)

- **Defect (in the INPUT-1 oracle, not the product):** CS-C12's full-section sub-assertion `! sed -n '/^Complexity centers (by cyclomatic complexity)/,/^$/p' … | grep -qE 'codegraph-kernel/grammars/|__tests__/'` required that NO row under `codegraph-kernel/grammars/` survive the production filter. RG-REQ-009-L01 promises fact-based exclusion ("a file distinguishable by no persisted fact stays ranked"); tree-sitter EXTERNAL SCANNERS under that directory are hand-written (`codegraph-kernel/grammars/scala/scanner.c` begins `#include "tree_sitter/alloc.h"` — no banner; `tree_sitter_scala_external_scanner_scan (cx 83)` at :295; `kotlin/scanner.c scan_automatic_semicolon (cx 38)`), carry `is_generated = 0`, and correctly stay ranked. The first admission's candidate failed CS-C12 on this sub-assertion alone (every other assertion in the check passed: the five generated files, the new headline, `--include-all`, the seam).
- **Correction:** the negative grep becomes `'grammars/[a-z_]+/parser\.c|__tests__/'` — the generated parsers by name (every `__tests__/` row above threshold carries `is_test = 1` on the fresh store, read 2026-09-20). Inputs text states the scanner residual.
- **Basis:** codegraph after-store rows and the checkout, read by the manager 2026-09-20; the builder's finding in the cycle-1 evidence and the codex gpt-5.6-terra review's decision D-CS-001 (docs/assurance/RG-BOOTSTRAP/decisions/D-CS-001.md, operator resolution: option 1) of the INPUT-1 admission (.agent-manager/slices/COMPLEXITY-SCOPE-1.superseded-INPUT-1/).

## OC-2 (2026-09-20) — CS-C13 asserted `/testsuite/`, a directory with no persisted fact (→ INPUT-2)

- **Defect (in the INPUT-1 oracle):** CS-C13's full-section sub-assertion included `/testsuite/`. poco's 788 `*/testsuite/*` files are CppUnit; the shipped structural test detector (RG-REQ-001-L07, gtest markers) sets `is_test = 0` for all 788 on the before AND after stores. `Foundation/testsuite/src/TestApp.cpp:240 int main(int argc, char** argv)` (cx 42) carries no persisted exclusion fact and correctly stays ranked; this slice preserves RG-REQ-001-L07 and does not change the detector.
- **Correction:** the negative grep becomes `'dependencies/|bison_parser'`. Inputs text states the CppUnit residual; follow-up IS-TEST-CPPUNIT-1 filed in §8.
- **Basis:** poco before/after stores (`select count(*), sum(is_test) from files where path like '%/testsuite/%'` → 788, 0 on both), read by the manager 2026-09-20.
