/**
 * Effective-verdict resolver.
 *
 * Thin wrapper over evaluateObligation() that layers the governance
 * overlay (active waivers) on top of the computed evaluation.
 *
 * Architectural principle: the computed verdict is TRUTH about the
 * evaluation. The effective verdict is POLICY about how that truth is
 * acted on. Waivers suppress gate failure; they do NOT erase the
 * underlying computed verdict.
 *
 * The returned EffectiveObligationVerdict exposes BOTH:
 *   - computed_verdict: the raw result of evaluateObligation
 *   - effective_verdict: computed_verdict, OR "WAIVED" if an active
 *     waiver matches the obligation's (req_id, version, obligation_id)
 *
 * The waiver_basis field captures the full audit trail of the waiver
 * that applied: waiver uid, reason, authorship, expiry, and policy
 * basis. Callers that need to distinguish "passed on merit" from
 * "waived" can read effective_verdict; callers that need the audit
 * trail can read waiver_basis.
 *
 * See docs/VISION.md and docs/architecture/measurement-model.txt for
 * the computed-vs-effective audit preservation principle.
 */

import type { VerificationObligation } from "../model/declaration.js";
import type { StoragePort } from "../ports/storage.js";
import {
	evaluateObligation,
	type Verdict,
} from "./obligation-evaluator.js";

/**
 * Five-state verdict enum used at the gate boundary.
 * WAIVED is ONLY an effective state — never a computed state.
 */
export type EffectiveVerdict = Verdict | "WAIVED";

/**
 * Audit trail of the waiver that suppressed a computed verdict.
 * Fields mirror WaiverDeclarationValue but are flattened for JSON
 * consumption by gate and evidence output surfaces.
 */
export interface WaiverBasis {
	waiver_uid: string;
	reason: string;
	created_at: string;
	created_by: string | null;
	expires_at: string | null;
	rationale_category: string | null;
	policy_basis: string | null;
}

export interface EffectiveObligationVerdict {
	obligation_id: string;
	obligation: string;
	method: string;
	target: string | null;
	threshold: number | null;
	operator: string | null;
	/** Raw result from evaluateObligation. Never erased. */
	computed_verdict: Verdict;
	/** Computed verdict OR "WAIVED" if an active waiver applies. */
	effective_verdict: EffectiveVerdict;
	/** Evidence from the computed evaluation. */
	evidence: Record<string, unknown>;
	/** Waiver audit trail, non-null iff effective_verdict === "WAIVED". */
	waiver_basis: WaiverBasis | null;
}

/**
 * Evaluate an obligation and layer waiver resolution on top.
 *
 * Lookup semantics:
 *   - matches waivers with exactly (reqId, requirementVersion, obligation_id)
 *   - excludes expired waivers (via storage.findActiveWaivers)
 *   - when multiple active waivers match, uses the most recently
 *     created (the first row returned, since findActiveWaivers
 *     orders by created_at DESC)
 *
 * Parameters:
 *   @param obl - The obligation being evaluated.
 *   @param reqId - Parent requirement's req_id.
 *   @param requirementVersion - Parent requirement's current version.
 *   @param storage - Storage port for both evaluation and waiver lookup.
 *   @param snapshotUid - Snapshot scope for evaluation.
 *   @param repoUid - Repository scope.
 *   @param now - Optional ISO 8601 timestamp for expiry comparison.
 *                Omit to use current time.
 */
export function resolveEffectiveVerdict(
	obl: VerificationObligation,
	reqId: string,
	requirementVersion: number,
	storage: StoragePort,
	snapshotUid: string,
	repoUid: string,
	now?: string,
): EffectiveObligationVerdict {
	// Compute the underlying verdict first. This is always executed,
	// regardless of waiver state, so that the audit trail is complete.
	const computed = evaluateObligation(obl, storage, snapshotUid, repoUid);

	// Look up any active, non-expired waiver matching the exact tuple.
	const waivers = storage.findActiveWaivers({
		repoUid,
		reqId,
		requirementVersion,
		obligationId: obl.obligation_id,
		now,
	});

	if (waivers.length === 0) {
		return {
			obligation_id: computed.obligation_id,
			obligation: computed.obligation,
			method: computed.method,
			target: computed.target,
			threshold: computed.threshold,
			operator: computed.operator,
			computed_verdict: computed.verdict,
			effective_verdict: computed.verdict,
			evidence: computed.evidence,
			waiver_basis: null,
		};
	}

	// Most recent waiver wins (findActiveWaivers orders created_at DESC).
	const waiver = waivers[0];
	const waiverValue = JSON.parse(waiver.valueJson) as {
		reason: string;
		created_at: string;
		created_by?: string;
		expires_at?: string;
		rationale_category?: string;
		policy_basis?: string;
	};

	return {
		obligation_id: computed.obligation_id,
		obligation: computed.obligation,
		method: computed.method,
		target: computed.target,
		threshold: computed.threshold,
		operator: computed.operator,
		computed_verdict: computed.verdict,
		effective_verdict: "WAIVED",
		evidence: computed.evidence,
		waiver_basis: {
			waiver_uid: waiver.declarationUid,
			reason: waiverValue.reason,
			created_at: waiverValue.created_at,
			created_by: waiverValue.created_by ?? null,
			expires_at: waiverValue.expires_at ?? null,
			rationale_category: waiverValue.rationale_category ?? null,
			policy_basis: waiverValue.policy_basis ?? null,
		},
	};
}
