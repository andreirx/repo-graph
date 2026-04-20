/**
 * Operational module promotion — Layer 2 module discovery.
 *
 * Promotes unattached project surfaces to operational module candidates.
 * This is the bridge from declared structure (Layer 1) to operational
 * structure (Layer 2).
 *
 * See docs/architecture/module-discovery-layers.txt for the full contract.
 *
 * Pure function. No filesystem access. No storage calls. No side effects.
 *
 * Promotion criteria (strict):
 *   - Surface kind in {cli, backend_service, web_app, worker}
 *   - At least one evidence item with confidence >= 0.70
 *   - NOT library-only (libraries require declared packaging)
 *
 * Aggregate confidence:
 *   - max(evidence.confidence) clamped to 0.85 (Layer 2 ceiling)
 *
 * Root derivation:
 *   - surface.rootPath exactly (no heuristics)
 */

import { v4 as uuidv4 } from "uuid";
import {
	buildModuleKey,
	type ModuleCandidate,
	type ModuleCandidateEvidence,
	type EvidenceSourceType,
} from "./module-candidate.js";
import type { SurfaceKind } from "../runtime/project-surface.js";
import type { DetectedSurface } from "../runtime/surface-detectors.js";

// ── Constants ──────────────────────────────────────────────────────

/** Layer 2 confidence ceiling per contract §2. */
const LAYER_2_CONFIDENCE_CEILING = 0.85;

/** Minimum evidence confidence for promotion per contract §3.2. */
const MIN_EVIDENCE_CONFIDENCE = 0.70;

/** Surface kinds eligible for promotion per contract §3.1. */
const PROMOTABLE_SURFACE_KINDS: ReadonlySet<SurfaceKind> = new Set([
	"cli",
	"backend_service",
	"web_app",
	"worker",
]);

/** Map surface kind to evidence source type. */
const SURFACE_KIND_TO_EVIDENCE_SOURCE: Record<string, EvidenceSourceType> = {
	cli: "surface_promotion_cli",
	backend_service: "surface_promotion_service",
	web_app: "surface_promotion_web_app",
	worker: "surface_promotion_worker",
};

// ── Input/Output types ─────────────────────────────────────────────

export interface PromotionInput {
	repoUid: string;
	snapshotUid: string;
	/** Unattached surfaces (no matching declared module). */
	unattachedSurfaces: DetectedSurface[];
	/** Existing module candidate roots (for dedup). */
	existingRoots: Set<string>;
}

export interface PromotionResult {
	/** New operational module candidates. */
	candidates: ModuleCandidate[];
	/** Evidence for promoted modules. */
	evidence: ModuleCandidateEvidence[];
	/** Diagnostics. */
	diagnostics: PromotionDiagnostics;
}

export interface PromotionDiagnostics {
	surfacesEvaluated: number;
	surfacesPromoted: number;
	surfacesSkippedKind: number;
	surfacesSkippedConfidence: number;
	promotionSkippedExistingRoot: number;
}

// ── Promotion orchestrator ─────────────────────────────────────────

/**
 * Promote qualifying unattached surfaces to operational module candidates.
 *
 * Surfaces are grouped by rootPath. Multiple qualifying surfaces at the
 * same root produce one module candidate with merged evidence.
 */
export function promoteUnattachedSurfaces(input: PromotionInput): PromotionResult {
	const { repoUid, snapshotUid, unattachedSurfaces, existingRoots } = input;

	const diagnostics: PromotionDiagnostics = {
		surfacesEvaluated: unattachedSurfaces.length,
		surfacesPromoted: 0,
		surfacesSkippedKind: 0,
		surfacesSkippedConfidence: 0,
		promotionSkippedExistingRoot: 0,
	};

	// 1. Filter surfaces that qualify for promotion.
	const qualifyingSurfaces: DetectedSurface[] = [];

	for (const surface of unattachedSurfaces) {
		// Check surface kind constraint.
		if (!PROMOTABLE_SURFACE_KINDS.has(surface.surfaceKind)) {
			diagnostics.surfacesSkippedKind++;
			continue;
		}

		// Check evidence confidence constraint.
		const maxEvidenceConfidence = Math.max(
			...surface.evidence.map((e) => e.confidence),
		);
		if (maxEvidenceConfidence < MIN_EVIDENCE_CONFIDENCE) {
			diagnostics.surfacesSkippedConfidence++;
			continue;
		}

		qualifyingSurfaces.push(surface);
	}

	// 2. Group qualifying surfaces by rootPath.
	const surfacesByRoot = new Map<string, DetectedSurface[]>();
	for (const surface of qualifyingSurfaces) {
		const root = normalizeRootPath(surface.rootPath);
		const existing = surfacesByRoot.get(root);
		if (existing) {
			existing.push(surface);
		} else {
			surfacesByRoot.set(root, [surface]);
		}
	}

	// 3. Create module candidates, skipping roots that already exist.
	const candidates: ModuleCandidate[] = [];
	const evidence: ModuleCandidateEvidence[] = [];

	for (const [rootPath, surfaces] of surfacesByRoot) {
		// Check for exact-root collision with existing candidates.
		if (existingRoots.has(rootPath)) {
			diagnostics.promotionSkippedExistingRoot++;
			continue;
		}

		const candidateUid = uuidv4();
		const moduleKey = buildModuleKey(repoUid, rootPath);

		// Compute aggregate confidence: max evidence confidence, clamped.
		const maxConfidence = Math.max(
			...surfaces.flatMap((s) => s.evidence.map((e) => e.confidence)),
		);
		const moduleConfidence = Math.min(maxConfidence, LAYER_2_CONFIDENCE_CEILING);

		// Use first surface's display name if available.
		const displayName = surfaces.find((s) => s.displayName)?.displayName ?? null;

		candidates.push({
			moduleCandidateUid: candidateUid,
			snapshotUid,
			repoUid,
			moduleKey,
			moduleKind: "operational",
			canonicalRootPath: rootPath,
			confidence: moduleConfidence,
			displayName,
			metadataJson: JSON.stringify({
				promotingSurfaceKinds: [...new Set(surfaces.map((s) => s.surfaceKind))],
			}),
		});

		// Create evidence from each promoting surface.
		for (const surface of surfaces) {
			const sourceType = SURFACE_KIND_TO_EVIDENCE_SOURCE[surface.surfaceKind];
			if (!sourceType) continue; // Should not happen given filter above.

			evidence.push({
				evidenceUid: uuidv4(),
				moduleCandidateUid: candidateUid,
				snapshotUid,
				repoUid,
				sourceType,
				sourcePath: surface.rootPath, // Use root as source path.
				evidenceKind: "operational_entrypoint",
				confidence: Math.max(...surface.evidence.map((e) => e.confidence)),
				payloadJson: JSON.stringify({
					promoting_surface_kind: surface.surfaceKind,
					original_evidence: surface.evidence.map((e) => ({
						sourceType: e.sourceType,
						sourcePath: e.sourcePath,
						evidenceKind: e.evidenceKind,
						confidence: e.confidence,
					})),
				}),
			});
		}

		diagnostics.surfacesPromoted += surfaces.length;
	}

	return { candidates, evidence, diagnostics };
}

// ── Helpers ────────────────────────────────────────────────────────

/**
 * Normalize a root path: strip trailing slashes, collapse "" to ".".
 */
function normalizeRootPath(path: string): string {
	let p = path.replace(/\/+$/, "");
	if (p === "" || p === ".") return ".";
	return p;
}
