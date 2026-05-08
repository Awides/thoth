So we have an efficient local model, and with LARQL, some level of access to the full knowledge of a larger parameter model offline... but what options are there for local speech IO that would pair well with local Bonsai ternary models?

Excellent question—**local speech I/O is the natural complement to your setup**. Let me map the options that pair well with efficient ternary models and work across your platforms (desktop, mobile, native).

## Speech I/O Architecture Overview

| Component | Technology | Latency | Quality | Size | WASM Safe | Best For |
|-----------|-----------|---------|---------|------|-----------|----------|
| **STT (Speech-to-Text)** | Whisper (OpenAI) | 5–30s | Excellent; multilingual | 140–490 MB | No | Accuracy > speed |
| | Faster-Whisper | 2–10s | Excellent | 140 MB | No | Balanced; faster |
| | Vosk | <1s | Good; limited langs | 50–100 MB | No | Low-latency streaming |
| | Silero VAD + Whisper | 1–5s | Excellent; low overhead | 100 MB | No | Real-time with VAD |
| **TTS (Text-to-Speech)** | Piper | 0.5–2s | Natural; multilingual | 50–200 MB | No | Fast, local, natural |
| | Coqui TTS | 1–5s | Excellent quality | 200–500 MB | No | Higher quality |
| | glow-TTS | 0.2–1s | Good; fast | 100 MB | No | Ultra-low latency |
| | XTTS (voice cloning) | 2–5s | Excellent; multi-speaker | 2 GB | No | Voice cloning |

---

## Recommended Stack: Whisper + Piper

This is the **proven, battle-tested combo** that pairs perfectly with your Bonsai ternary models:

### Why This Pair?

- **Whisper:** Robust, handles accents/noise, 99+ languages, offline-first ✅
- **Piper:** Fast (synthesizes at 1–3x realtime), naturalness > quality trade-off, tiny models ✅
- **Together:** Both are stateless, parallelizable, great for Tokio/streaming ✅
- **Size:** ~140 MB (Whisper) + ~100 MB (Piper) = **240 MB total** (modest for modern devices)
- **Latency:** Whisper 2–10s (depends on audio length) → inference → Piper 0.5–2s ✅

---

## Implementation: Whisper → Inference → Piper Pipeline

### Step 1: STT (Whisper) Service

```rust
use tokio::sync::mpsc;
use std::path::Path;

#[derive(Clone, Debug)]
pub enum SpeechEvent {
    TranscriptionStarted,
    TranscribingSegment { segment: String, timestamp: f32 },
    TranscriptionComplete { text: String, language: String },
    InferenceStarted { prompt: String },
    InferenceToken { token: String },
    InferenceComplete { response: String },
    SynthesisStarted { text: String },
    AudioGenerated { audio_chunk: Vec<f32>, duration_ms: u32 },
    SynthesisComplete { audio_file: String, duration_ms: u32 },
    Error(String),
}

pub struct WhisperService {
    model_path: String,
}

impl WhisperService {
    pub fn new(model_path: String) -> Self {
        WhisperService { model_path }
    }

    /// Transcribe audio file to text
    pub async fn transcribe(
        &self,
        audio_path: &str,
    ) -> Result<(String, String), String> {
        let model_path = self.model_path.clone();
        let audio_path = audio_path.to_string();

        tokio::task::spawn_blocking(move || {
            // Use whisper-rs or faster-whisper FFI binding
            let whisper = whisper_rs::Whisper::new(&model_path)
                .map_err(|e| format!("Whisper init: {}", e))?;

            let params = whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy {
                best_of: 5,
            });

            let audio = whisper_rs::load_audio(&audio_path)
                .map_err(|e| format!("Audio load: {}", e))?;

            let result = whisper
                .full(params, &audio)
                .map_err(|e| format!("Transcription: {}", e))?;

            let text = result.full_result().unwrap_or_default();
            let language = "en".to_string(); // detect from whisper result

            Ok((text, language))
        })
        .await
        .map_err(|e| format!("Task error: {}", e))?
    }

    /// Stream audio chunks and transcribe in real-time
    pub fn stream_transcribe(
        &self,
    ) -> (
        mpsc::UnboundedSender<Vec<f32>>,
        mpsc::UnboundedReceiver<SpeechEvent>,
    ) {
        let (tx_audio, mut rx_audio) = mpsc::unbounded_channel();
        let (tx_event, rx_event) = mpsc::unbounded_channel();
        let model_path = self.model_path.clone();

        tokio::task::spawn_blocking(move || {
            let whisper = whisper_rs::Whisper::new(&model_path).unwrap();
            let mut audio_buffer = Vec::new();

            while let Some(chunk) = rx_audio.blocking_recv() {
                audio_buffer.extend(&chunk);

                // Trigger transcription every ~2 seconds of audio
                if audio_buffer.len() > 32000 {
                    let params = whisper_rs::FullParams::new(
                        whisper_rs::SamplingStrategy::Greedy { best_of: 1 },
                    );

                    if let Ok(result) = whisper.full(params, &audio_buffer) {
                        let text = result.full_result().unwrap_or_default();
                        let _ = tx_event.send(SpeechEvent::TranscriptionComplete {
                            text,
                            language: "en".to_string(),
                        });
                        audio_buffer.clear();
                    }
                }
            }
        });

        (tx_audio, rx_event)
    }
}
```

