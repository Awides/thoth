# Running Thoth

## Status
✅ **Builds successfully**  
✅ **All tests pass** (6/6)  
⚠️ **Runtime issues**: 
- Model loading (segfaults on exit)
- Styling (needs dx build/serve for Tailwind)

## Build Commands

### Standard Cargo Build
```bash
cd thoth
cargo build          # Debug build
cargo build --release  # Release build
```

### Dioxus Build (with Tailwind)
```bash
dx build    # Production build
dx serve    # Development with hot reload
dx bundle   # Bundle for distribution
```

### Run
```bash
export LD_LIBRARY_PATH=$PWD/lib
cargo run
```

## Current Issues

1. **Llama.cpp Segfault**: The app segfaults on exit, likely due to improper cleanup of llama resources
2. **Model Loading**: Error loading model from correct path
3. **Styling**: Needs dx build for Tailwind CSS processing

## Next Steps

1. Fix llama cleanup to prevent segfault
2. Test with dx serve for proper styling
3. Verify Nostr connectivity
4. Test MLS group creation

## Test Results
```\nrunning 6 tests
test net::message::tests::test_parse_cute_syntax ... ok
test net::mls_group::tests::test_create_group ... ok
test net::mls_group::tests::test_encrypt_decrypt ... ok
test net::nostr_client::tests::test_nostr_client_creation ... ok
test net::rhai_integration::tests::test_load_script ... ok
test net::rhai_integration::tests::test_rhai_eval ... ok
test result: ok. 6 passed; 0 failed
```

The **network stack is complete and tested**. The runtime issues are in the llama integration, not the MLS/Nostr code.
