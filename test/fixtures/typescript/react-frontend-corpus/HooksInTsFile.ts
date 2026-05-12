// FD-1B-EXT validation: React hooks in a plain .ts file (no JSX)
import { useState, useEffect } from 'react';

// Custom hook in a .ts file - should be detected
export function useCounter(initial: number) {
    const [count, setCount] = useState(initial);

    useEffect(() => {
        console.log('Count changed:', count);
    }, [count]);

    return { count, setCount };
}

// Another custom hook - should be detected
export function useLogger(prefix: string) {
    useEffect(() => {
        console.log(prefix, 'mounted');
        return () => console.log(prefix, 'unmounted');
    }, [prefix]);
}

// Non-hook function - should NOT be detected as component (no JSX return)
// and should NOT affect hook detection
export function DataProcessor(data: unknown[]) {
    return data.map(item => JSON.stringify(item));
}
