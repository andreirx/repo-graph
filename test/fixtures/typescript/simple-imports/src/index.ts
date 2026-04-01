import { UserRepository } from "./repository.js";
import { UserService } from "./service.js";

const repo = new UserRepository();
const service = new UserService(repo);

export { service };
