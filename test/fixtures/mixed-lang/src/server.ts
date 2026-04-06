// TS file: uses express (from package.json deps)
import express from "express";
const app = express();
app.get("/api", (req, res) => res.json({ ok: true }));
export function start() { /* empty */ }
