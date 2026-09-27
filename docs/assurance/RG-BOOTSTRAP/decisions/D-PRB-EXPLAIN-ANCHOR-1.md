# D-PRB-EXPLAIN-ANCHOR-1 — explain's caller rows keep declaration lines, labelled; call sites through the LiveGraph are a follow-up

Raised: 2026-09-27 by the builder of PYTHON-RECEIVER-BINDING-1 (INPUT-4, admission 3, build-1). The builder was answering implementation review-0 F-1 (reviewer codex gpt-6-sol), which found that `callers` and `explain` give different, unlabelled lines for the same calls. django: `rmap callers ListMixin.extend` anchors `django/contrib/gis/geos/mutable_list.py:123` (`self.extend(other)`), while `explain` cites `:121`, the line where `__iadd__` is defined.

**Why the lines cannot simply be aligned.** On a green certificate, `explain` is served from the LiveGraph for TypeScript. The LiveGraph's IR edge (`repo-graph-ir` `IrEdge`: src, dst, edge_type, basis, provenance, import) carries no source location. `callgraph_cert::lg_caller_rows` therefore builds explain's caller rows from the caller's declaration (`symbol_context`). Aligning explain with `callers` on every backend needs a data-shape change across the LiveGraph/IR boundary, which this slice freezes.

Resolved: 2026-09-27 by the OPERATOR (in-place-manager): option A. The human may override.

## Options (the builder's)
- **A (ratified): the candidate as built.** `explain`'s Callers rows keep each caller's declaration line and say so inline: `lines above are where each caller is defined — for the call sites run rmap callers <name>`. Reward: honest on both backends now; a change to the CLI's rendering only; reversible. Risk: `explain` still points the agent at the declaration.
- **B: carry call-site ranges through the LiveGraph IR**, then anchor explain from the edge line everywhere. Reward: identical call-site anchors on every surface and backend. Risk: a boundary data-shape change; it needs its own slice.
- **C: align on SQLite only.** Risk: `explain`'s lines would depend on the backend, breaking ANCHORS-EVERYWHERE-1's LiveGraph = SQLite anchor parity.
- **D: serve explain's Callers from SQLite even on green.** Risk: reverses the ratified LiveGraph-on-green serving and the nodes-free explain invariant.

## Basis
- **RG-REQ-005-L08** requires the call site for `callers`/`callees` rows, which the slice delivers. For explain and the other surfaces it requires that "every symbol citation … renders a line from one store". A caller's declaration line is such a citation.
- **RG-REQ-002-L02** allows surfaces to differ when the answer states the differing basis inline, which A does.
- Implementation review-2 accepted the candidate with this ruling in its inputs.

## Follow-up
EXPLAIN-CALLSITE-ANCHOR-1 (option B), to be queued by the human.
