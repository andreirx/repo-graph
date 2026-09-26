# D-TS-WORKSPACE-1 — Does the certainty rule supersede the IMPORTS-WORKSPACE-PACKAGE-EDGE-1 no-go?

Raised: 2026-09-23 by the independent review of RG-BOOTSTRAP-INPUT-8: FRAKTAG `packages/api/src/server.ts:6` `import { Fraktag } from '@fraktag/engine'` names a workspace member whose declared entry (`main: dist/index.js`) is build output the index never holds, while its source entry `packages/engine/src/index.ts` is indexed; rmap stamps the import external and calls `packages/engine` isolated (60 such imports on amodx). IMPORTS-WORKSPACE-PACKAGE-EDGE-1 ratified a no-go on the `src/index.ts` convention absent verified evidence; the operator may not supersede a ratified record.
Resolved: 2026-09-26 by the HUMAN — option A.

## Options (as presented)
- **A (ratified)** — the import resolves to the member's indexed source entry as INFERRED, recording both candidates (source entry, declared entry) and the reason; default module views exclude it and state "+N inferred imports — `--include-inferred`"; the LiveGraph route keeps its label and says so. Reward: the workspace relationship becomes visible and investigable; nothing certain is asserted. Risk: the two backends answer differently until a shared rule exists.
- **B** — keep the no-go. Reward: parity. Risk: `packages/engine` and `@amodx/shared` keep reading as isolated.
- **C** — require declared build evidence (tsconfig `rootDir`/`outDir`, `exports` maps). Reward: evidence-bound. Risk: new plumbing; unresolved where the evidence is absent (FRAKTAG has none).
