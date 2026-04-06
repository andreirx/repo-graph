// Fixture: various obj.method() call patterns for receiver type resolution.

// Array receiver — type should be "string[]" or similar
const lines = "hello\nworld".split("\n");
lines.map((l) => l.trim());

// Map receiver — type should be "Map<string, number>"
const scores = new Map<string, number>();
scores.set("a", 1);
scores.get("a");

// String receiver — type should be "string"
let greeting = "hello";
greeting.toUpperCase();

// Date receiver — type should be "Date"
const now = new Date();
now.toISOString();

// Class instance receiver — type should be "Greeter"
class Greeter {
	greet(): string {
		return "hi";
	}
}
const g = new Greeter();
g.greet();

export {};
