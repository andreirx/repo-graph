/**
 * Surface discovery orchestrator — pure core logic.
 *
 * Takes detected surfaces from manifest detectors and links them
 * to existing module candidates. Produces ProjectSurface[] and
 * ProjectSurfaceEvidence[] for persistence.
 *
 * Policy: surfaces link to existing module candidates by root path.
 * Unattached surfaces (no matching candidate) are returned separately
 * for Layer 2 operational promotion. See operational-promotion.ts.
 *
 * No filesystem access. No storage calls. No side effects.
 */

import type { ModuleCandidate } from "../modules/module-candidate.js";
import type {
	ProjectSurface,
	ProjectSurfaceEvidence,
} from "./project-surface.js";
import type { DetectedSurface } from "./surface-detectors.js";
import {
	computeSourceSpecificId,
	computeStableSurfaceKey,
	computeProjectSurfaceUid,
	computeProjectSurfaceEvidenceUid,
} from "./surface-identity.js";

// ── Input/Output ───────────────────────────────────────────────────

export interface SurfaceDiscoveryInput {
	repoUid: string;
	snapshotUid: string;
	/** Detected surfaces from manifest detectors. */
	detectedSurfaces: DetectedSurface[];
	/** Existing module candidates to link against. */
	moduleCandidates: ModuleCandidate[];
}

export interface SurfaceDiscoveryResult {
	surfaces: ProjectSurface[];
	evidence: ProjectSurfaceEvidence[];
	/** Surfaces with no matching module candidate (for Layer 2 promotion). */
	unattachedSurfaces: DetectedSurface[];
	/** Count of unattached surfaces (convenience, equals unattachedSurfaces.length). */
	unattachedCount: number;
}

// ── Internal types for deduplication ──────────────────────────────

interface PendingSurface {
	surface: ProjectSurface;
	evidenceItems: ProjectSurfaceEvidence[];
	confidence: number;
}

// ── Orchestrator ───────────────────────────────────────────────────

/**
 * Link detected surfaces to module candidates and produce persistence
 * rows. Surfaces without a matching module candidate are collected
 * for Layer 2 operational promotion.
 *
 * Deduplication policy:
 * - Surfaces with the same stableSurfaceKey are merged in core before storage.
 * - The highest-confidence surface wins (its metadata and display name are kept).
 * - Evidence from all duplicate detections is merged and deduplicated by UID.
 *
 * This prevents the database INSERT OR REPLACE from making arbitrary
 * last-writer-wins decisions and ensures evidence accumulates rather
 * than being discarded.
 */
