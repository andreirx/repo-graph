# ts-extractor — agent notes

- A bare import that is type-only or a re-export produces no edge and no unresolved row. A per-file remainder cannot show what was never extracted. (src/extractor.rs:1525-1527, as of 36cc1367; source: RC-11)
- The extractor resolves relative specifiers itself. It emits a bare specifier with the raw text as `target_key`; the indexer's ladder resolves it. (src/extractor.rs:1451, :1527, :1811; source: RC-2)
