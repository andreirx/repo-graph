// Server entry — adds the express dep that triggers the
// backend_service surface, plus more env vars and a dynamic-path
// fs mutation. Shared file ownership means everything here is
// also visible from the cli surface.

import express from "express";
import * as fs from "node:fs";

const apiKey = process.env.API_KEY ?? "dev-key";
const dbUrl = process.env.DATABASE_URL;

// Literal create_dir.
fs.mkdirSync("uploads", { recursive: true });

// Dynamic-path write — produces evidence with no identity row.
function persist(target: string, payload: string) {
	fs.writeFile(target, payload, () => {});
}

const app = express();
app.get("/api", (_req, res) => {
	persist("runtime-target", JSON.stringify({ apiKey, dbUrl }));
	res.json({ ok: true });
});

export function start() {
	return app;
}
