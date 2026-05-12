// FD-1A validation: basic Express app patterns
import express from 'express';

const app = express();

// Route: GET /api/users
app.get('/api/users', (req, res) => {
  res.json({ users: [] });
});

// Route: POST /api/users
app.post('/api/users', (req, res) => {
  res.status(201).json({ created: true });
});

// Route: GET /api/users/:id (path param)
app.get('/api/users/:id', (req, res) => {
  res.json({ id: req.params.id });
});

app.listen(3000);