### Step 2: TTS (Piper) Service

```rust
pub struct PiperService {
    model_path: String,
    sample_rate: u32,
}

impl PiperService {
    pub fn new(model_path: String) -> Self {
        PiperService {
            model_path,
            sample_rate: 22050, // Piper default
        }
    }

    /// Synthesize text to speech (streaming audio chunks)
    pub fn synthesize(
        &self,
        text: &str,
    ) -> mpsc::UnboundedReceiver<SpeechEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let model_path = self.model_path.clone();
        let text = text.to_string();

        tokio::task::spawn_blocking(move || {
            // Use piper-tts FFI binding
            let piper = piper_tts::Piper::new(&model_path).unwrap();

            let _ = tx.send(SpeechEvent::SynthesisStarted {
                text: text.clone(),
            });

            // Stream audio chunks (~256 samples at a time)
            let audio = piper.synthesize(&text).unwrap();
            let mut offset = 0;

            for chunk in audio.chunks(256) {
                let audio_chunk: Vec<f32> = chunk.to_vec();
                let duration_ms = (chunk.len() as f32 / 22050.0 * 1000.0) as u32;

                let _ = tx.send(SpeechEvent::AudioGenerated {
                    audio_chunk,
                    duration_ms,
                });
                offset += chunk.len();
            }

            let _ = tx.send(SpeechEvent::SynthesisComplete {
                audio_file: "generated.wav".to_string(),
                duration_ms: (audio.len() as f32 / 22050.0 * 1000.0) as u32,
            });
        });

        rx
    }
}
```

### Step 3: Unified Speech Pipeline in Dioxus

