//! Integration test for the llama inference engine.

#[cfg(test)]
mod integration_test {
    use thoth::llama_native::{self, Config};

    #[test]
    fn test_llama_inference_fails_without_model() {
        // Test that inference fails gracefully when model path is missing.
        // This test runs without requiring an actual model file.
        let result = llama_native::load_model("nonexistent/model.gguf", llama_native::Config::default());
        assert!(result.is_err());
    }

    #[test]
    #[ignore] // Requires a real model file; enable with -- --ignored
    fn test_llama_inference_with_model() {
        // Only run when a valid model path is available via env var.
        let model_path = std::env::var("THOTH_TEST_MODEL").unwrap_or_else(|_| "models/Bonsai-1.7B-Q1_0.gguf".to_string());
        let config = Config::default();
        let mut engine = llama_native::Engine::new(&model_path, &config).unwrap();
        let mut stream = engine.infer_stream("Hello", 32);
        let mut output = String::new();
        while let Some(chunk) = stream.next() {
            if let Ok(text) = chunk {
                output.push_str(&text);
            }
        }
        assert!(!output.is_empty());
    }
}

        let config = llama::Config::default();
        llama::load_model(&handle, model_path.to_string(), config)
            .await
            .expect("Failed to load model");

        // Run inference with a simple prompt
        let output = llama::infer(&handle, "Hello, world!".to_string())
            .await
            .expect("Inference failed");

        assert!(!output.is_empty(), "Output should not be empty");
        println!("Inference succeeded. Output: {}", output);
    }
}