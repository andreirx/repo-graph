import type { EdgeType, Resolution, SourceLocation } from "./types.js";

/**
 * A graph edge. Connects any node to any node.
 * Typed, with resolution and extractor provenance.
 */
export interface GraphEdge {
	edgeUid: string;
	snapshotUid: string;
	repoUid: string;
	sourceNodeUid: string;
	targetNodeUid: string;
	type: EdgeType;
	resolution: Resolution;
	/** Name and version of the extractor that produced this edge. e.g. "ts-core:0.1.0" */
	extractor: string;
	location: SourceLocation | null;
	metadataJson: string | null;
}
