// Web Worker for llama.cpp WASM inference with progress
console.log('[Worker] Worker script loaded, version with progress reporting');
self.postMessage({ type: 'worker_loaded' });

// Global error handler
self.onerror = (message, source, lineno, colno, error) => {
    console.error('[Worker] Global error:', message, source, lineno, colno, error);
    self.postMessage({ type: 'error', message: `${message} at ${source}:${lineno}:${colno}` });
};

let Module = null;
let modelPtr = 0;
let ctxPtr = 0;
let samplerPtr = 0;
let vocabPtr = 0;
let modelLoaded = false;

const initWasm = () => {
    return new Promise((resolve, reject) => {
        console.log('[Worker] initWasm: Starting...');
        
        const timeoutId = setTimeout(() => {
            console.error('[Worker] WASM initialization timeout after 30s');
            reject(new Error('WASM initialization timeout'));
        }, 30000);
        
        const Module = {
            onRuntimeInitialized: () => {
                console.log('[Worker] onRuntimeInitialized callback fired');
                clearTimeout(timeoutId);
                console.log('[Worker] WASM runtime initialized, calling backend init');
                try {
                    Module._llama_backend_init();
                    console.log('[Worker] llama.cpp backend initialized');
                    resolve();
                } catch (e) {
                    console.error('[Worker] Backend init error:', e);
                    reject(e);
                }
            },
            print: (text) => console.log('[llama]', text),
            printErr: (text) => console.error('[llama]', text),
            locateFile: (path) => {
                console.log('[Worker] locateFile:', path);
                return path;
            },
        };
        
        globalThis.Module = Module;
        self.Module = Module;
        
        console.log('[Worker] About to import llama.js');
        try {
            importScripts('./llama.js');
            console.log('[Worker] importScripts completed, waiting for WASM...');
        } catch (e) {
            clearTimeout(timeoutId);
            console.error('[Worker] importScripts error:', e);
            reject(e);
        }
    });
};

// Fetch with progress
const fetchWithProgress = async (url, onProgress) => {
    const response = await fetch(url);
    const total = parseInt(response.headers.get('content-length')) || 0;
    let loaded = 0;
    
    const reader = response.body.getReader();
    const chunks = [];
    
    while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        
        chunks.push(value);
        loaded += value.length;
        
        if (total > 0 && onProgress) {
            const percent = ((loaded / total) * 100).toFixed(1);
            const mb = (loaded / 1024 / 1024).toFixed(2);
            const totalMB = (total / 1024 / 1024).toFixed(2);
            onProgress(mb, totalMB, percent);
        }
    }
    
    const arrayBuffer = new Uint8Array(loaded);
    let offset = 0;
    for (const chunk of chunks) {
        arrayBuffer.set(chunk, offset);
        offset += chunk.length;
    }
    
    return arrayBuffer;
};

const loadModel = async (modelPath) => {
    if (!Module) throw new Error('WASM not initialized');
    
    console.log('[Worker] Step 1: Fetching model:', modelPath);
    const startTime = Date.now();
    
    try {
        const modelData = await fetchWithProgress(modelPath, (mb, totalMB, percent) => {
            self.postMessage({ 
                type: 'progress', 
                message: `Downloading: ${mb}/${totalMB} MB (${percent}%)` 
            });
        });
        
        const fetchTime = ((Date.now() - startTime) / 1000).toFixed(2);
        console.log(`[Worker] Step 2: Model fetched in ${fetchTime}s, size: ${(modelData.length / 1024 / 1024).toFixed(2)} MB`);
        
        self.postMessage({ 
            type: 'progress', 
            message: `Writing ${ (modelData.length / 1024 / 1024).toFixed(1)} MB to memory...` 
        });
        
        console.log('[Worker] Step 3: Writing to virtual FS...');
        Module.FS.writeFile('/model.gguf', modelData);
        console.log('[Worker] Step 4: Model written to FS');
        
        console.log('[Worker] Step 5: Loading model...');
        const params = Module._llama_model_default_params();
        Module.setValue(params + 4, 0, 'i32');
        Module.setValue(params + 8, 0, 'i32');
        
        modelPtr = Module._llama_model_load_from_file('/model.gguf', params);
        if (modelPtr === 0) throw new Error('Failed to load model');
        console.log('[Worker] Step 6: Model loaded at pointer:', modelPtr);
        
        vocabPtr = Module._llama_model_get_vocab(modelPtr);
        
        const ctxParams = Module._llama_context_default_params();
        Module.setValue(ctxParams, 256, 'i32');  // Smaller context for web
        Module.setValue(ctxParams + 4, 0, 'i32');
        
        ctxPtr = Module._llama_init_from_model(modelPtr, ctxParams);
        if (ctxPtr === 0) throw new Error('Failed to create context');
        console.log('[Worker] Step 7: Context created at:', ctxPtr);
        
        const samplerParams = Module._llama_sampler_chain_default_params();
        samplerPtr = Module._llama_sampler_chain_init(samplerParams);
        
        Module._llama_sampler_chain_add(samplerPtr, Module._llama_sampler_init_temp(0.8));
        Module._llama_sampler_chain_add(samplerPtr, Module._llama_sampler_init_top_k(40));
        Module._llama_sampler_chain_add(samplerPtr, Module._llama_sampler_init_top_p(0.9, 1));
        Module._llama_sampler_chain_add(samplerPtr, Module._llama_sampler_init_greedy());
        
        console.log('[Worker] Step 8: Samplers initialized');
        modelLoaded = true;
        const totalTime = ((Date.now() - startTime) / 1000).toFixed(2);
        console.log(`[Worker] Model loading complete in ${totalTime}s`);
        self.postMessage({ type: 'progress', message: 'Model loaded successfully!' });
    } catch (err) {
        console.error('[Worker] Load error:', err);
        throw err;
    }
};

