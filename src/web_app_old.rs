 use dioxus::prelude::*;
 
 static TAILWIND: Asset = asset!("/assets/tailwind.css");

#[derive(Clone, PartialEq, Copy)]
enum MessageRole { User, Assistant, System }

#[derive(Clone, PartialEq)]
struct Message {
    id: u64,
    role: MessageRole,
    content: String,
    thinking: String,
}

enum LoadingState { Loading, Ready, Error(&'static str) }

pub fn App() -> Element {
    let mut messages = use_signal(|| Vec::<Message>::new());
    let mut next_id = use_signal(|| 0u64);
    let mut is_loading = use_signal(|| false);
    let mut loading_state = use_signal(|| LoadingState::Loading);
     let mut input = use_signal(|| String::new());

     // Startup: set ready and show welcome message
     let startup = {
         let mut ls = loading_state.clone();
         let mut ms = messages.clone();
         let nid = next_id.clone();
         async move {
             // Simulate short initialization (e.g., connecting to network)
             tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
             ls.set(LoadingState::Ready);
             // Welcome system message
             let id = nid();
             nid.set(id + 1);
             ms.with_mut(|v| {
                 v.push(Message {
                     id,
                     role: MessageRole::System,
                     content: "🌐 Web interface active. Connect your device via Nostr/MLS to use the model remotely.".to_string(),
                     thinking: String::new(),
                 });
             });
         }
     };
     let _startup_fut = use_future(move || startup);

    let on_submit = move |e: FormEvent| {
        e.prevent_default();
        let val = input.read().trim().to_string();
        if val.is_empty() || !matches!(*loading_state.read(), LoadingState::Ready) {
            return;
        }

        let id = next_id();
        next_id.set(id + 1);
        messages.with_mut(|v| {
            v.push(Message { id, role: MessageRole::User, content: val.clone(), thinking: String::new() })
        });

        let aid = next_id();
        next_id.set(aid + 1);
        messages.with_mut(|v| {
            v.push(Message { id: aid, role: MessageRole::Assistant, content: String::new(), thinking: String::new() })
        });

        is_loading.set(true);
        let mut msgs = messages.clone();
        let mut il = is_loading.clone();
        let handle2 = handle;
        let aid2 = aid;
        let prompt = val;

         dioxus::prelude::spawn(async move {
             // Simulate network request to remote device
             tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
             msgs.with_mut(|v| {
                 if let Some(msg) = v.iter_mut().find(|m| m.id == aid2) {
                     msg.content = "Remote inference not yet implemented. In the future, your prompt will be sent via Nostr to a device running the model.".to_string();
                 }
             });
             il.set(false);
         });

        input.set(String::new());
    };

    let current_loading = loading_state.clone();
     let input_val = input.read().clone();
    let msgs = messages();

    rsx! {
        document::Stylesheet { href: TAILWIND },
        div { class: "h-screen flex flex-col bg-[#0d0d0d] text-[#ededed]",
            div {
                class: "flex-1 overflow-y-auto p-4 space-y-3",
                 for msg in msgs.iter() {
                     div {
                         key: msg.id,
                         class: "p-3 rounded-lg max-w-[80%]",
                         style: format!("background: {}",
                             match msg.role {
                                 MessageRole::User => "#3b82f6",
                                 MessageRole::Assistant => "#1a1a1a",
                                 MessageRole::System => "#5c2d2d",
                             }
                         ),
                         pre { class: "m-0 whitespace-pre-wrap font-inherit", msg.content.clone() }
                  }
              },
            }
            div { class: "p-3 border-t border-[#262626] bg-[#1a1a1a]",
                p { class: "text-xs text-gray-500 mb-2",
                    match &*current_loading.read() {
                        LoadingState::Loading => "Loading model...",
                        LoadingState::Ready => "Ready",
                        LoadingState::Error(_) => "Error",
                    }
                 },
                 form {
                    onsubmit: on_submit,
                     input {
                         r#type: "text",
                         autofocus: true,
                         placeholder: "Prompt...",
                         value: input_val,
                         oninput: move |e| *input.write() = e.data.value(),
                         class: "w-full px-3 py-2 border rounded bg-[#0d0d0d] text-[#ededed] border-[#262626]",
                     }
                }
            }
        }
    }
}