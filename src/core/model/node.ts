import type {
	NodeKind,
	NodeSubtype,
	SourceLocation,
	Visibility,
} from "./types.js";

/**
 * A graph node. Everything is a node: symbols, files, modules,
 * endpoints, events, tables, configs, tests, states.
 * Differentiated by kind + subtype.
 */
export interface GraphNode {
	nodeUid: string;
	snapshotUid: string;
	repoUid: string;
	/** Portable cross-system identity. Format: "<repoUid>:<path>#<qualifiedName>:<kind>" */
	stableKey: string;
	kind: NodeKind;
	subtype: NodeSubtype | null;
	name: string;
	qualifiedName: string | null;
	fileUid: string | null;
	parentNodeUid: string | null;
	location: SourceLocation | null;
	signature: string | null;
	visibility: Visibility | null;
	docComment: string | null;
	metadataJson: string | null;
}
