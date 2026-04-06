// UI package — deps are react + react-dom (NOT express).
// React references should classify as external_library_candidate.
// Express references should NOT classify as external here —
// express is not in this package's deps.

import React from "react";
import { useState } from "react";
import sharedUtil from "@shared/util";

export function App() {
	const [count, setCount] = useState(0);
	// Unresolved call to sharedUtil() — callee from alias import.
	// UI tsconfig has no paths of its own, inherits @shared/* from base.
	sharedUtil();
	return React.createElement("div", null, count);
}
