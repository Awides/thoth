//! Integration test for the llama inference engine.

#[cfg(test)]
mod tests {
    use crate::llama;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_inference_basic() {
        // Spawn inference thread
        let handle = llama::spawn_inference_thread();

        // Load the default model
        let model_path = "/home/awides/dev/bn/thoth/models/Bonsai-1.7B-Q1_0.gguf";
        if !std::path::Path::new(model_path).exists() {
            eprintln!("Model not found at: {}", model_path);
            return;
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