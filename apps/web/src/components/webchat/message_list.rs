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
    #[prop(optional)] streaming_content: Option<String>,
    #[prop(optional)] is_streaming: Option<bool>,
) -> impl IntoView {
    let is_streaming = is_streaming.unwrap_or(false);

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

            {if is_streaming {
                view! {
                    <StreamingMessage content=streaming_content.unwrap_or_default() />
                }.into_any()
            } else {
                view! { <div /> }.into_any()
            }}
        </div>
    }
}
