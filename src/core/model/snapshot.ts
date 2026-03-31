import type { SnapshotKind, SnapshotStatus } from "./types.js";

/**
 * A snapshot represents the state of a repo's extracted knowledge
 * at a point in time. Immutable once sealed.
 */
export interface Snapshot {
	snapshotUid: string;
	repoUid: string;
	parentSnapshotUid: string | null;
	kind: SnapshotKind;
	basisRef: string | null;
	basisCommit: string | null;
	dirtyHash: string | null;
	status: SnapshotStatus;
	filesTotal: number;
	nodesTotal: number;
	edgesTotal: number;
	createdAt: string;
	completedAt: string | null;
	label: string | null;
}
