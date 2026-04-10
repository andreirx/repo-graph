// CLI entry — env vars + literal fs mutations + a rename.
// This file is shared between the cli surface and the backend_service
// surface because both surfaces own the package root.

import * as fs from "node:fs";

const port = process.env.PORT;
const dbUrl = process.env.DATABASE_URL;
const debug = process.env["DEBUG"];

// Literal fs mutations.
fs.writeFileSync("logs/app.log", "boot");
fs.unlinkSync("data/cache.json");
fs.renameSync("tmp/staging.txt", "data/final.txt");

export function start() {
	return { port, dbUrl, debug };
}
