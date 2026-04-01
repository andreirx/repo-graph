/** A user in the system. */
export interface User {
	id: string;
	name: string;
	email: string;
}

export type UserId = string;

export enum Role {
	ADMIN = "admin",
	USER = "user",
}
