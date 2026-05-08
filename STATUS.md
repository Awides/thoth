# Bonsai 1.7B + Burn Integration Status 🔥

## What We Did

### 1. Downloaded Bonsai 1.7B ONNX Model
```bash
models/bonsai-1.7b-onnx/
├── model.onnx              # Main model (~2.8GB)
├── tokenizer.json          # Tokenizer vocabulary
├── tokenizer_config.json   # Tokenizer config
├── config.json             # Model config (Qwen3 architecture)
└── chat_template.jinja     # Chat template
```

Model specs:
- **Architecture**: Qwen3ForCausalLM
- **Parameters**: 1.7B
- **Vocab size**: 151,669 tokens
- **Hidden size**: 2048
- **Layers**: 28
- **Attention heads**: 16 (Q), 8 (KV)
- **Context**: 32,768 tokens
- **Format**: ONNX (externally stored data)

### 2. Implemented Tokenizer Integration
- Added `tokenizers` crate (HuggingFace tokenizers)
- Loads tokenizer from `tokenizer.json`
- Proper error handling if tokenizer fails to load

### 3. Updated Burn Engine
- Now accepts model path in constructor
- Loads tokenizer on initialization
- Tokenizes input prompts
- Ready for ONNX model loading

## Current Status

**Working:**
- ✅ Pure Rust Burn backend
- ✅ Tokenizer loading (HuggingFace format)
- ✅ Model files downloaded
- ✅ Streaming token events
- ✅ Dioxus UI integration

**TODO - Next Steps:**

1. **Load ONNX Model in Burn**
   ```rust
   use burn_onnx::Model;
   
   let model = Model::<DefaultBackend>::load(
       "models/bonsai-1.7b-onnx/model.onnx",
       device,
   )?;
   ```

2. **Implement Inference Loop**
   - Forward pass through ONNX model
   - Sample from output logits (temperature, top-p, top-k)
   - Detokenize output tokens
   - Stream tokens as they're generated

3. **Handle Qwen3 Specifics**
   - RoPE embeddings (YaRN variant)
   - RMSNorm (not LayerNorm)
   - SwiGLU activation
   - Grouped Query Attention (GQA)

4. **Memory Management**
   - Model is ~4.5GB on disk
   - Will need ~8-10GB RAM at runtime
   - Consider quantization (INT8, FP16)

## Architecture

```
User Input
    ↓
Tokenizer (tokenizers crate)
    ↓
Input IDs [batch, seq_len]
    ↓
Burn ONNX Model
    ↓
Logits [batch, seq_len, vocab_size]
    ↓
Sampler (temperature, top-p, top-k)
    ↓
Next Token ID
    ↓
Detokenize → String
    ↓
Stream to UI
```

## Testing

```bash
# Run with ONNX model
cargo run

# The app should:
# 1. Load tokenizer from models/bonsai-1.7b-onnx/
# 2. Accept user input
# 3. Tokenize and run inference
# 4. Stream response back
```

## Performance Expectations

**Desktop (WebGPU backend):**
- Small models (<1B): 10-50 tokens/sec
- 1.7B model: 5-20 tokens/sec
- Depends on GPU

**WASM/WebGPU (browser):**
- 1.7B model: 1-5 tokens/sec
- Good for always-available local inference

**Wizard of Oz Pattern:**
- Use local Bonsai for:
  - Quick responses
  - UI interactions
  - Simple Q&A
  - Tool routing
  
- Escalate to remote models for:
  - Complex reasoning
  - Long context needs
  - Specialized knowledge

---

**Bottom line:** We have the model files and tokenizer ready. Next step is implementing the actual ONNX inference loop in Burn!
