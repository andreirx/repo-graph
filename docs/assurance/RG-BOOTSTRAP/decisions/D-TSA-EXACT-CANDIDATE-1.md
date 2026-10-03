# D-TSA-EXACT-CANDIDATE-1 — a `paths` target written with its extension is probed as itself before the shared candidate list

Raised: 2026-10-03 by the document review of TS-ALIAS-RESOLUTION-1 INPUT-2 (cycle 1; reviewer codex gpt-6-sol; F-TSA2-03).

**The problem in plain words.** `repo-graph-import-resolver`'s `candidate_paths` (`lib.rs:246-267`) appends the TS extensions and `/index.<ext>` to a path and never tries the path itself, so a `paths` value such as `"./src/foo/one.ts"` (valid per TypeScript's module-resolution reference) reaches no candidate and the import stays unresolved although the compiler binds it. The slice reuses `candidate_paths` for every substitution (D-TSA-RESOLVER-EDGE-1: one matcher). The same gap exists today for a relative import written with its extension (`import "./x.ts"`), on both routes.

**Options (reward / risk, by usefulness to an agent net of misdirection).**
- A — inside `tsconfig_alias_hits` only: when a substituted path ends in one of the candidate extensions (`.ts .tsx .d.ts .mts .cts .js .jsx .mjs .cjs .json` as `candidate_paths` lists them), the path itself is tried first, then the shared list; `candidate_paths` and the LiveGraph route unchanged. Reward: a `paths` target written with its extension binds static as the compiler binds it; the slice's preserved LiveGraph behaviour stays byte-identical; one candidate-list implementation (the exact probe is a prefix to the shared list, not a second list). Risk: relative imports written with an extension stay unresolved on both routes until the follow-up.
- B — add the exact probe to `candidate_paths` itself, for both routes and for relative imports too. Reward: the compiler's behaviour everywhere. Risk: changes the LiveGraph route and every relative-import binding inside a slice whose preserved obligations and before-root predictions assume them unchanged; unmeasured movement.
- C — leave the gap. Risk: a valid `paths` target is reported as unresolved: the agent is told a file is missing that the compiler finds.

Resolved: 2026-10-03 by the OPERATOR (in-place manager) as **A**, overridable by the human. Follow-up filed: EXPLICIT-EXTENSION-IMPORT-1 (option B as its own measured slice on both routes). The slice binds a test with an explicit-extension `paths` target.
