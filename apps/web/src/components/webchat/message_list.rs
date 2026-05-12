//! 消息列表组件
//!
//! 渲染聊天消息列表，并在流式接收时显示 StreamingMessage 占位符。

use leptos::prelude::*;

use crate::components::webchat::{MessageItem, StreamingMessage};
use crate::webchat::ChatMessage;

/// 消息列表组件
#[component]
pub fn MessageList(
    messages: Signal<Vec<ChatMessage>>,
    #[prop(default = Signal::derive(|| String::new()))]
    streaming_content: Signal<String>,
    #[prop(default = Signal::derive(|| false))]
    is_streaming: Signal<bool>,
) -> impl IntoView {
    view! {
        <div class="message-list">
            <For
                each=move || messages.get()
                key=|msg| msg.id.clone()
                children=move |message| {
                    view! {
                        <MessageItem message=message />
                    }
                }
            />

            {move || {
                if is_streaming.get() {
                    view! {
                        <StreamingMessage content=streaming_content.get() />
                    }.into_any()
                } else {
                    view! { <div /> }.into_any()
                }
            }}
        </div>
    }
}
