// Minimal test worker - no model loading
self.onmessage = (e) => {
    const { type } = e.data;
    
    if (type === 'test') {
        self.postMessage({ type: 'test_result', message: 'Worker is alive!' });
    }
    
    if (type === 'wasm_test') {
        // Test if Module is available
        if (typeof Module !== 'undefined' && Module) {
            self.postMessage({ type: 'wasm_ok', has_module: true });
        } else {
            self.postMessage({ type: 'wasm_ok', has_module: false });
        }
    }
};

console.log('[Test Worker] Initialized');
self.postMessage({ type: 'init' });
