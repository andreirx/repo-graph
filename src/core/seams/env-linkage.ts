/**
 * Env dependency linkage — links file-local env detections to surfaces.
 *
 * Pure function. No filesystem, no storage.
 *
 * Takes:
 *   - Detected env accesses (file-local facts from detectors)
 *   - File-to-surface ownership (from module_file_ownership + surfaces)
 *
 * Produces:
 *   - SurfaceEnvDependency[] (identity: one per surface/env_name)
 *   - SurfaceEnvEvidence[] (per-occurrence)
 *
 * Linkage policy:
 *   - File path → owning module candidate → attached surfaces
 *   - If a file is owned by a module with surfaces, its env accesses
 *     become dependencies of those surfaces
 *   - If a file has no owning surface, its accesses are dropped
 *     (documented as TECH-DEBT)
 *   - One env var accessed in multiple files → one dependency row,
 *     multiple evidence rows
 *   - access_kind on the dependency = "required" if ANY occurrence
 *     is required, else "optional" if any is optional, else "unknown"
 */

import { v4 as uuidv4 } from "uuid";
import type {
	DetectedEnvDependency,
	EnvAccessKind,
	SurfaceEnvDependency,
	SurfaceEnvEvidence,
} from "./env-dependency.js";

// ── Input/Output ───────────────────────────────────────────────────

export interface EnvLinkageInput {
	repoUid: string;
	snapshotUid: string;
	/** File-local detected env accesses. */
	detectedAccesses: DetectedEnvDependency[];
	/**
	 * Map: filePath → projectSurfaceUid[].
	 * A file can belong to multiple surfaces (rare but possible).
	 * Built by the caller from module_file_ownership + project_surfaces.
	 */
	fileToSurfaces: Map<string, string[]>;
}

export interface EnvLinkageResult {
	dependencies: SurfaceEnvDependency[];
	evidence: SurfaceEnvEvidence[];
	/** Accesses dropped because the file has no owning surface. */
	unlinkedDropped: number;
}

// ── Orchestrator ───────────────────────────────────────────────────

export function linkEnvDependencies(input: EnvLinkageInput): EnvLinkageResult {
	const { repoUid, snapshotUid, detectedAccesses, fileToSurfaces } = input;

	// Group accesses by surface + env name.
	// Key: surfaceUid|envName → occurrences
	const grouped = new Map<string, {
		surfaceUid: string;
		envName: string;
		occurrences: DetectedEnvDependency[];
	}>();
	let unlinkedDropped = 0;

	for (const access of detectedAccesses) {
		const surfaceUids = fileToSurfaces.get(access.filePath);
		if (!surfaceUids || surfaceUids.length === 0) {
			unlinkedDropped++;
			continue;
		}

		for (const surfaceUid of surfaceUids) {
			const key = `${surfaceUid}|${access.varName}`;
			const existing = grouped.get(key);
			if (existing) {
				existing.occurrences.push(access);
			} else {
				grouped.set(key, {
					surfaceUid,
					envName: access.varName,
					occurrences: [access],
				});
			}
		}
	}

	// Build identity + evidence rows.
	const dependencies: SurfaceEnvDependency[] = [];
	const evidence: SurfaceEnvEvidence[] = [];

	for (const group of grouped.values()) {
		const depUid = uuidv4();

		// Aggregate access kind: required > optional > unknown.
		const accessKind = aggregateAccessKind(group.occurrences);

		// First default value found.
		const defaultValue = group.occurrences.find((o) => o.defaultValue)?.defaultValue ?? null;

		// Max confidence across occurrences.
		const confidence = Math.max(...group.occurrences.map((o) => o.confidence));

		dependencies.push({
			surfaceEnvDependencyUid: depUid,
			snapshotUid,
			repoUid,
			projectSurfaceUid: group.surfaceUid,
			envName: group.envName,
			accessKind,
			defaultValue,
			confidence,
			metadataJson: null,
		});

		for (const occ of group.occurrences) {
			evidence.push({
				surfaceEnvEvidenceUid: uuidv4(),
				surfaceEnvDependencyUid: depUid,
				snapshotUid,
				repoUid,
				sourceFilePath: occ.filePath,
				lineNumber: occ.lineNumber,
				accessPattern: occ.accessPattern,
				confidence: occ.confidence,
				metadataJson: null,
			});
		}
	}

	return { dependencies, evidence, unlinkedDropped };
}

// ── Helpers ────────────────────────────────────────────────────────

function aggregateAccessKind(occurrences: DetectedEnvDependency[]): EnvAccessKind {
	let hasRequired = false;
	let hasOptional = false;
	for (const o of occurrences) {
		if (o.accessKind === "required") hasRequired = true;
		if (o.accessKind === "optional") hasOptional = true;
	}
	if (hasRequired) return "required";
	if (hasOptional) return "optional";
	return "unknown";
}
