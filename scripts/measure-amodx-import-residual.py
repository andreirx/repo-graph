#!/usr/bin/env python3
# IMPORTS-PACKAGE-EXTERNAL-EVIDENCE measurement v2 (read-only). Cleaner: statement-anchored import regex,
# textual tsconfig alias-prefix detection (bypasses JSONC parsing), @types node_modules check.
import json, os, re, glob
from collections import defaultdict

AMODX = "/Users/apple/Documents/APLICATII BIJUTERIE/amodx"
PARTITIONS = ["admin", "backend", "infra", "renderer",
              "packages/effects", "packages/plugins", "packages/shared", "tools/mcp-server"]
NODE_BUILTINS = set("assert buffer child_process cluster console crypto dgram dns domain events fs http http2 "
                    "https net os path perf_hooks process punycode querystring readline repl stream "
                    "string_decoder timers tls tty url util v8 vm worker_threads zlib".split())

def strip_jsonc(t):
    t = re.sub(r'//[^\n]*', '', t); t = re.sub(r'/\*.*?\*/', '', t, flags=re.S)
    return re.sub(r',(\s*[}\]])', r'\1', t)

def load_json(p):
    try: return json.loads(strip_jsonc(open(p).read()))
    except Exception: return None

# workspace map
workspace = set()
for part in PARTITIONS:
    pj = load_json(f"{AMODX}/{part}/package.json")
    if pj and pj.get("name"): workspace.add(pj["name"])

def pkg_name(spec):
    segs = spec.split('/')
    return ('/'.join(segs[:2]) if len(segs) >= 2 else spec) if spec.startswith('@') else segs[0]

def nm_has(part, name):  # node_modules/<name> OR node_modules/@types/<name> (type package), partition then root
    types = "@types/" + (name[1:].replace('/', '__') if name.startswith('@') else name)
    for base in (f"{AMODX}/{part}/node_modules", f"{AMODX}/node_modules"):
        if os.path.isdir(f"{base}/{name}") or os.path.isdir(f"{base}/{types}"): return True
    return False

# alias prefixes from tsconfig TEXT (bypass JSONC): keys like "@/*" -> prefix "@/"
def alias_prefixes(part):
    prefixes = []
    for tc in glob.glob(f"{AMODX}/{part}/tsconfig*.json"):
        try: txt = open(tc).read()
        except Exception: continue
        for m in re.finditer(r'"([^"]+?)\*?"\s*:\s*\[', txt):  # paths keys
            k = m.group(1)
            if not k.startswith('.') and ('compilerOptions' in txt and 'paths' in txt):
                prefixes.append(k)
    return [p for p in prefixes if p and not p[0].isalnum() or p.startswith('@')]

# import statement specifiers (anchored): import/export ... from 'x'; side-effect import 'x'; dynamic import('x')
STMT = re.compile(r"""^\s*(?:import|export)\b[^\n;]*?\bfrom\s*['"]([^'"]+)['"]""", re.M)
SIDE = re.compile(r"""^\s*import\s+['"]([^'"]+)['"]""", re.M)
DYN  = re.compile(r"""\bimport\(\s*['"]([^'"]+)['"]""")

cat_counts = defaultdict(int); cat_specs = defaultdict(set)
per_part = defaultdict(lambda: defaultdict(int))
transitive = defaultdict(int); unknown = defaultdict(int)

for part in PARTITIONS:
    pj = load_json(f"{AMODX}/{part}/package.json") or {}
    declared = set()
    for k in ("dependencies", "devDependencies", "peerDependencies"):
        declared |= set((pj.get(k) or {}).keys())
    aps = alias_prefixes(part)
    files = glob.glob(f"{AMODX}/{part}/src/**/*.ts", recursive=True) + glob.glob(f"{AMODX}/{part}/src/**/*.tsx", recursive=True)
    for f in files:
        try: text = open(f).read()
        except Exception: continue
        for spec in DYN.findall(text):
            if not spec.startswith('.'):
                cat_counts["dynamic"] += 1; cat_specs["dynamic"].add(spec); per_part[part]["dynamic"] += 1
        for spec in set(STMT.findall(text)) | set(SIDE.findall(text)) if False else (STMT.findall(text) + SIDE.findall(text)):
            if spec.startswith('.'): continue
            name = pkg_name(spec)
            if spec.startswith("node:") or name in NODE_BUILTINS: cat = "node_builtin_external"
            elif name in workspace: cat = "workspace_local"
            elif any(spec.startswith(p) for p in aps): cat = "tsconfig_alias"
            elif name in declared: cat = "declared_external"
            elif nm_has(part, name): cat = "TRANSITIVE_external_node_modules"; transitive[name] += 1
            else: cat = "TRUE_unknown"; unknown[name] += 1
            cat_counts[cat] += 1; cat_specs[cat].add(spec); per_part[part][cat] += 1

print("=== amodx residual import classification v2 (read-only) ===")
print(f"workspace: {sorted(workspace)}\n")
print(f"{'CATEGORY':<34}{'occ':>8}{'distinct':>10}")
for c in sorted(cat_counts, key=lambda c: -cat_counts[c]):
    print(f"{c:<34}{cat_counts[c]:>8}{len(cat_specs[c]):>10}")
print("\n=== BLOCKING residual split (slice-2 target) ===")
print(f"TRANSITIVE external (node_modules/@types, not declared): {sum(transitive.values())} occ / {len(transitive)} distinct")
print("  ", sorted(transitive.items(), key=lambda x: -x[1])[:15])
print(f"TRUE unknown (not workspace/alias/declared/node_modules):  {sum(unknown.values())} occ / {len(unknown)} distinct")
print("  ", sorted(unknown.items(), key=lambda x: -x[1])[:15])
print("\n=== per-partition ===")
for part in PARTITIONS:
    if per_part[part]: print(f"  {part}: {dict(per_part[part])}")
