// FD-1A validation: NEGATIVE case - dynamic paths should NOT be detected
import express from 'express';

const app = express();
const BASE_URL = '/api/v1';

// Dynamic path with template literal interpolation - should NOT detect
app.get(`${BASE_URL}/users`, (req, res) => {
  res.json({ users: [] });
});

// Another dynamic path - should NOT detect
const VERSION = 'v2';
app.post(`/api/${VERSION}/items`, (req, res) => {
  res.status(201).json({});
});

// Static template literal (no interpolation) - SHOULD detect
app.get(`/static/path`, (req, res) => {
  res.json({});
});
