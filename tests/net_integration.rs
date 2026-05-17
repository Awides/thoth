#[cfg(test)]
mod tests {
    use thoth::net::{
        InferenceRequest, InferenceResponse,
        DeviceCaps, MlsGroupManager,
    };

    #[test]
    fn test_inference_request_serde_roundtrip() {
        let req = InferenceRequest {
            request_id: "test-req-001".into(),
            prompt_segments: vec!["<|im_start|>system\nYou are Tot.".into(), "<|im_start|>user\nHello<|im_end|>".into()],
            system_prompt: "You are Tot.".into(),
            model_hint: Some("Bonsai-1.7B".into()),
            max_tokens: Some(256),
            temperature: Some(0.5),
        };

        let json = serde_json::to_string(&req).unwrap();
        let de: InferenceRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(de.request_id, "test-req-001");
        assert_eq!(de.prompt_segments.len(), 2);
        assert_eq!(de.temperature, Some(0.5));
    }

    #[test]
    fn test_inference_response_serde_roundtrip() {
        let resp = InferenceResponse {
            request_id: "test-resp-001".into(),
            content: "Hello! I'm Tot.".into(),
            thinking: "User said hello...".into(),
            model_used: "Bonsai-1.7B-Q1_0".into(),
            tokens_generated: 8,
            elapsed_ms: 420,
        };

        let json = serde_json::to_string(&resp).unwrap();
        let de: InferenceResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(de.content, "Hello! I'm Tot.");
        assert_eq!(de.elapsed_ms, 420);
    }

    #[test]
    fn test_device_caps_scoring_order() {
        let desktop = DeviceCaps {
            device_name: "desktop".into(),
            pubkey: "pk1".into(),
            gpu_layers: 99,
            ram_mb: 32768,
            cpu_cores: 16,
            model_loaded: Some("model".into()),
            is_desktop: true,
            supports_inference: true,
            timestamp: 0,
        };
        let phone = DeviceCaps {
            device_name: "phone".into(),
            pubkey: "pk2".into(),
            gpu_layers: 0,
            ram_mb: 8192,
            cpu_cores: 4,
            model_loaded: None,
            is_desktop: false,
            supports_inference: true,
            timestamp: 0,
        };
        let weak = DeviceCaps {
            device_name: "weak".into(),
            pubkey: "pk3".into(),
            gpu_layers: 0,
            ram_mb: 2048,
            cpu_cores: 2,
            model_loaded: None,
            is_desktop: false,
            supports_inference: false,
            timestamp: 0,
        };
        assert!(desktop.score() > phone.score());
        assert!(phone.score() > weak.score());
    }

    #[tokio::test]
    #[ignore]
    async fn test_mls_group_between_two_instances() {
        let mut alice = MlsGroupManager::new();
        let mut bob = MlsGroupManager::new();

        alice.create_group("test-group".to_string(), "alice".to_string()).unwrap();

        let bob_kp = bob.generate_key_package("bob".to_string()).unwrap();

    let (commit_bytes, welcome_bytes) = alice.add_member("test-group", &bob_kp, "bob".to_string()).unwrap();

    let bob_group_id = bob.process_welcome(&welcome_bytes as &[u8], "bob".to_string()).unwrap();

        let msg = b"Hello from Alice via MLS!";
        let ciphertext = alice.encrypt("test-group", msg).unwrap();
        let decrypted = bob.decrypt(&bob_group_id, &ciphertext).unwrap();
        assert_eq!(decrypted, msg);

        let msg2 = b"Hello from Bob via MLS!";
        let ciphertext2 = bob.encrypt(&bob_group_id, msg2).unwrap();
        let decrypted2 = alice.decrypt("test-group", &ciphertext2).unwrap();
        assert_eq!(decrypted2, msg2);
    }
}
