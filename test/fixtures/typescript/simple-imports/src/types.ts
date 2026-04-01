/** A user in the system. */
export interface User {
	id: string;
	name: string;
	email: string;
	getDisplayName(): string;
}

export type UserId = string;

/** Interface with overloaded method signatures. */
export interface Formatter {
	format(value: string): string;
	format(value: number): string;
	format(value: Date): string;
}

export enum Role {
	ADMIN = "admin",
	USER = "user",
}
