// FD-1A validation: server receiver variant
import express from 'express';

const server = express();

// Route: GET /health
server.get('/health', (req, res) => {
  res.json({ status: 'ok' });
});

// Route: GET /metrics
server.get('/metrics', (req, res) => {
  res.json({ uptime: process.uptime() });
});

server.listen(8080);
