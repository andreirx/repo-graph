// BI-1C fixture: Worker with SharedArrayBuffer consumption
// Expected: 3 shared_array_buffer surfaces (Atomics.wait, Atomics.load, Atomics.store)
// Note: onmessage handler is NOT detected — would require dataflow to prove SAB reception

declare const self: Worker;

interface WorkerMessage {
    buffer: SharedArrayBuffer;
    command: string;
}

self.onmessage = (event: MessageEvent<WorkerMessage>) => {
    const { buffer, command } = event.data;
    const view = new Int32Array(buffer);

    if (command === "start") {
        processData(view);
    }
};

function processData(view: Int32Array): void {
    // Wait for signal from main thread
    const result = Atomics.wait(view, 0, 0);

    if (result === "ok") {
        // Read value
        const value = Atomics.load(view, 0);
        console.log("Received value:", value);

        // Write back result
        Atomics.store(view, 1, value * 2);
    }
}
