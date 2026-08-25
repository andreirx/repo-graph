#!/usr/bin/env python3
"""EMBED-CONCERN-SPIKE-1 — do embedding clusters reveal cross-module concerns (seam candidates)?
K-means (K=24, cosine) over the EMBED-SEED-SPIKE-1 file vectors; clusters spanning >=2 deployable
modules, ranked by span then intra-cluster cohesion. See spike doc for measured results."""
# (verbatim the demo run of 2026-08-25 — kept for reproducibility; imports spike.py's corpus+cache)
