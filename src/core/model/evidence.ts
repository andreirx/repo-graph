import type { ArtifactKind } from "./types.js";

/**
 * Reference to a heavy evidence file stored outside the database.
 * The database stores metadata. The file system (or object store) stores content.
 */
export interface Artifact {
	artifactUid: string;
	snapshotUid: string;
	repoUid: string;
	kind: ArtifactKind;
	/** Path relative to artifact store root */
	relativePath: string;
	contentHash: string | null;
	sizeBytes: number | null;
	format: string | null;
	createdAt: string;
}

/**
 * Links an artifact to a node, edge, or inference it supports.
 */
export interface EvidenceLink {
	evidenceLinkUid: string;
	snapshotUid: string;
	subjectType: "node" | "edge" | "inference";
	subjectUid: string;
	artifactUid: string;
	note: string | null;
}
