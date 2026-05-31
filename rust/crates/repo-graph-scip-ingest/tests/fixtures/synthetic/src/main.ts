// Synthetic INGEST-CORE-1 fixture (committed). See shapes.ts.

import { Circle } from "./shapes"; // top-level import -> file-scope reference (FILE caller)

export function makeCircle(radius: number): Circle {
  return new Circle(radius); // cross-file reference to Circle (instantiation, not a call)
}

export function report(radius: number): string {
  const circle = makeCircle(radius); // same-file call: report -> makeCircle
  return circle.describe(); // cross-file call: report -> Circle.describe (shapes.ts)
}
