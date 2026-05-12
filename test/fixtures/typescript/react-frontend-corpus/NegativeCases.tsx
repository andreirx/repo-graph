// FD-1B validation: Negative cases (should NOT be detected)
import React from 'react';

// Negative 1: lowercase function returning JSX (not a component by convention)
function helper() {
  return <div>Helper</div>;
}

// Negative 2: function not returning JSX
function calculateTotal(items: number[]): number {
  return items.reduce((sum, item) => sum + item, 0);
}

// Negative 3: arrow function not returning JSX
const formatDate = (date: Date) => date.toISOString();

// Negative 4: PascalCase but async (could be confused but has no JSX)
async function DataLoader() {
  const response = await fetch('/api/data');
  return response.json();
}

export { helper, calculateTotal, formatDate, DataLoader };
