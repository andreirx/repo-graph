// FD-1B validation: React hook usage detection
import React, { useState, useEffect, useCallback, useMemo, useRef, useContext } from 'react';

const ThemeContext = React.createContext('light');

// Component with multiple built-in hooks
function Counter() {
  // Hook 1: useState
  const [count, setCount] = useState(0);

  // Hook 2: useRef
  const buttonRef = useRef<HTMLButtonElement>(null);

  // Hook 3: useEffect
  useEffect(() => {
    document.title = `Count: ${count}`;
    return () => {
      document.title = 'React App';
    };
  }, [count]);

  // Hook 4: useCallback
  const increment = useCallback(() => {
    setCount(c => c + 1);
  }, []);

  // Hook 5: useMemo
  const doubled = useMemo(() => count * 2, [count]);

  // Hook 6: useContext
  const theme = useContext(ThemeContext);

  return (
    <div className={`counter ${theme}`}>
      <p>Count: {count} (doubled: {doubled})</p>
      <button ref={buttonRef} onClick={increment}>Increment</button>
    </div>
  );
}

export { Counter, ThemeContext };
