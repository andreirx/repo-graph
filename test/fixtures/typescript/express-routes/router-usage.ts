// FD-1A validation: Express Router patterns
import { Router } from 'express';

const router = Router();

// Route: GET /items
router.get('/items', (req, res) => {
  res.json({ items: [] });
});

// Route: POST /items
router.post('/items', (req, res) => {
  res.status(201).json({ created: true });
});

// Route: DELETE /items/:itemId
router.delete('/items/:itemId', (req, res) => {
  res.status(204).send();
});

export default router;
