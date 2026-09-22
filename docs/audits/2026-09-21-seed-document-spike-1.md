# SEED-DOCUMENT-1 mechanism spike — what does document composition actually move? (2026-09-21)

Manager spike, read-only, before any packet (operator practice: a mechanism claim is verified on the live
system before packeting). HEAD 12eecd5 (v0.18.0 + the round-six queue through ALIAS-SUSPICION-1).

## Question

RG-REQ-010-L10 (ratified 2026-09-14) says a method chunk's embedded document shall carry, besides its
qualified name, doc comment and leading body lines, **its enclosing type's doc and a bounded identifier
summary**, so that a task query can reach the method that does the work — measured on FRAKTAG's
"where are conversations persisted to disk" with the 0.30 floor and the model unchanged. The composition
was a diagnosis (SEED-CHUNK-3 §2.6, 2026-09-08), never a measurement. This spike measures it.

## Method

Same model as the product (`minishlab/potion-code-16M-v2`, the operator's local model cache, loaded with the
Python `model2vec` 0.9.0 package). Chunks = the product's own seed corpus query (SYMBOL nodes with a span,
generated/excluded files out) over isolated stores built by the HEAD binary
(`/private/tmp/SEED-DOCUMENT-1-{fraktag,leveldb}-before`, `/private/tmp/SEED-DOCUMENT-1-grpc-spike`); span
text read from the checkouts; documents composed exactly as `repo-graph-seed/src/document.rs` does today
(variant V0) and with each candidate composition; cosine against the verbatim query. Operator registry digest
4762fd948a05b208 before and after; no daemon served; no retained root touched.

