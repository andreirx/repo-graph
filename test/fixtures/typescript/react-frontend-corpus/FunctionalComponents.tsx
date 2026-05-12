// FD-1B validation: Functional component detection
import React from 'react';

// Component 1: Basic function component
function UserProfile() {
  return (
    <div className="user-profile">
      <h1>User Profile</h1>
    </div>
  );
}

// Component 2: Function with props
function UserCard({ name, email }: { name: string; email: string }) {
  return (
    <div className="user-card">
      <span>{name}</span>
      <span>{email}</span>
    </div>
  );
}

// Component 3: Function returning fragment
function NavigationLinks() {
  return (
    <>
      <a href="/home">Home</a>
      <a href="/about">About</a>
    </>
  );
}

export { UserProfile, UserCard, NavigationLinks };
