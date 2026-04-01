import type { UserRepository } from "./repository.js";
import { Role, type User } from "./types.js";

export class UserService {
	constructor(private repo: UserRepository) {}

	async getUser(id: string): Promise<User | null> {
		return this.repo.findById(id);
	}

	async createUser(name: string, email: string): Promise<User> {
		const user: User = {
			id: generateId(),
			name,
			email,
		};
		await this.repo.save(user);
		return user;
	}

	async promoteToAdmin(userId: string): Promise<void> {
		const user = await this.repo.findById(userId);
		if (!user) throw new Error("User not found");
		await this.repo.updateRole(userId, Role.ADMIN);
	}
}

function generateId(): string {
	return Math.random().toString(36).slice(2);
}
