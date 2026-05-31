// Synthetic INGEST-CORE-1 fixture (committed). Designed to exercise every invariant
// the 4c harness asserts. Frozen: regenerate index.scip only via the documented step.

export class Circle {
  radius: number;

  // Explicit constructor in a NON-abstract class -> ts-extractor emits an AST node;
  // SCIP `<constructor>` reconciles to it (constructor reconciliation).
  constructor(radius: number) {
    this.radius = radius;
  }

  // Getter -> SCIP `<get>area` reconciles to the AST `area` getter (getter reconciliation).
  get area(): number {
    return 3.14 * this.radius * this.radius;
  }

  // Branch -> cyclomatic complexity > 1 (complexity attachment).
  describe(): string {
    if (this.radius > 10) {
      return "big";
    }
    return "small";
  }
}

export abstract class Shape {
  label: string;

  // Explicit constructor in an ABSTRACT class -> ts-extractor emits NO AST node;
  // SCIP `<constructor>` stays a labeled fallback (proven coverage gap).
  constructor(label: string) {
    this.label = label;
  }

  abstract size(): number;
}
