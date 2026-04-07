import axios from "axios";

export async function listProducts() {
  return axios.get("/api/v2/products");
}

export async function getProduct(id: string) {
  return axios.get(`/api/v2/products/${id}`);
}

export async function fetchOrders() {
  return axios.get("/api/v2/orders");
}
