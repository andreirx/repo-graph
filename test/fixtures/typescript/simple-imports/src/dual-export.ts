/**
 * Regression fixture: TypeScript "companion type" pattern.
 * A const and a type alias share the same name. Both are legal
 * and occupy different declaration spaces (value vs type).
 * The extractor must produce distinct stable_keys for each.
 */

export const Status = {
	ACTIVE: "active",
	INACTIVE: "inactive",
} as const;

export type Status = (typeof Status)[keyof typeof Status];

export const Priority = {
	LOW: 1,
	MEDIUM: 2,
	HIGH: 3,
} as const;

export type Priority = (typeof Priority)[keyof typeof Priority];