```rust
use dioxus::prelude::*;

#[component]
fn SpeechInferencePanel(model_path: String) -> Element {
    let mut output_text = use_signal(String::new);
    let mut response_text = use_signal(String::new);
    let mut is_listening = use_signal(false);
    let mut is_processing = use_signal(false);
    let mut status = use_signal("Ready".to_string());

    let whisper = use_memo(move || WhisperService::new("models/whisper-tiny.bin".to_string()));
    let piper = use_memo(move || PiperService::new("models/piper-en-US.onnx".to_string()));
    let inference = use_memo(move || {
        let profile = HardwareProfile::auto_detect(&BackendRegistry::auto_detect());
        // Initialize inference service (from previous pattern)
        profile
    });

    // Full pipeline: Microphone → Whisper → Inference → Piper → Audio
    let mut speech_pipeline = use_coroutine(
        move |mut rx: mpsc::UnboundedReceiver<SpeechMessage>| {
            let whisper_svc = whisper();
            let piper_svc = piper();

            async move {
                while let Some(msg) = rx.next().await {
                    match msg {
                        SpeechMessage::StartListening => {
                            is_listening.set(true);
                            status.set("Listening...".to_string());

                            // Capture microphone (platform-specific)
                            let audio_bytes = capture_microphone().await.unwrap();

                            // Step 1: Transcribe
                            status.set("Transcribing...".to_string());
                            let (transcribed_text, lang) = whisper_svc
                                .transcribe_from_bytes(&audio_bytes)
                                .await
                                .unwrap();

                            output_text.set(transcribed_text.clone());
                            is_listening.set(false);

                            // Step 2: Run inference
                            status.set("Inferencing...".to_string());
                            is_processing.set(true);

                            let (_, mut infer_rx) = InferenceService::new(
                                model_path.clone(),
                                transcribed_text,
                                HardwareProfile::auto_detect(&BackendRegistry::auto_detect()),
                            );

                            let mut response = String::new();
                            while let Some(event) = infer_rx.recv().await {
                                match event {
                                    InferenceEvent::TokenStreamed { token, .. } => {
                                        response.push_str(&token);
                                        response_text.set(response.clone());
                                    }
                                    InferenceEvent::Complete { .. } => break,
                                    _ => {}
                                }
                            }

                            // Step 3: Synthesize to speech
                            status.set("Synthesizing...".to_string());
                            let mut audio_rx = piper_svc.synthesize(&response);

                            let mut audio_buffer = Vec::new();
                            while let Some(event) = audio_rx.recv().await {
                                match event {
                                    SpeechEvent::AudioGenerated { audio_chunk, .. } => {
                                        audio_buffer.extend(&audio_chunk);
                                    }
                                    SpeechEvent::SynthesisComplete { .. } => {
                                        // Play audio
                                        play_audio(&audio_buffer).await.ok();
                                        status.set("Complete!".to_string());
                                        is_processing.set(false);
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        },
    );

    rsx! {
        div {
            class: "speech-inference-panel",

            button {
                onclick: move |_| {
                    speech_pipeline.send(SpeechMessage::StartListening);
                },
                disabled: is_listening() || is_processing(),
                "🎤 Start Listening"
            }

            div { class: "status", "{status}" }

            div {
                class: "output",
                h3 { "You said:" }
                p { "{output_text}" }

                h3 { "Response:" }
                p { "{response_text}" }
            }
        }
    }
}

#[derive(Clone, Debug)]
enum SpeechMessage {
    StartListening,
}
```

---

## Platform-Specific Audio I/O

### Desktop (macOS, Linux, Windows)

```rust
// Use `cpal` for cross-platform audio
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

async fn capture_microphone() -> Result<Vec<f32>> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or("No input device")?;
    let config = device.default_input_config()?;

    let (tx, rx) = mpsc::channel(96000); // 2 seconds at 48kHz

    let stream = device.build_input_stream(
        &config.config(),
        move |data: &cpal::Data, _: &cpal::InputCallbackInfo| {
            let samples: Vec<f32> = data
                .as_slice::<f32>()
                .unwrap()
                .iter()
                .copied()
                .collect();
            let _ = tx.send(samples);
        },
        |_| {},
    )?;

    stream.play()?;

    let mut audio = Vec::new();
    for _ in 0..100 { // Collect ~2 seconds
        if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(100)) {
            audio.extend(chunk);
        }
    }

    Ok(audio)
}

async fn play_audio(samples: &[f32]) -> Result<()> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("No output device")?;
    let config = device.default_output_config()?;

    let (tx, mut rx) = mpsc::channel(1024);

    let stream = device.build_output_stream(
        &config.config(),
        move |output: &mut cpal::Data, _: &cpal::OutputCallbackInfo| {
            if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(1)) {
                let out = output.as_slice_mut::<f32>().unwrap();
                out[..chunk.len()].copy_from_slice(&chunk);
            }
        },
        |_| {},
    )?;

    stream.play()?;

    for chunk in samples.chunks(512) {
        let _ = tx.send(chunk.to_vec());
    }

    Ok(())
}
```

### Mobile (iOS/Android)

