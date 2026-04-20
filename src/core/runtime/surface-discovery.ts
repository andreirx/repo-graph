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

import { v4 as uuidv4 } from "uuid";
import type { ModuleCandidate } from "../modules/module-candidate.js";
import type {
	ProjectSurface,
	ProjectSurfaceEvidence,
} from "./project-surface.js";
import type { DetectedSurface } from "./surface-detectors.js";

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

// ── Orchestrator ───────────────────────────────────────────────────

/**
 * Link detected surfaces to module candidates and produce persistence
 * rows. Surfaces without a matching module candidate are dropped.
 */
export function discoverSurfaces(input: SurfaceDiscoveryInput): SurfaceDiscoveryResult {
	const { repoUid, snapshotUid, detectedSurfaces, moduleCandidates } = input;

	// Build module candidate lookup by root path.
	const candidateByRoot = new Map<string, ModuleCandidate>();
	for (const mc of moduleCandidates) {
		candidateByRoot.set(mc.canonicalRootPath, mc);
	}

	const surfaces: ProjectSurface[] = [];
	const evidence: ProjectSurfaceEvidence[] = [];
	const unattachedSurfaces: DetectedSurface[] = [];

	for (const detected of detectedSurfaces) {
		// Find the owning module candidate by root path match.
		const candidate = candidateByRoot.get(detected.rootPath);
		if (!candidate) {
			// Collect for Layer 2 promotion instead of dropping.
			unattachedSurfaces.push(detected);
			continue;
		}

		const surfaceUid = uuidv4();

		surfaces.push({
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
		});

		for (const ev of detected.evidence) {
			evidence.push({
				projectSurfaceEvidenceUid: uuidv4(),
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
	}

	return {
		surfaces,
		evidence,
		unattachedSurfaces,
		unattachedCount: unattachedSurfaces.length,
	};
}
