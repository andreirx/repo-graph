// UI package — deps are react + react-dom (NOT express).
// React references should classify as external_library_candidate.
// Express references should NOT classify as external here —
// express is not in this package's deps.

import React from "react";
import { useState } from "react";

export function App() {
	const [count, setCount] = useState(0);
	return React.createElement("div", null, count);
}