const runInference = (prompt) => {
    if (!modelLoaded) throw new Error('Model not loaded');
    
    console.log('[Worker] Starting inference on:', prompt.substring(0, 50) + '...');
    
    if (ctxPtr !== 0) Module._llama_free(ctxPtr);
    const ctxParams = Module._llama_context_default_params();
    Module.setValue(ctxParams, 512, 'i32');
    Module.setValue(ctxParams + 4, 0, 'i32');
    ctxPtr = Module._llama_init_from_model(modelPtr, ctxParams);
    Module._llama_sampler_reset(samplerPtr);
    
    const promptPtr = Module._malloc(prompt.length + 1);
    Module.stringToUTF8(prompt, promptPtr, prompt.length + 1);
    
    const maxTokens = 2048;
    const tokensPtr = Module._malloc(maxTokens * 4);
    const nTokens = Module._llama_tokenize(vocabPtr, promptPtr, prompt.length, tokensPtr, maxTokens, true, true);
    Module._free(promptPtr);
    
    if (nTokens <= 0) throw new Error('Tokenization failed');
    console.log('[Worker] Prompt tokenized:', nTokens, 'tokens');
    
    const batch = Module._llama_batch_get_one(tokensPtr, nTokens);
    if (Module._llama_decode(ctxPtr, batch) !== 0) {
        throw new Error('Failed to decode prompt');
    }
    
    const eos = Module._llama_token_eos(vocabPtr);
    const maxNew = Math.min(256, 512 - nTokens);
    
    for (let i = 0; i < maxNew; i++) {
        const newToken = Module._llama_sampler_sample(samplerPtr, ctxPtr, -1);
        
        if (newToken === eos) {
            console.log('[Worker] EOS token reached');
            break;
        }
        
        const bufSize = 256;
        const buf = Module._malloc(bufSize);
        const tokenArray = Module._malloc(4);
        Module.setValue(tokenArray, newToken, 'i32');
        
        const n = Module._llama_detokenize(vocabPtr, tokenArray, 1, buf, bufSize, false, true);
        
        if (n > 0) {
            const tokenStr = Module.UTF8ToString(buf, n);
            if (tokenStr) {
                self.postMessage({ type: 'token', token: tokenStr });
            }
        }
        
        Module._free(buf);
        Module._free(tokenArray);
        
        Module._llama_sampler_accept(samplerPtr, newToken);
        const batch2 = Module._llama_batch_get_one(tokenArray, 1);
        if (Module._llama_decode(ctxPtr, batch2) !== 0) break;
    }
    
    console.log('[Worker] Inference complete');
};

self.onmessage = async (e) => {
    const { type, path, prompt } = e.data;
    
    console.log('[Worker] Received message:', type, path ? path : '');
    
    try {
        if (type === 'init') {
            console.log('[Worker] Initializing WASM...');
            await initWasm();
            self.postMessage({ type: 'ready' });
        }
        
        if (type === 'load') {
            console.log('[Worker] Load command received, path:', path);
            if (!Module) {
                console.log('[Worker] Module not initialized, initializing...');
                await initWasm();
            }
            console.log('[Worker] Starting model load...');
            await loadModel(path);
            console.log('[Worker] Model load complete');
            self.postMessage({ type: 'ready' });
        }
        
        if (type === 'infer') {
            if (!modelLoaded) {
                self.postMessage({ type: 'error', message: 'Model not loaded' });
                return;
            }
            runInference(prompt);
            self.postMessage({ type: 'done' });
        }
    } catch (err) {
        console.error('[Worker] Error:', err);
        self.postMessage({ type: 'error', message: err.toString() + '\n' + err.stack });
    }
};