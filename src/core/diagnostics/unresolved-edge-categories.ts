/**
 * Machine-stable diagnostic categories for unresolved edges.
 *
 * Human-readable labels are rendered from these machine keys at
 * display time — they are NOT encoded into storage or JSON output.
 *
 * Keys are versioned as part of the snapshot's extraction_diagnostics
 * payload. Adding a new key is non-breaking. Removing or renaming an
 * existing key is breaking and must bump diagnostics_version.
 *
 * See docs/architecture/gate-contract.txt for the contract discipline
 * applied here: machine identity first, presentation layered on top.
 */

export const UnresolvedEdgeCategory = {
	IMPORTS_FILE_NOT_FOUND: "imports_file_not_found",
	INSTANTIATES_CLASS_NOT_FOUND: "instantiates_class_not_found",
	IMPLEMENTS_INTERFACE_NOT_FOUND: "implements_interface_not_found",
	CALLS_THIS_WILDCARD_METHOD_NEEDS_TYPE_INFO:
		"calls_this_wildcard_method_needs_type_info",
	CALLS_THIS_METHOD_NEEDS_CLASS_CONTEXT:
		"calls_this_method_needs_class_context",
	CALLS_OBJ_METHOD_NEEDS_TYPE_INFO: "calls_obj_method_needs_type_info",
	CALLS_FUNCTION_AMBIGUOUS_OR_MISSING: "calls_function_ambiguous_or_missing",
	OTHER: "other",
} as const;

export type UnresolvedEdgeCategory =
	(typeof UnresolvedEdgeCategory)[keyof typeof UnresolvedEdgeCategory];

/**
 * Human-readable labels for diagnostic categories. Used only at
 * render time (CLI table formatter, trust report display). Consumers
 * that act on diagnostics programmatically should use the machine
 * keys directly.
 */
const HUMAN_LABELS: Record<UnresolvedEdgeCategory, string> = {
	imports_file_not_found: "IMPORTS (file not found)",
	instantiates_class_not_found: "INSTANTIATES (class not found)",
	implements_interface_not_found: "IMPLEMENTS (interface not found)",
	calls_this_wildcard_method_needs_type_info:
		"CALLS this.*.method (needs type info)",
	calls_this_method_needs_class_context:
		"CALLS this.method (needs class context)",
	calls_obj_method_needs_type_info: "CALLS obj.method (needs type info)",
	calls_function_ambiguous_or_missing:
		"CALLS function (ambiguous or missing)",
	other: "OTHER (unclassified)",
};

export function humanLabelForCategory(
	category: UnresolvedEdgeCategory | string,
): string {
	return HUMAN_LABELS[category as UnresolvedEdgeCategory] ?? category;
}

/**
 * Categories identified as CALLS-family. Used by reliability formulas
 * that compute the call-graph resolution rate.
 */
export const CALLS_CATEGORIES: readonly UnresolvedEdgeCategory[] =
	Object.freeze([
		UnresolvedEdgeCategory.CALLS_THIS_WILDCARD_METHOD_NEEDS_TYPE_INFO,
		UnresolvedEdgeCategory.CALLS_THIS_METHOD_NEEDS_CLASS_CONTEXT,
		UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
		UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
	]);

/**
 * Categories identified as IMPORTS-family. Used by reliability
 * formulas that judge import-graph completeness.
 */
export const IMPORTS_CATEGORIES: readonly UnresolvedEdgeCategory[] =
	Object.freeze([UnresolvedEdgeCategory.IMPORTS_FILE_NOT_FOUND]);
