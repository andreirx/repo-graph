import express from "express";
const app = express();

app.get("/api/v2/products", async (req, res) => {
  res.json([]);
});

app.get("/api/v2/products/:id", async (req, res) => {
  res.json({});
});

app.post("/api/v2/products", async (req, res) => {
  res.json({});
});

app.delete("/api/v2/products/:id", async (req, res) => {
  res.sendStatus(204);
});

app.listen(3000);
