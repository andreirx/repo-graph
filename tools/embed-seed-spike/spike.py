#!/usr/bin/env python3
"""EMBED-SEED-SPIKE-1 — can local embeddings SEED orientation (pick the anchor file/symbol
for a natural-language task) better than lexical matching, using ONLY what rmap already
indexed (files + symbols tables) plus source spans?

Corpus: rmap SQLite index of glamCRM (retained smoke state root).
Model : LM Studio, text-embedding-nomic-embed-text-v1.5 (768d), local, OpenAI-compatible API.
Metric: hit@1 / hit@5 / hit@10 of an expert-chosen anchor file per task, for
        (F) file-level docs, (S) symbol-level docs rolled up max-per-file, (L) lexical baseline.
Determinism notes: vectors cached by sha256(doc text + model id); ranking ties broken by path.
"""
import hashlib, json, os, re, sqlite3, sys, time, math
import numpy as np, requests

DB = "/private/tmp/repo-graph-tests/use-1/databases/232740d405431851.db"
REPO = "/Users/apple/Documents/APLICATII BIJUTERIE/glamCRM"
MODEL = "text-embedding-nomic-embed-text-v1.5"
URL = "http://localhost:1234/v1/embeddings"
CACHE = "/private/tmp/embed-spike/cache.npz"
SYMBOL_KINDS = ("CLASS", "INTERFACE", "METHOD", "FUNCTION", "TYPE_ALIAS", "ENUM", "CONSTRUCTOR")

# task -> acceptable anchor files (any hit counts). Ground truth established by reading the tree.
TASKS = [
    ("Where does the serverless backend fetch currency exchange rates from the national bank (BNR)?",
     ["serverless/packages/backend/src/services/bnr-service.ts", "serverless/packages/backend/src/handlers/exchange-rates.ts"]),
    ("How does the frontend authenticate users with AWS Cognito?",
     ["frontend/web/src/auth/CognitoAuthAdapter.ts"]),
    ("Where is the PDF document for an estimate generated?",
     ["serverless/packages/backend/src/services/estimate-service.ts", "backend/src/main/java/soft/bijuterie/glam/backend/core/estimate/EstimateDocumentService.java"]),
    ("Which code decides the applicable VAT rate for an offer?",
     ["serverless/packages/backend/src/services/applicable-vat-service.ts", "serverless/packages/backend/src/handlers/vat.ts"]),
    ("Where does the frontend configure the API base URL and the HTTP client used for REST calls?",
     ["frontend/web/src/config/api-client.ts", "frontend/web/src/config/runtime-config.ts"]),
    ("Where are tenant branding settings (logo, title) served by the backend?",
     ["serverless/packages/backend/src/handlers/tenant-brand.ts"]),
    ("Where does the React frontend call the offers REST endpoints?",
     ["frontend/web/src/api/offer.ts"]),
    ("Where is the activity log written when a user changes data?",
     ["serverless/packages/backend/src/services/activity-log-service.ts"]),
    ("How are uploaded files stored and downloaded (S3 presigned URLs)?",
     ["serverless/packages/backend/src/handlers/files-v2.ts", "serverless/packages/backend/src/handlers/files.ts", "frontend/web/src/api/fileHosting.ts"]),
    ("Which lambda handler runs on a schedule (cron)?",
     ["serverless/packages/backend/src/handlers/scheduled"]),  # prefix match
    ("Where does the one-off migration script transform etape (stage) data?",
     ["serverless/packages/backend/src/scripts/transform-etape.ts"]),
    ("Where is the warm-up lambda that keeps functions hot?",
     ["serverless/packages/backend/src/handlers/warmup.ts"]),
    ("Which Java backend code manages price lists?",
     ["backend/src/main/java/soft/bijuterie/glam/backend/core/pricelist", "backend/src/main/java/soft/bijuterie/glam/backend/app/controllers/PriceListController.java"]),
    ("Where are sales targets per company computed?",
     ["serverless/packages/backend/src/handlers/company-targets.ts", "serverless/packages/backend/src/handlers/sales-targets.ts", "serverless/packages/backend/src/services/sales-target-service.ts"]),
    ("Where does the UI let a user edit an existing offer?",
     ["frontend/web/src/pages/EditOfferPage.tsx", "frontend/workspace/src/pages/OfferDetailPage.tsx"]),
    ("Which handler serves health checks?",
     ["serverless/packages/backend/src/handlers/health.ts"]),
]

def sh(s): return hashlib.sha256(s.encode()).hexdigest()

def read_lines(path, a, b):
    try:
        with open(os.path.join(REPO, path), encoding="utf-8", errors="replace") as f:
            lines = f.read().splitlines()
        return "\n".join(lines[max(a - 1, 0):b])
    except OSError:
        return ""

