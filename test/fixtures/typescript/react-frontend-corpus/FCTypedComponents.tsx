// FD-1B validation: React.FC typed component detection
import React from 'react';

interface CardProps {
  title: string;
  children: React.ReactNode;
}

// Component 1: React.FC with generic type
const Card: React.FC<CardProps> = ({ title, children }) => {
  return (
    <article className="card">
      <h2>{title}</h2>
      <div className="card-content">{children}</div>
    </article>
  );
};

interface ModalProps {
  isOpen: boolean;
  onClose: () => void;
}

// Component 2: React.FC with multiple props
const Modal: React.FC<ModalProps> = ({ isOpen, onClose }) => {
  if (!isOpen) return null;
  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content">Modal</div>
    </div>
  );
};

export { Card, Modal };
