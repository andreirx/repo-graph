# D-DU-CC10 — What does a document whose bytes cannot be read look like in `docs list --json`?

Raised: 2026-09-23 by three document reviews of DOCS-UNREADABLE-DECODE-1 (codex gpt-5.6-terra): RG-REQ-008-L05 admits and counts a document whose bytes cannot be read; RG-REQ-008-L08 requires `content_hash` on every JSON entry; no truthful hash exists for such a document. (The poco crash — `packaging/README.txt` is Windows-1252 — is fixed by hashing the bytes and decoding the text separately; that part needed no ruling.)
Resolved: 2026-09-23 by the HUMAN through the certainty rule ("mark what cannot be determined") — option A.

## Options
- **A (ratified)** — `content_hash` always present, `null` when the bytes could not be read. Reward: uniform JSON shape; an honest unknown; the CLI never dies on any admitted document. Risk: a CLI DTO change (`Option<String>`, key always serialised).
- **B** — key omitted when bytes were not read. Reward: consumers that treat a missing key as unknown need no change; no null semantics. Risk: a conditional shape every consumer must handle.
- **C** — do not inventory bytes-unreadable documents. Reward: less inventory and read work; every entry has a real hash. Risk: contradicts L05's admitted-and-counted intent; a permission-denied README silently vanishes.

## Resolution
RG-REQ-008-L08 and L05 amended. DOCS-UNREADABLE-DECODE-1 resumes with the byte-hash change plus the CLI `Option<String>` decode (key always serialised).
