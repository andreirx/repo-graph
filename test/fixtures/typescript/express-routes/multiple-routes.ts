// FD-1A validation: multiple routes in one file
import express from 'express';

const app = express();

// CRUD routes for products
app.get('/products', listProducts);
app.get('/products/:id', getProduct);
app.post('/products', createProduct);
app.put('/products/:id', updateProduct);
app.patch('/products/:id', patchProduct);
app.delete('/products/:id', deleteProduct);

// Middleware mount
app.use('/api', apiMiddleware);

function listProducts(req: any, res: any) { res.json([]); }
function getProduct(req: any, res: any) { res.json({}); }
function createProduct(req: any, res: any) { res.status(201).json({}); }
function updateProduct(req: any, res: any) { res.json({}); }
function patchProduct(req: any, res: any) { res.json({}); }
function deleteProduct(req: any, res: any) { res.status(204).send(); }
function apiMiddleware(req: any, res: any, next: any) { next(); }
