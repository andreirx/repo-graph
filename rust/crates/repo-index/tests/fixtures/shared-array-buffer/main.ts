// BI-1C fixture: Main thread with SharedArrayBuffer
// Expected: 3 shared_array_buffer surfaces (SharedArrayBuffer, Atomics.store, Atomics.notify)
// Note: Worker and postMessage are NOT detected — they require dataflow to prove SAB transfer

const BUFFER_SIZE = 1024;

function setupWorker(): Worker {
    // Create shared memory
    const sab = new SharedArrayBuffer(BUFFER_SIZE);
    const view = new Int32Array(sab);

    // Initial value
    Atomics.store(view, 0, 0);

    // Spawn worker
    const worker = new Worker(new URL("./worker.ts", import.meta.url));

    // Hand off shared buffer
    worker.postMessage({ buffer: sab, command: "start" });

    return worker;
}

export function notifyWorker(worker: Worker, sab: SharedArrayBuffer): void {
    const view = new Int32Array(sab);
    Atomics.store(view, 0, 1);
    Atomics.notify(view, 0, 1);
}

// Main entry
const worker = setupWorker();
