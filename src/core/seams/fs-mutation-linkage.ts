/**
 * Filesystem mutation linkage — links file-local mutation detections
 * to surfaces.
 *
 * Pure function. No filesystem, no storage.
 *
 * Linkage policy:
 *   - File path → owning module candidate → attached surfaces
 *   - Identity unit: (project_surface_uid, target_path, mutation_kind)
 *   - Literal-path mutations create identity + evidence rows
 *   - Dynamic-path mutations create evidence rows only (no identity)
 *   - Same path + different kind → separate identity rows
 *   - Same path + same kind across multiple files → one identity row,
 *     multiple evidence rows
 *   - Files with no owning surface are dropped
 *
 * Rename/copy semantics: target_path is the SOURCE path. Destination
 * (if known) is preserved in evidence metadata. Document this in
 * the slice notes.
 */

import { v4 as uuidv4 } from "uuid";
import type {
	DetectedFsMutation,
	SurfaceFsMutation,
	SurfaceFsMutationEvidence,
} from "./fs-mutation.js";

// ── Input/Output ───────────────────────────────────────────────────

export interface FsMutationLinkageInput {
	repoUid: string;
	snapshotUid: string;
	detectedMutations: DetectedFsMutation[];
	/**
	 * Map: filePath → projectSurfaceUid[].
	 * A file can belong to multiple surfaces. Same shape as env linkage.
	 */
	fileToSurfaces: Map<string, string[]>;
}

export interface FsMutationLinkageResult {
	identities: SurfaceFsMutation[];
	evidence: SurfaceFsMutationEvidence[];
	/** Mutations dropped because the source file has no owning surface. */
	unlinkedDropped: number;
}

// ── Orchestrator ───────────────────────────────────────────────────

export function linkFsMutations(input: FsMutationLinkageInput): FsMutationLinkageResult {
	const { repoUid, snapshotUid, detectedMutations, fileToSurfaces } = input;

	// Group LITERAL-path mutations by (surface, target_path, kind).
	// Dynamic-path mutations skip identity grouping entirely.
	const grouped = new Map<string, {
		surfaceUid: string;
		targetPath: string;
		mutationKind: DetectedFsMutation["mutationKind"];
		occurrences: DetectedFsMutation[];
	}>();
	const dynamicOccurrences: Array<{
		surfaceUid: string;
		mutation: DetectedFsMutation;
	}> = [];
	let unlinkedDropped = 0;

	for (const mutation of detectedMutations) {
		const surfaceUids = fileToSurfaces.get(mutation.filePath);
		if (!surfaceUids || surfaceUids.length === 0) {
			unlinkedDropped++;
			continue;
		}

		for (const surfaceUid of surfaceUids) {
			if (mutation.dynamicPath || mutation.targetPath === null) {
				// Dynamic path: evidence only, no identity row.
				dynamicOccurrences.push({ surfaceUid, mutation });
				continue;
			}

			const key = `${surfaceUid}|${mutation.targetPath}|${mutation.mutationKind}`;
			const existing = grouped.get(key);
			if (existing) {
				existing.occurrences.push(mutation);
			} else {
				grouped.set(key, {
					surfaceUid,
					targetPath: mutation.targetPath,
					mutationKind: mutation.mutationKind,
					occurrences: [mutation],
				});
			}
		}
	}

	const identities: SurfaceFsMutation[] = [];
	const evidence: SurfaceFsMutationEvidence[] = [];

	// Build identity + evidence rows for literal-path groups.
	for (const group of grouped.values()) {
		const identityUid = uuidv4();
		const confidence = Math.max(...group.occurrences.map((o) => o.confidence));

		// Collect distinct destination paths across occurrences (for rename/copy).
		const destinations = new Set<string>();
		for (const occ of group.occurrences) {
			if (occ.destinationPath) destinations.add(occ.destinationPath);
		}
		const identityMetadata = destinations.size > 0
			? JSON.stringify({ destinationPaths: [...destinations].sort() })
			: null;

		identities.push({
			surfaceFsMutationUid: identityUid,
			snapshotUid,
			repoUid,
			projectSurfaceUid: group.surfaceUid,
			targetPath: group.targetPath,
			mutationKind: group.mutationKind,
			confidence,
			metadataJson: identityMetadata,
		});

		for (const occ of group.occurrences) {
			evidence.push({
				surfaceFsMutationEvidenceUid: uuidv4(),
				surfaceFsMutationUid: identityUid,
				snapshotUid,
				repoUid,
				projectSurfaceUid: group.surfaceUid,
				sourceFilePath: occ.filePath,
				lineNumber: occ.lineNumber,
				mutationKind: occ.mutationKind,
				mutationPattern: occ.mutationPattern,
				dynamicPath: false,
				confidence: occ.confidence,
				metadataJson: occ.destinationPath
					? JSON.stringify({ destinationPath: occ.destinationPath })
					: null,
			});
		}
	}

	// Evidence-only rows for dynamic-path occurrences.
	for (const { surfaceUid, mutation } of dynamicOccurrences) {
		evidence.push({
			surfaceFsMutationEvidenceUid: uuidv4(),
			surfaceFsMutationUid: null,
			snapshotUid,
			repoUid,
			projectSurfaceUid: surfaceUid,
			sourceFilePath: mutation.filePath,
			lineNumber: mutation.lineNumber,
			mutationKind: mutation.mutationKind,
			mutationPattern: mutation.mutationPattern,
			dynamicPath: true,
			confidence: mutation.confidence,
			metadataJson: null,
		});
	}

	return { identities, evidence, unlinkedDropped };
}
