import type { Entity } from "./entity.js";

export interface Repository<T extends Entity> {
  findById(id: string): Promise<T | null>;
  save(entity: T): Promise<void>;
}
