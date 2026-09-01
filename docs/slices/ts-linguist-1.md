# TS-LINGUIST-1 — a file's language is its content, not its extension

Status: SPECIFIED (2026-09-01) · Track: diverse-verification queue (high). CODE slice.
Maturity: MATURE (language classification feeds persisted facts).

## 1. Problem (measured — VCMI, smoke 2026-09-01T15-01-04Z)

VCMI's 54 Qt Linguist translation files (`mapeditor/translation/*.ts`, `launcher/translation/
*.ts`) are XML (`<?xml version="1.0" ...?><!DOCTYPE TS>`, verified) but classify as
TypeScript by extension. The misclassification cascades into FOUR surfaces: churn's entire
top-25 ("czech.ts 1 commits 5418 lines"), dead/inferences applicability ("React … applies to
this snapshot's … TypeScript files"), deps' ecosystem header, stats rows
("launcher/translation 27 files, 0 symbols, unknown"). Extension-as-evidence is the
name-vs-semantics defect at the file level.

## 2. Contract

1. **Content-sniff the ambiguous extension.** For `.ts` (and `.mts`/`.cts` if the same
   ambiguity exists — verify) whose content begins with an XML declaration or `<!DOCTYPE TS`
   (bounded read of the first line(s), never the whole file), classify per the schema's
   existing non-code token for XML/config content — VERIFY what the language token set
   supports and use the existing honest token (do not invent a new language token without a
   DECISION_REQUIRED). The sniff is deterministic content evidence; no name heuristics
   beyond selecting WHICH files to sniff.
2. **Byte-parity on genuine TypeScript**: real TS repos (FRAKTAG, glamCRM, amodx) index
   byte-identically (sniff cost bounded; a `.ts` starting with code is untouched).
3. **The cascades self-heal via the fact**: churn/dead/inferences/deps/stats need no edits —
   validate they read correctly post-fix; if any surface separately caches or re-derives
   language from extension, that is a FINDING (fix in-slice only on the four affected
   surfaces; else record).
4. Unreadable file at sniff time → classify by the honest fallback the indexer already uses
   for unreadable files (never silently default to TypeScript); the unreadable count already
   surfaces.

## 3. Stop conditions

Frozen: storage schema (the language COLUMN exists; only the value assignment changes — a
new token value needs DECISION_REQUIRED), exit codes, trust computation, enrich/seed
semantics. STANDING HONESTY RULES. Unmet DoD → STOP + DECISION_REQUIRED. Never touch the
operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Reproducing test FIRST: fixture with a Qt Linguist `.ts` + a real TypeScript `.ts` →
  pre-fix both classify TypeScript (FAILS); post-fix the XML one carries the non-code token,
  the real one unchanged.
- Unit: sniff boundary cases (BOM, leading whitespace/comments, empty file, unreadable).
- Live proof (isolated state root, registry sha unchanged): VCMI — dead/inferences no
  longer claim TypeScript applicability; deps' ecosystem header free of TypeScript; stats'
  translation rows honest. CHURN CRITERION AMENDED (ruling TSLING1-CHURN-HEAL, 2026-09-01):
  churn measures git activity and translation files genuinely churn — they REMAIN visible;
  the defect was the TypeScript label, which the fact-fix cures. Three surfaces heal, not
  four. FRAKTAG byte-parity spot-check vs the 15-01-04Z captures
  (allowing only lines the enrichment epoch legitimately moves).
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

Qt Linguist files never masquerade as TypeScript; the language-derived surfaces heal through
the one fact (churn honestly keeps real translation activity);
genuine TS repos byte-stable; sniffing is bounded content evidence with honest fallbacks;
gates green.
