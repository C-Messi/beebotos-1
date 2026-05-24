//! 消息列表组件
//!
//! 渲染聊天消息列表，并在流式接收时显示 StreamingMessage 占位符。

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::components::webchat::{MessageItem, StreamingMessage, WaitingMessage};
use crate::webchat::{ChatMessage, ToolCallEvent};

fn scroll_to_bottom(list_ref: NodeRef<leptos::html::Div>) {
    if let Some(element) = list_ref.get_untracked() {
        if let Ok(html_element) = element.dyn_into::<web_sys::HtmlElement>() {
            html_element.set_scroll_top(html_element.scroll_height());
        }
    }
}

fn schedule_scroll_to_bottom(list_ref: NodeRef<leptos::html::Div>) {
    scroll_to_bottom(list_ref);

    let next_tick_ref = list_ref;
    gloo_timers::callback::Timeout::new(0, move || {
        scroll_to_bottom(next_tick_ref);
    })
    .forget();

    let settled_ref = list_ref;
    gloo_timers::callback::Timeout::new(80, move || {
        scroll_to_bottom(settled_ref);
    })
    .forget();
}

pub fn should_show_waiting_indicator(is_sending: bool, is_streaming: bool) -> bool {
    is_sending && !is_streaming
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_indicator_shows_before_streaming_starts() {
        assert!(should_show_waiting_indicator(true, false));
        assert!(!should_show_waiting_indicator(true, true));
        assert!(!should_show_waiting_indicator(false, false));
    }
}

/// 消息列表组件
#[component]
pub fn MessageList(
    messages: Signal<Vec<ChatMessage>>,
    #[prop(default = Signal::derive(|| String::new()))] streaming_content: Signal<String>,
    #[prop(default = Signal::derive(|| false))] is_streaming: Signal<bool>,
    #[prop(default = Signal::derive(|| false))] is_waiting: Signal<bool>,
    #[prop(default = Signal::derive(Vec::new))] streaming_tool_calls: Signal<Vec<ToolCallEvent>>,
) -> impl IntoView {
    let list_ref = NodeRef::<leptos::html::Div>::new();

    Effect::new(move |_| {
        let _message_count = messages.get().len();
        let _streaming_content_len = streaming_content.get().len();
        let _streaming = is_streaming.get();
        let _waiting = is_waiting.get();
        let _tool_call_count = streaming_tool_calls.get().len();

        schedule_scroll_to_bottom(list_ref);
    });

    view! {
        <div class="message-list" node_ref=list_ref>
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
                        <StreamingMessage
                            content=streaming_content.get()
                            tool_calls=streaming_tool_calls.get()
                        />
                    }.into_any()
                } else if is_waiting.get() {
                    view! { <WaitingMessage /> }.into_any()
                } else {
                    view! { <div /> }.into_any()
                }
            }}
        </div>
    }
}
