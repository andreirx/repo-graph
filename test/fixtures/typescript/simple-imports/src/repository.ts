import type { Role, User } from "./types.js";

export class UserRepository {
	private users: Map<string, User> = new Map();

	async findById(id: string): Promise<User | null> {
		return this.users.get(id) ?? null;
	}

	async save(user: User): Promise<void> {
		this.users.set(user.id, user);
	}

	async updateRole(userId: string, role: Role): Promise<void> {
		const user = this.users.get(userId);
		if (user) {
			console.log(`Updated role for ${userId} to ${role}`);
		}
	}
}
