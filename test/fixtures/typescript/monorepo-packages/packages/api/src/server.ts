// API package — deps are express + cors (NOT react).
// express and cors should classify as external_library_candidate.
// React references should NOT classify as external here —
// react is not in this package's deps.

import express from "express";
import cors from "cors";
import apiHelper from "@api/helper";

const app = express();
app.use(cors());

// Unresolved call to express() and cors() — callees from external import.
// Unresolved call to app.use() — receiver from same-file value.
// Unresolved call to apiHelper() — callee from alias import (@api/*).
apiHelper();

export function startServer(): void {
	// empty
}
