use crate::mem::*;

#[tokio::main]
async fn main() {
    println!("=== Memvid Test ===");
    let _ = std::fs::remove_file("test.mv2");
    let _ = std::fs::remove_file("test.mv2.idx");
    
    let handle = worker::spawn_worker();
    println!("Opening...");
    if let Ok(_) = handle.open("test.mv2".to_string()).await {
        println!("Opened!");
        let event = MessageEvent {
            shell_id: 1,
            message: Message {
                id: 1,
                msg_type: MessageType::Display,
                content: "Test!".to_string(),
                element_kind: None,
                value: None,
                sender_id: "test".into(),
                sender_name: "Test".into(),
                timestamp: String::new(),
            },
        };
        println!("Appending...");
        handle.append_message(event);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        if let Ok(m) = std::fs::metadata("test.mv2") {
            println!("Size: {} bytes", m.len());
        }
    }
}
