/**
 * Surface discovery orchestrator — pure core logic.
 *
 * Takes detected surfaces from manifest detectors and links them
 * to existing module candidates. Produces ProjectSurface[] and
 * ProjectSurfaceEvidence[] for persistence.
 *
 * Policy: only persist surfaces linked to an existing module candidate.
 * If no module candidate exists for a detected surface's root path,
 * the surface is dropped. This is documented as a known limitation
 * (TECH-DEBT) — unattached surface handling is deferred.
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
	/** Count of surfaces dropped because no module candidate matched. */
	unattachedDropped: number;
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
	let unattachedDropped = 0;

	for (const detected of detectedSurfaces) {
		// Find the owning module candidate by root path match.
		const candidate = candidateByRoot.get(detected.rootPath);
		if (!candidate) {
			unattachedDropped++;
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

	return { surfaces, evidence, unattachedDropped };
}
