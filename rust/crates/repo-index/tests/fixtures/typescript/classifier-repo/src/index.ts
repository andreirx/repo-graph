// Fixture file exercising each classifier bucket + basis code.
// Every call below is designed to remain UNRESOLVED after the
// indexer's resolution pass, so each one becomes a row in
// unresolved_edges with a specific classification.

import debounce from "lodash";
import aliased from "@/lib/missing";
import relatively from "./local-nonexistent";

// external_library_candidate / callee_matches_external_import
debounce();

// internal_candidate / callee_matches_internal_import (via tsconfig alias)
aliased();

// internal_candidate / callee_matches_internal_import (via relative import)
relatively();

// unknown / no_supporting_signal
mysteryFunction();

// Static declaration of a symbol that IS invoked elsewhere-unreferenced.
// The declaration itself is resolved; this function body is empty.
export function standalone(): void {
	// nothing
}
