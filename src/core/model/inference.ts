import type { InferenceKind } from "./types.js";

/**
 * Computed guess. Always attached to a snapshot.
 * Always carries a confidence score. Never contaminates the facts tables.
 */
export interface Inference {
	inferenceUid: string;
	snapshotUid: string;
	repoUid: string;
	targetStableKey: string;
	kind: InferenceKind;
	valueJson: string;
	/** 0.0 to 1.0 */
	confidence: number;
	/** What evidence supports this inference */
	basisJson: string;
	extractor: string;
	createdAt: string;
}