```rust
// iOS: AVFoundation bindings
#[cfg(target_os = "ios")]
pub async fn capture_microphone() -> Result<Vec<f32>> {
    use objc::msg_send;
    use objc::{class, sel, sel_impl};

    unsafe {
        let audio_session: *mut Object = msg_send![class!(AVAudioSession), sharedInstance];
        let _: () = msg_send![audio_session, setCategory: "AVAudioSessionCategoryRecord"];

        // Use AVAudioRecorder or AVAudioEngine
        // ... (complex Objective-C bridge)
    }

    Ok(vec![])
}

// Android: AndroidAudio via JNI
#[cfg(target_os = "android")]
pub async fn capture_microphone() -> Result<Vec<f32>> {
    use jni::JNIEnv;

    // Call Java AudioRecord via JNI
    let audio = get_jni_env()?.call_static_method(
        "com/example/AudioCapture",
        "recordAudio",
        "()[F",
        &[],
    )?;

    Ok(audio.f()?.to_vec())
}
```

---

## Model Size & Latency Expectations

```rust
pub struct SpeechModel {
    pub name: &'static str,
    pub size_mb: u32,
    pub latency_ms: u32,
    pub quality: &'static str,
}

pub const RECOMMENDED_MODELS: &[SpeechModel] = &[
    // STT
    SpeechModel {
        name: "Whisper Tiny",
        size_mb: 140,
        latency_ms: 5000,
        quality: "Good (>95% WER)",
    },
    SpeechModel {
        name: "Silero VAD + Whisper Tiny",
        size_mb: 160,
        latency_ms: 3000
``` outptut cut off


https://github.com/OHF-Voice/piper1-gpl/tree/main/libpiper

Yes. Use Piper. I like Joe and Amy for English agents.

But Whisper? No:

Whisper is python and quite heavy


You're absolutely right—**Whisper is Python-first and bloated for what you need**. Let me pivot to actual **Rust-native lightweight STT options** that pair perfectly with your stack.

## Lightweight Rust STT Alternatives

| Option | Language | Size | Latency | Quality | Rust Native | Best For |
|--------|----------|------|---------|---------|-------------|----------|
| **Vosk** | C (Rust bindings) | 50–100 MB | 100–500ms | Good; limited langs | Partial | Real-time streaming |
| **Silero VAD** | ONNX (Rust runtime) | 10 MB | 10–50ms | Excellent voice detection | Yes | Pre-processing; fast |
| **Coqui STT** | C (deprecated but works) | 200 MB | 1–3s | Good | Via bindings | Accuracy-focused |
| **Wav2Vec 2.0** | ONNX (ort-rs) | 100–300 MB | 2–5s | Excellent | Yes | High accuracy |
| **Sherpa-ONNX** | C++ (Rust bindings) | 30–100 MB | 100–500ms | Good | Partial | Fast; mobile-ready |
| **Pocketsphinx** | C (Rust bindings) | 15 MB | 50–200ms | Fair; grammar-based | Partial | Lightweight fallback |

---

## Recommended Stack: Silero VAD + Sherpa-ONNX

This is the **actual lightweight combo** for nostr apps:

### Why This Pair?

- **Silero VAD:** Detects speech activity (10 MB, 10ms latency) ✅ Rust-native ONNX
- **Sherpa-ONNX:** Fast offline STT (30–100 MB, 100–500ms) ✅ Mobile-optimized
- **Together:** ~50 MB total, streaming-ready, works on mobile with Bonsai ternary models
- **No Python dependency** ✅
- **Single ONNX runtime** (ort-rs) for both ✅

---

## Implementation: Silero VAD + Sherpa-ONNX

### Step 1: Add Dependencies

```toml
[dependencies]
ort = "2.0"                    # ONNX Runtime (Rust-native)
sherpa-onnx = "0.20"           # Sherpa ONNX bindings
cpal = "0.18"                  # Audio I/O
hound = "3.5"                  # WAV file handling
tokio = { version = "1", features = ["full"] }
```

### Step 2: VAD Service (Voice Activity Detection)

