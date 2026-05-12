// FD-1B validation: Arrow function component detection
import React from 'react';

// Component 1: Basic arrow function
const Dashboard = () => {
  return (
    <main>
      <h1>Dashboard</h1>
    </main>
  );
};

// Component 2: Arrow with implicit return
const Sidebar = () => (
  <aside>
    <nav>Sidebar Navigation</nav>
  </aside>
);

// Component 3: Arrow with props destructuring
const Button = ({ label, onClick }: { label: string; onClick: () => void }) => (
  <button onClick={onClick}>{label}</button>
);

export { Dashboard, Sidebar, Button };
