import type { Entity } from "./entity.js";
import type { Repository } from "./repository.js";

export class BaseService<T extends Entity> {
  constructor(protected readonly repo: Repository<T>) {}
}