def build_corpus():
    con = sqlite3.connect(DB)
    snap = con.execute("select snapshot_uid from snapshots where status='ready' order by created_at desc limit 1").fetchone()[0]
    files = {}
    for fu, path, lang, is_test in con.execute("select file_uid,path,language,is_test from files"):
        if is_test or "/__tests__/" in path or "/e2e/" in path:
            continue
        files[fu] = path
    file_docs = []
    for fu, path in files.items():
        body = read_lines(path, 1, 60)
        file_docs.append((path, f"search_document: {path}\n{body}"))
    sym_docs = []
    q = ("select n.subtype,n.qualified_name,n.name,n.file_uid,n.line_start,n.line_end,n.signature,n.doc_comment "
         "from nodes n where n.snapshot_uid=? and n.kind='SYMBOL'")
    for st, qn, name, fu, ls, le, sig, doc in con.execute(q, (snap,)):
        if st not in SYMBOL_KINDS or fu not in files:
            continue
        path = files[fu]
        body = read_lines(path, ls or 1, min(le or (ls or 1) + 10, (ls or 1) + 12))
        text = f"search_document: {path} :: {st} {qn or name}{(sig or '')[:200]}\n{(doc or '')[:300]}\n{body}"
        sym_docs.append((path, text))
    return file_docs, sym_docs

def embed(texts, cache):
    out, todo = [None] * len(texts), []
    for i, t in enumerate(texts):
        k = sh(MODEL + "\x00" + t)
        if k in cache: out[i] = cache[k]
        else: todo.append(i)
    B = 32
    for s in range(0, len(todo), B):
        idx = todo[s:s + B]
        r = requests.post(URL, json={"model": MODEL, "input": [texts[i][:6000] for i in idx]}, timeout=600)
        r.raise_for_status()
        for j, d in zip(idx, r.json()["data"]):
            v = np.asarray(d["embedding"], dtype=np.float32); v /= (np.linalg.norm(v) + 1e-9)
            out[j] = v; cache[sh(MODEL + "\x00" + texts[j])] = v
        print(f"  embedded {min(s+B, len(todo))}/{len(todo)}", file=sys.stderr)
    return np.vstack(out)

def tokenize(s):
    return [w for w in re.findall(r"[a-z0-9]+", re.sub(r"([a-z])([A-Z])", r"\1 \2", s).lower()) if len(w) > 2]

def lexical_rank(query, docs):
    # tf-idf-ish: sum over query terms of idf * (1 if term in doc)
    toks = [set(tokenize(t)) for _, t in docs]
    N = len(docs)
    qt = set(tokenize(query))
    idf = {w: math.log((N + 1) / (1 + sum(1 for s in toks if w in s))) for w in qt}
    scores = [sum(idf[w] for w in qt if w in s) for s in toks]
    return scores

def hit(anchors, ranked_paths, k):
    for p in ranked_paths[:k]:
        for a in anchors:
            if p == a or p.startswith(a.rstrip("/") + "/"):
                return True
    return False

def rank_files_from_scores(paths, scores):
    best = {}
    for p, s in zip(paths, scores):
        if p not in best or s > best[p]: best[p] = s
    return [p for p, _ in sorted(best.items(), key=lambda x: (-x[1], x[0]))]

def main():
    t0 = time.time()
    file_docs, sym_docs = build_corpus()
    print(f"corpus: {len(file_docs)} files, {len(sym_docs)} symbols ({time.time()-t0:.1f}s)")
    cache = {}
    if os.path.exists(CACHE):
        z = np.load(CACHE); cache = {k: z[k] for k in z.files}
    t0 = time.time()
    Vf = embed([t for _, t in file_docs], cache)
    Vs = embed([t for _, t in sym_docs], cache)
    np.savez(CACHE, **cache)
    print(f"embedding done ({time.time()-t0:.1f}s incl. cache)")
    Q = embed(["search_query: " + q for q, _ in TASKS], cache)
    fpaths = [p for p, _ in file_docs]; spaths = [p for p, _ in sym_docs]
    res = {"F": [0, 0, 0], "S": [0, 0, 0], "L": [0, 0, 0]}
    rows = []
    for qi, (q, anchors) in enumerate(TASKS):
        rf = rank_files_from_scores(fpaths, (Vf @ Q[qi]).tolist())
        rs = rank_files_from_scores(spaths, (Vs @ Q[qi]).tolist())
        rl = rank_files_from_scores(fpaths, lexical_rank(q, file_docs))
        row = {"task": q, "anchors": anchors}
        for key, r in (("F", rf), ("S", rs), ("L", rl)):
            hs = [hit(anchors, r, k) for k in (1, 5, 10)]
            for i, h in enumerate(hs): res[key][i] += int(h)
            row[key] = {"hit@1": hs[0], "hit@5": hs[1], "hit@10": hs[2], "top3": r[:3]}
        rows.append(row)
    n = len(TASKS)
    print("\n=== hit@k over %d tasks (anchor file in top-k) ===" % n)
    for key, label in (("F", "embeddings, file-level docs"), ("S", "embeddings, symbol-level (max per file)"), ("L", "lexical tf-idf baseline, file-level")):
        print(f"  {label:45s} hit@1 {res[key][0]}/{n}  hit@5 {res[key][1]}/{n}  hit@10 {res[key][2]}/{n}")
    print("\n=== per task ===")
    for r in rows:
        print(f"- {r['task']}")
        for key in ("F", "S", "L"):
            print(f"    {key}: @1={int(r[key]['hit@1'])} @5={int(r[key]['hit@5'])} @10={int(r[key]['hit@10'])}  top3={r[key]['top3']}")
    json.dump({"model": MODEL, "tasks": rows, "summary": res, "n": n,
               "corpus": {"files": len(file_docs), "symbols": len(sym_docs)}},
              open("/private/tmp/embed-spike/results.json", "w"), indent=1)

if __name__ == "__main__":
    main()