export function discoverSurfaces(input: SurfaceDiscoveryInput): SurfaceDiscoveryResult {
	const { repoUid, snapshotUid, detectedSurfaces, moduleCandidates } = input;

	// Build module candidate lookup by root path.
	const candidateByRoot = new Map<string, ModuleCandidate>();
	for (const mc of moduleCandidates) {
		candidateByRoot.set(mc.canonicalRootPath, mc);
	}

	// Deduplicate surfaces by stableSurfaceKey, keeping highest confidence.
	const surfaceByKey = new Map<string, PendingSurface>();
	const unattachedSurfaces: DetectedSurface[] = [];

	for (const detected of detectedSurfaces) {
		// Find the owning module candidate by root path match.
		const candidate = candidateByRoot.get(detected.rootPath);
		if (!candidate) {
			// Collect for Layer 2 promotion instead of dropping.
			unattachedSurfaces.push(detected);
			continue;
		}

		// Compute source-specific identity from detected surface fields.
		const sourceSpecificId = computeSourceSpecificId({
			sourceType: detected.sourceType,
			// Phase 0 identity fields
			binName: detected.binName,
			frameworkName: detected.frameworkName,
			scriptName: detected.scriptName,
			serviceName: detected.serviceName,
			dockerfilePath: detected.dockerfilePath,
			// Phase 1 identity fields (reserved, detectors not implemented)
			makefilePath: detected.makefilePath,
			workspaceName: detected.workspaceName,
			chartName: detected.chartName,
			infraModulePath: detected.infraModulePath,
			// Fallback fields
			entrypointPath: detected.entrypointPath ?? undefined,
			rootPath: detected.rootPath,
		});

		// Compute stable surface key (snapshot-independent identity).
		const stableSurfaceKey = computeStableSurfaceKey({
			repoUid,
			moduleCanonicalRootPath: candidate.canonicalRootPath,
			surfaceKind: detected.surfaceKind,
			sourceType: detected.sourceType,
			sourceSpecificId,
		});

		// Compute snapshot-scoped surface UID (same for duplicate detections).
		const surfaceUid = computeProjectSurfaceUid(snapshotUid, stableSurfaceKey);

		// Compute evidence items for this detection.
		const newEvidence: ProjectSurfaceEvidence[] = [];
		for (const ev of detected.evidence) {
			const evidenceUid = computeProjectSurfaceEvidenceUid({
				projectSurfaceUid: surfaceUid,
				sourceType: ev.sourceType,
				sourcePath: ev.sourcePath,
				payload: ev.payload,
			});

			newEvidence.push({
				projectSurfaceEvidenceUid: evidenceUid,
				projectSurfaceUid: surfaceUid,
				snapshotUid,
				repoUid,
				sourceType: ev.sourceType,
				sourcePath: ev.sourcePath,
				evidenceKind: ev.evidenceKind,
				confidence: ev.confidence,
				payloadJson: ev.payload ? JSON.stringify(ev.payload) : null,
			});
		}

		const existing = surfaceByKey.get(stableSurfaceKey);
		if (existing) {
			// Duplicate detection — merge evidence, keep higher-confidence surface.
			// Evidence UIDs are deterministic, so duplicates from merged evidence
			// will have the same UID and deduplicate naturally.
			for (const ev of newEvidence) {
				// Only add if not already present (by UID).
				if (!existing.evidenceItems.some((e) => e.projectSurfaceEvidenceUid === ev.projectSurfaceEvidenceUid)) {
					existing.evidenceItems.push(ev);
				}
			}

			// If this detection has higher confidence, replace the surface
			// but keep accumulated evidence.
			if (detected.confidence > existing.confidence) {
				existing.surface = {
					projectSurfaceUid: surfaceUid,
					snapshotUid,
					repoUid,
					moduleCandidateUid: candidate.moduleCandidateUid,
					surfaceKind: detected.surfaceKind,
					displayName: detected.displayName,
					rootPath: detected.rootPath,
					entrypointPath: detected.entrypointPath,
					buildSystem: detected.buildSystem,
					runtimeKind: detected.runtimeKind,
					confidence: detected.confidence,
					metadataJson: detected.metadata ? JSON.stringify(detected.metadata) : null,
					sourceType: detected.sourceType,
					sourceSpecificId,
					stableSurfaceKey,
				};
				existing.confidence = detected.confidence;
			}
		} else {
			// First detection with this key.
			surfaceByKey.set(stableSurfaceKey, {
				surface: {
					projectSurfaceUid: surfaceUid,
					snapshotUid,
					repoUid,
					moduleCandidateUid: candidate.moduleCandidateUid,
					surfaceKind: detected.surfaceKind,
					displayName: detected.displayName,
					rootPath: detected.rootPath,
					entrypointPath: detected.entrypointPath,
					buildSystem: detected.buildSystem,
					runtimeKind: detected.runtimeKind,
					confidence: detected.confidence,
					metadataJson: detected.metadata ? JSON.stringify(detected.metadata) : null,
					sourceType: detected.sourceType,
					sourceSpecificId,
					stableSurfaceKey,
				},
				evidenceItems: newEvidence,
				confidence: detected.confidence,
			});
		}
	}

	// Flatten deduplicated map to arrays.
	const surfaces: ProjectSurface[] = [];
	const evidence: ProjectSurfaceEvidence[] = [];
	for (const pending of surfaceByKey.values()) {
		surfaces.push(pending.surface);
		for (const ev of pending.evidenceItems) {
			evidence.push(ev);
		}
	}

	return {
		surfaces,
		evidence,
		unattachedSurfaces,
		unattachedCount: unattachedSurfaces.length,
	};
}