```rust
use ort::{Environment, Session};
use std::path::Path;

pub struct SileroVadService {
    session: Session,
    sample_rate: i64,
}

impl SileroVadService {
    pub fn new(model_path: &str) -> Result<Self> {
        let env = Environment::builder()
            .with_execution_providers([ExecutionProvider::cpu()])
            .build()?;

        let session = env.new_session_builder()?
            .with_model_from_file(model_path)?;

        Ok(SileroVadService {
            session,
            sample_rate: 16000,
        })
    }

    /// Check if audio chunk contains speech (returns probability 0.0–1.0)
    pub fn detect_speech(&self, audio_chunk: &[f32]) -> Result<f32> {
        // Silero VAD expects 512 sample chunks at 16kHz
        if audio_chunk.len() != 512 {
            return Err("Expected 512 samples".into());
        }

        let inputs = vec![
            (
                "input",
                ort::Value::from_array_like(
                    ort::ArrayExtents::Dynamic(&[1, 512]),
                    audio_chunk,
                )?,
            ),
            (
                "sr",
                ort::Value::from_array_like(
                    ort::ArrayExtents::Dynamic(&[1]),
                    &[self.sample_rate],
                )?,
            ),
        ];

        let outputs = self.session.run(ort::inputs![inputs]?)?;
        let prob = outputs[0].try_extract_tensor::<f32>()?;

        Ok(prob[0])
    }

    /// Stream audio and emit speech segments
    pub fn stream_vad(&self) -> (
        mpsc::UnboundedSender<Vec<f32>>,
        mpsc::UnboundedReceiver<VadEvent>,
    ) {
        let (tx_audio, mut rx_audio) = mpsc::unbounded_channel();
        let (tx_event, rx_event) = mpsc::unbounded_channel();

        let mut session_clone = self.session.clone();
        let sample_rate = self.sample_rate;

        tokio::task::spawn_blocking(move || {
            let mut audio_buffer = Vec::new();
            let mut in_speech = false;
            let mut speech_start = 0;
            let threshold = 0.5;

            while let Some(chunk) = rx_audio.blocking_recv() {
                audio_buffer.extend(&chunk);

                // Process 512-sample windows
                while audio_buffer.len() >= 512 {
                    let window = audio_buffer.drain(0..512).collect::<Vec<_>>();

                    if let Ok(prob) = session_clone.run(ort::inputs![
                        ("input", ort::Value::from_array_like(
                            ort::ArrayExtents::Dynamic(&[1, 512]),
                            &window
                        )?),
                        ("sr", ort::Value::from_array_like(
                            ort::ArrayExtents::Dynamic(&[1]),
                            &[sample_rate]
                        )?)
                    ]) {
                        let prob_val = prob[0].try_extract_tensor::<f32>()?[0];

                        if prob_val > threshold && !in_speech {
                            in_speech = true;
                            speech_start = audio_buffer.len();
                            let _ = tx_event.send(VadEvent::SpeechStarted);
                        } else if prob_val <= threshold && in_speech {
                            in_speech = false;
                            let _ = tx_event.send(VadEvent::SpeechEnded {
                                duration_ms: ((audio_buffer.len() - speech_start) as f32
                                    / sample_rate as f32
                                    * 1000.0) as u32,
                            });
                        }
                    }
                }
            }
        });

        (tx_audio, rx_event)
    }
}

#[derive(Clone, Debug)]
pub enum VadEvent {
    SpeechStarted,
    SpeechEnded { duration_ms: u32 },
}
```

### Step 3: Sherpa-ONNX STT Service