**Fidelity check (V0 vs the product's own `find --json` scores on the same root):** ConversationManager
0.387 = 0.387; ConversationManager.listSessions 0.333 = 0.333; createSession 0.274 (recorded 0.27); logTurn
0.155 (recorded 0.15); leveldb top five identical to three decimals. The spike's SCORES are the product's.
Its RANKS are approximate: the production/test partition and the field tier are approximated (one-line
undocumented chunk = field), the decl tier is not modelled.

## Variants

| Id | Composition (order) |
|---|---|
| V0 | qualified name, own doc, first 60 body lines (today) |
| VE | qualified name, **whole enclosing-type doc**, own doc, body |
| VC | as VE, enclosing doc cut at 240 characters |
| VS | as VE, enclosing doc = **first sentence only**, at most 160 characters |
| VP | as VE, enclosing doc = its first paragraph |
| +summary | identifier words of the span (camelCase/snake_case split, lower-cased, de-duplicated, 48 words), placed before the body, after the body, or drawn only from span lines beyond the 60-line cap (32 words) |

Enclosing type = the symbol whose qualified name is the chunk's qualified name minus its last `.`/`::`
segment, in the same file (or unique in the corpus — a C++ class declared in a header).

## Results

### FRAKTAG — "where are conversations persisted to disk" (the requirement's measure; floor 0.30)

Code under analysis: `packages/engine/src/core/ConversationManager.ts:48-60` class doc
"ConversationManager handles conversation persistence. Each conversation is its own Tree (conv-{uuid}),
stored in Internal KB." + an 8-line ASCII tree; `:71 async createSession(` (doc ":68 Creates a new
conversation session as a separate Tree."); `:133 async logTurn(` (doc "Logs a conversation turn into the
session's tree.") — neither method's own text contains "persist", "disk" or "store".

| Variant | createSession :71 | logTurn :133 | owning class :61 | rows 1-2 |
|---|---|---|---|---|
| V0 today | 0.274, rank 14 — absent | 0.155, rank 83 | 0.387, rank 3 | Fraktag.listConversationSessions, Fraktag.conversationManager |
| VE whole class doc | **0.370, rank 6** | 0.220, rank 41 | 0.387, rank 5 | ConversationManager.deleteSession, .listSessions; `.constructor` rank 4 |
| VC 240 chars | **0.372, rank 6** | 0.224, rank 40 | rank 5 | deleteSession, `.constructor` rank 2 |
| VS first sentence | **0.333, rank 9** | 0.192, rank 60 | **rank 3** | Fraktag.listConversationSessions, ConversationManager.deleteSession; `.constructor` rank 8 |
| V0 + summary (no enclosing doc) | 0.274 (no gain) | 0.156 | rank 4 | listSessions falls 0.333 → 0.310 |
| VE + summary (before or after the body — identical) | 0.354 (**-0.016** vs VE) | 0.220 / 0.215 | rank 3 | |
| VP first paragraph of the class doc | 0.361, rank 6 | 0.213, rank 44 | rank 5 | deleteSession, listSessions |
| VP + tail-only summary | 0.361 = VP (no method here exceeds 60 lines) | 0.213 | rank 5 | |

Controls, same root: "how are content atoms hashed" top two unchanged in order, scores UP
(`ContentStore.findByHash` 0.664 → 0.689/0.690, `calculateHash` 0.600 → 0.653/0.658);
"where are tree nodes saved to disk" `TreeStore.saveNode` 0.623 rank 1 under every variant.

### leveldb — "crash recovery"

Byte-identical under every variant: of 1,304 chunks with an enclosing symbol, **0** have a documented
enclosing type in the store (C++ class docs are not captured as `doc_comment` on the class node today), so the
composition changes nothing there. `leveldb::DBImpl::Recover` stays at 0.201 (rank 20).

### grpc-java (36,395 chunks; 23,133 have a documented enclosing type; enclosing-doc length median 68, p90 379, max 6,409 characters)

"where are retries scheduled with backoff":
`io.grpc.internal.RetryScheduler.schedule` (RetryScheduler.java:30) rank 5 → VE 3 / VS 5;
`io.grpc.internal.BackoffPolicyRetryScheduler.schedule` (:52) rank 10 → VE 5 / VS 4 — better;
but `BackoffPolicyRetryScheduler.logger` (:38, a two-line logger field) goes from absent to **rank 1 (0.630)**
under VE/VC — a row that is nothing but its class's doc; under VS it is rank 9 (0.560).

"how is the deadline propagated to the server":
`io.grpc.stub.AbstractStub.withDeadline` (AbstractStub.java:141) rank 2 (0.610) → VE **rank 9 (0.502)** /
VC 3 (0.586) / VS 2 (0.608); `io.grpc.internal.ClientStream.setDeadline` (ClientStream.java:94) rank 4 (0.595)
→ VE/VC **rank 10 (0.491)** / VS 9 (0.544). A method whose own text already matched precisely is pulled
AWAY by a class doc about something else.

## Findings

1. **The enclosing type's doc is the whole effect.** It alone takes createSession over the floor and into the
   top ten (0.274 → 0.333…0.372).
2. **The identifier summary has no measurable benefit and a small cost.** Placement is irrelevant (before or
   after the body gives identical scores — these documents do not reach the 512-token window), and with or
   without the class doc it never raises a target score; it lowers createSession by 0.016 and listSessions
   by 0.023. Mechanism: potion's tokenizer already splits identifiers into word pieces, so the summary repeats
   tokens the body contributed — in a mean-pooled static model that only re-weights the mean.
3. **Added text is never free in a mean-pooled model.** The more class doc is added, the more (a) precisely
   matching methods are diluted (AbstractStub.withDeadline 2 → 9 with the whole doc) and (b) short members
   become echoes of their class doc (a logger field at rank 1; a 5-line constructor at rank 2-4). The first
   sentence, bounded, keeps the FRAKTAG gain (rank 9, 0.333 — 0.033 above the floor) with the least harm
   (withDeadline stays 2; the echo rows stay low; the owning class stays rank 3, SEED-CHUNK-3's acceptance).
4. logTurn stays below the floor under every variant (best 0.224). TreeStore.saveNode / ContentStore.create
   are not moved by any variant on the conversations query (their classes' docs are not about conversations);
   on their own queries they already rank 1.
5. leveldb gains nothing until C++ class docs reach the store — a separate extractor fact, not composition.
6. Cost common to every variant: the document hash is the reuse key, so the first seed pass after the change
   re-embeds every chunk of every repo (reused = 0), silently.

## Limits of this spike

Ranks are approximate (tiers approximated); the two grpc-java queries were chosen by the manager and have no
recorded baseline; one TypeScript, one C++ and one Java corpus; first-sentence extraction here is a simple
comment-marker strip + first period rule.

## Appendix — the spike script (`sd1-spike.py`, run in a scratch venv with `model2vec numpy`)

```python
"""SEED-DOCUMENT-1 mechanism spike (manager, read-only): does document COMPOSITION move the
write-path methods above the 0.30 floor? Same model (potion-code-16M-v2 from the local cache),
documents composed from the isolated before-root's store + the checkout's source bytes.
V0 must reproduce the product's scores (fidelity check) before any variant is believed."""
import sqlite3, glob, sys, re, os
import numpy as np
from model2vec import StaticModel

root, checkout, query = sys.argv[1], sys.argv[2], sys.argv[3]
watch = sys.argv[4].split(',') if len(sys.argv) > 4 else []
MODEL = glob.glob(os.path.expanduser('~/Library/Application Support/repo-graph/seed-model-cache/models--minishlab--potion-code-16M-v2/snapshots/*'))[0]
model = StaticModel.from_pretrained(MODEL)
MAX_BODY_LINES, MAX_DOC_CHARS = 60, 6000

db = glob.glob(root + '/databases/*.db')[0]
k = sqlite3.connect('file:' + db + '?mode=ro', uri=True)
snap = k.execute("select snapshot_uid from snapshots order by created_at desc limit 1").fetchone()[0]
rows = k.execute("""SELECT n.stable_key, f.path, n.qualified_name, n.doc_comment, n.line_start, n.line_end, f.is_test, n.subtype
  FROM nodes n JOIN files f ON f.file_uid = n.file_uid
  WHERE n.snapshot_uid = ? AND n.kind='SYMBOL' AND n.line_start IS NOT NULL AND f.is_generated=0 AND f.is_excluded=0
  ORDER BY f.path, n.line_start""", (snap,)).fetchall()
files = {}
def lines_of(p):
    if p not in files:
        try: files[p] = open(os.path.join(checkout, p), encoding='utf-8', errors='replace').read().split('\n')
        except OSError: files[p] = None
    return files[p]

def parent_qn(qn):
    i = max(qn.rfind('.'), qn.rfind('::'))
    if i <= 0: return None
    return qn[:i] if qn[i] == '.' else qn[:i]

by_file_qn = {}
for r in rows:
    if r[2]: by_file_qn.setdefault((r[1], r[2]), r)
by_qn = {}
for r in rows:
    if r[2]: by_qn.setdefault(r[2], []).append(r)

WORD = re.compile(r'[A-Za-z_][A-Za-z0-9_]*')
SPLIT = re.compile(r'[A-Z]+(?=[A-Z][a-z])|[A-Z]?[a-z]+|[A-Z]+|[0-9]+')
def summary(text, cap):
    out, seen = [], set()
    for ident in WORD.findall(text):
        for w in SPLIT.findall(ident.replace('_', ' ')):
            w = w.lower()
            if len(w) < 3 or w in seen: continue
            seen.add(w); out.append(w)
            if len(out) >= cap: return ' '.join(out)
    return ' '.join(out)

def first_para(doc):
    out=[]
    for ln in doc.split('\n'):
        t=ln.strip().lstrip('/*').strip()
        if not t and out: break
        if t: out.append(ln)
    return '\n'.join(out)

def first_sentence(doc):
    words=[]
    for ln in doc.split('\n'):
        t=ln.strip()
        while t and t[0] in '/*#!': t=t[1:]
        t=t.strip()
        if not t:
            if words: break
            continue
        if t.startswith('@') or t.startswith('<p>'): break
        words.append(t)
    text=' '.join(words)
    m=re.search(r'\.(\s|$)', text)
    if m: text=text[:m.start()+1]
    return text[:160]

def compose(r, variant):
    sk, path, qn, doc, ls, le, is_test, subtype = r
    fl = lines_of(path)
    if fl is None or ls is None: return None
    le = le or ls
    span_lines = fl[ls - 1:le]
    if not span_lines: return None
    body = '\n'.join(span_lines[:MAX_BODY_LINES])
    enc = None
    if qn and variant != 'V0':
        p = parent_qn(qn)
        if p:
            pr = by_file_qn.get((path, p))
            if pr is None and len(by_qn.get(p, [])) == 1: pr = by_qn[p][0]
            if pr is not None and pr[3]: enc = pr[3]
    parts = []
    if qn: parts.append(qn)
    if enc and 'E' in variant: parts.append(enc)
    if enc and 'P' in variant: parts.append(first_para(enc))
    if enc and 'C' in variant: parts.append(enc[:240])
    if enc and 'S' in variant: parts.append(first_sentence(enc))
    if doc: parts.append(doc)
    parts.append(body)
    if 'T' in variant:
        tail = summary('\n'.join(span_lines[MAX_BODY_LINES:]), 32)
        if tail: parts.append(tail)
    return '\n'.join(parts)[:MAX_DOC_CHARS]

q = model.encode([query])[0]
for variant in sys.argv[5].split(','):
    docs, keep = [], []
    for r in rows:
        d = compose(r, variant)
        if d: docs.append(d); keep.append(r)
    E = model.encode(docs)
    s = E @ q / (np.linalg.norm(E, axis=1) * np.linalg.norm(q) + 1e-12)
    def tier(i):
        r = keep[i]; one = (r[5] or r[4]) == r[4]
        return (1 if r[6] else 0, 1 if (one and not r[3]) else 0)
    order = sorted(range(len(keep)), key=lambda i: (tier(i), -s[i]))
    rank = {}
    for n, i in enumerate(order, 1): rank.setdefault(keep[i][2], (n, s[i]))
    print(f'== {variant}  ({len(docs)} chunks)  [approx. product order: production before test, body-bearing before one-line undocumented]')
    for n, i in enumerate(order[:10], 1):
        print(f'   {n:2d} {s[i]:.3f} {keep[i][2]}  {keep[i][1]}:{keep[i][4]}')
    for w in watch:
        if w in rank: print(f'   watch {w}: rank {rank[w][0]} score {rank[w][1]:.3f}')
```

## Addendum 2026-09-23 — scope of the enclosing sentence: every child kind vs METHOD chunks only

Raised by the SEED-DOCUMENT-1 document review (SD-DEC-01): RG-REQ-010-L10 says "a method chunk's document"; the spike's first-sentence variant (VS) had given the sentence to every chunk with a documented parent (properties, constructors, getters too). Re-measured with the same script (`sd1-spike.py`, variants `VSM` = subtype `METHOD` only, `VK` = METHOD/CONSTRUCTOR/FUNCTION) on the same isolated FRAKTAG root:

| Variant | createSession :71 | logTurn :133 | owning class :61 (raw rank; the product's field tier is not modelled) | ConversationManager.constructor :62 |
|---|---|---|---|---|
| VS every child kind | 0.333, rank 9 | 0.192 | rank 3 | 0.341, rank 8 (an echo row: a 5-line DI constructor carrying the class sentence) |
| VSM METHOD only | 0.333, rank 9 | 0.192 | rank 4 raw — rank 3 once `Fraktag.conversationManager` (index.ts:79, a one-line undocumented property the product field-tiers) sinks | 0.199, rank 54 (unchanged from today) |
| VK METHOD + CONSTRUCTOR + FUNCTION | 0.333, rank 10 | 0.192 | rank 4 raw (same note) | 0.341, rank 9 |

Controls under VSM: `how are content atoms hashed` → `ContentStore.findByHash` first (0.690); `where are tree nodes saved to disk` → `TreeStore.saveNode` first (0.623). FRAKTAG's chunks with an enclosing name by subtype: PROPERTY 611, METHOD 299, VARIABLE 45, CONSTRUCTOR 25, GETTER 9.

Reading: the target outcome is identical under METHOD-only; the every-child variant's only extra effect is the constructor echo row entering the top ten. METHOD-only is the ratified wording read literally and the smaller change (D-SD1-002 revised accordingly, 2026-09-23).
