import axios from "axios";

export async function fetchProduct(id: string) {
  return axios.get(`/api/v2/products/${id}`);
}

export async function createProduct(data: any) {
  return axios.post("/api/v2/products", data);
}

export async function fetchOrders() {
  return axios.get("/api/v2/orders");
}