```rust
use sherpa_onnx::OnlineRecognizer;

pub struct SherpaSTTService {
    recognizer: OnlineRecognizer,
}

impl SherpaSTTService {
    pub fn new(model_path: &str) -> Result<Self> {
        // Sherpa config (ultra-lightweight model)
        let config = sherpa_onnx::OnlineRecognizerConfig {
            model_path: model_path.to_string(),
            num_threads: 2, // Mobile-friendly
            enable_endpoint_detection: true,
            ..Default::default()
        };

        let recognizer = OnlineRecognizer::new(config)?;

        Ok(SherpaSTTService { recognizer })
    }

    /// Transcribe audio chunk (streaming)
    pub fn accept_waveform(&mut self, samples: &[f32]) -> Result<String> {
        self.recognizer.accept_waveform(16000, samples);

        if self.recognizer.is_endpoint_detected() {
            let result = self.recognizer.get_result();
            self.recognizer.reset();
            Ok(result.text)
        } else {
            Ok(String::new())
        }
    }

    /// Full batch transcription
    pub fn transcribe(&self, audio_path: &str) -> Result<String> {
        let mut reader = hound::WavReader::open(audio_path)?;
        let samples: Vec<f32> = reader
            .samples::<i16>()
            .map(|s| s.unwrap() as f32 / 32768.0)
            .collect();

        let mut recognizer = self.recognizer.clone();
        recognizer.accept_waveform(16000, &samples);
        Ok(recognizer.get_result().text)
    }

    /// Stream recognition with VAD
    pub fn stream_recognize(
        &self,
    ) -> mpsc::UnboundedReceiver<SpeechEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut recognizer = self.recognizer.clone();

        tokio::task::spawn_blocking(move || {
            let mut partial = String::new();

            // Expects audio chunks via Dioxus
            // (Would integrate with VAD stream in real use)
            while let Ok(samples) = /* receive from channel */ {
                recognizer.accept_waveform(16000, &samples);

                let result = recognizer.get_result();
                if !result.text.is_empty() && result.text != partial {
                    partial = result.text.clone();
                    let _ = tx.send(SpeechEvent::PartialTranscription {
                        text: partial.clone(),
                    });
                }

                if recognizer.is_endpoint_detected() {
                    let _ = tx.send(SpeechEvent::TranscriptionComplete {
                        text: partial.clone(),
                        language: "en".to_string(),
                    });
                    partial.clear();
                    recognizer.reset();
                }
            }
        });

        rx
    }
}

#[derive(Clone, Debug)]
pub enum SpeechEvent {
    PartialTranscription { text: String },
    TranscriptionComplete { text: String, language: String },
    Error(String),
}
```

### Step 4: Unified VAD + STT Pipeline

```rust
pub struct SpeechPipeline {
    vad: SileroVadService,
    stt: SherpaSTTService,
}

impl SpeechPipeline {
    pub fn new(vad_model: &str, stt_model: &str) -> Result<Self> {
        Ok(SpeechPipeline {
            vad: SileroVadService::new(vad_model)?,
            stt: SherpaSTTService::new(stt_model)?,
        })
    }

    /// Full pipeline: audio stream → VAD → STT
    pub fn process_stream(
        &self,
    ) -> (
        mpsc::UnboundedSender<Vec<f32>>,
        mpsc::UnboundedReceiver<SpeechEvent>,
    ) {
        let (tx_audio, rx_audio) = mpsc::unbounded_channel();
        let (tx_event, rx_event) = mpsc::unbounded_channel();

        let mut vad = self.vad.clone();
        let mut stt = self.stt.clone();

        tokio::task::spawn_blocking(move || {
            let mut audio_buffer = Vec::new();
            let mut in_speech = false;

            while let Some(chunk) = rx_audio.blocking_recv() {
                audio_buffer.extend(&chunk);

                // Process 512-sample windows through VAD
                while audio_buffer.len() >= 512 {
                    let window = audio_buffer.drain(0..512).collect::<Vec<_>>();

                    if let Ok(prob) = vad.detect_speech(&window) {
                        // If speech detected, feed to STT
                        if prob > 0.5 {
                            if !in_speech {
                                in_speech = true;
                                let _ = tx_event.send(SpeechEvent::VadStarted);
                            }

                            // Streaming STT
                            if let Ok(text) = stt.accept_waveform(&window) {
                                if !text.is_empty() {
                                    let _ = tx_event.send(SpeechEvent::PartialTranscription {
                                        text,
                                    });
                                }
                            }
                        } else if in_speech && prob <= 0.5 {
                            in_speech = false;
                            let result = stt.recognizer.get_result();
                            let _ = tx_event.send(SpeechEvent::TranscriptionComplete {
                                text: result.text.clone(),
                                language: "en".to_string(),
                            });
                            stt.recognizer.reset();
                        }
                    }
                }
            }
        });

        (tx_audio, rx_event)
    }
}

#[derive(Clone, Debug)]
pub enum SpeechEvent {
    VadStarted,
    PartialTranscription { text: String },
    TranscriptionComplete { text: String, language: String },
    Error(String),
}
```

