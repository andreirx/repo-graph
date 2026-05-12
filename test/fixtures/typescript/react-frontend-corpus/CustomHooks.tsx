// FD-1B validation: Custom hook detection
import React, { useState, useEffect, useCallback } from 'react';

// Custom hook 1: useLocalStorage
function useLocalStorage<T>(key: string, initialValue: T) {
  const [storedValue, setStoredValue] = useState<T>(() => {
    try {
      const item = window.localStorage.getItem(key);
      return item ? JSON.parse(item) : initialValue;
    } catch {
      return initialValue;
    }
  });

  const setValue = useCallback((value: T) => {
    setStoredValue(value);
    window.localStorage.setItem(key, JSON.stringify(value));
  }, [key]);

  return [storedValue, setValue] as const;
}

// Custom hook 2: useFetch
function useFetch<T>(url: string) {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  useEffect(() => {
    fetch(url)
      .then(res => res.json())
      .then(setData)
      .catch(setError)
      .finally(() => setLoading(false));
  }, [url]);

  return { data, loading, error };
}

// Component using custom hooks
function Settings() {
  const [theme, setTheme] = useLocalStorage('theme', 'light');
  const { data: config } = useFetch<{ version: string }>('/api/config');

  return (
    <div>
      <button onClick={() => setTheme(theme === 'light' ? 'dark' : 'light')}>
        Toggle theme: {theme}
      </button>
      {config && <p>Version: {config.version}</p>}
    </div>
  );
}

export { useLocalStorage, useFetch, Settings };