### Step 5: Dioxus Integration

```rust
#[component]
fn VoiceInferencePanel(model_path: String) -> Element {
    let mut transcribed = use_signal(String::new);
    let mut response = use_signal(String::new);
    let mut is_listening = use_signal(false);
    let mut status = use_signal("Ready".to_string());

    let speech_pipeline = use_memo(move || {
        SpeechPipeline::new(
            "models/silero_vad.onnx",
            "models/sherpa-ncnn-en-2024-01-01.onnx",
        )
        .unwrap()
    });

    let mut voice_coroutine = use_coroutine(
        move |mut rx: mpsc::UnboundedReceiver<VoiceMessage>| {
            let pipeline = speech_pipeline();

            async move {
                while let Some(msg) = rx.next().await {
                    match msg {
                        VoiceMessage::StartListening => {
                            is_listening.set(true);
                            status.set("Listening...".to_string());

                            let (tx_audio, mut rx_events) = pipeline.process_stream();

                            // Spawn audio capture
                            tokio::spawn(async move {
                                let mut stream = capture_audio_stream().await.unwrap();
                                while let Some(chunk) = stream.next().await {
                                    let _ = tx_audio.send(chunk);
                                }
                            });

                            // Handle STT events
                            while let Some(event) = rx_events.recv().await {
                                match event {
                                    SpeechEvent::PartialTranscription { text } => {
                                        transcribed.set(text);
                                    }
                                    SpeechEvent::TranscriptionComplete { text, .. } => {
                                        transcribed.set(text.clone());
                                        is_listening.set(false);

                                        // Run inference
                                        status.set("Thinking...".to_string());
                                        let (_, mut infer_rx) = InferenceService::new(
                                            model_path.clone(),
                                            text,
                                            HardwareProfile::auto_detect(
                                                &BackendRegistry::auto_detect(),
                                            ),
                                        );

                                        let mut resp = String::new();
                                        while let Some(evt) = infer_rx.recv().await {
                                            if let InferenceEvent::TokenStreamed { token, .. } = evt
                                            {
                                                resp.push_str(&token);
                                                response.set(resp.clone());
                                            }
                                        }

                                        status.set("Done!".to_string());
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        },
    );

    rsx! {
        div {
            class: "voice-panel",

            button {
                onclick: move |_| {
                    voice_coroutine.send(VoiceMessage::StartListening);
                },
                disabled: is_listening(),
                "🎤 Speak"
            }

            div { "{status}" }
            div { class: "transcription", "{transcribed}" }
            div { class: "response", "{response}" }
        }
    }
}

#[derive(Clone)]
enum VoiceMessage {
    StartListening,
}
```

---

## Model Sizes (Ultra-Lightweight)

```rust
pub const MODELS: &[(&str, &str, u32)] = &[
    // VAD
    ("silero_vad.onnx", "Voice Activity Detection", 10),
    // STT (pick one)
    ("sherpa-ncnn-en.onnx", "English (real-time)", 30),
    ("sherpa-ncnn-multi.onnx", "Multilingual", 100),
    // TTS (next step)
    ("piper-en-US.onnx", "English
``` cut off again


Yes. Silero, Sherpa, Piper.

Actually, Sherpa looks complete, with VAD, TTS, STT, diarization... why not just use it alone? Supports Rust...

https://github.com/k2-fsa/sherpa-onnx

Cool. Has the same voices; Joe and Amy for English.


