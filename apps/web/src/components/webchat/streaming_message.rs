//! 流式消息组件
//!
//! 显示正在生成的内容，附带闪烁光标指示器。
//! 内容通过 ContentRenderer 实时渲染，支持 Markdown / JSON / 纯文本。

use leptos::prelude::*;

use crate::components::webchat::ContentRenderer;
use crate::webchat::ToolCallEvent;

/// 流式消息组件：显示打字机缓冲内容 + 闪烁光标
#[component]
pub fn StreamingMessage(
    /// 当前已接收的流式内容
    content: String,
    #[prop(default = Vec::new())] tool_calls: Vec<ToolCallEvent>,
) -> impl IntoView {
    view! {
        <div class="message assistant streaming">
            <div class="message-avatar">"🤖"</div>
            <div class="message-content-wrapper">
                <ToolCallList tool_calls=tool_calls />
                <ContentRenderer content=content attachments=vec![] />
            </div>
        </div>
    }
}

#[component]
pub fn WaitingMessage() -> impl IntoView {
    view! {
        <div class="message assistant waiting">
            <div class="message-avatar">"🤖"</div>
            <div class="message-content thinking-message">
                <span>"Thinking"</span>
                <span class="thinking-dots" aria-hidden="true">
                    <span></span>
                    <span></span>
                    <span></span>
                </span>
            </div>
        </div>
    }
}

#[component]
pub fn ToolCallList(tool_calls: Vec<ToolCallEvent>) -> impl IntoView {
    let calls = tool_calls.clone();
    if tool_calls.is_empty() {
        return view! { <div /> }.into_any();
    }

    view! {
            <div class="tool-call-list">
                <For
                    each=move || calls.clone()
                    key=|call| call.id.clone()
                    children=move |call| {
                        view! { <ToolCallBadge call=call /> }
                    }
                />
            </div>
    }
    .into_any()
}

#[component]
fn ToolCallBadge(call: ToolCallEvent) -> impl IntoView {
    let preview = call.argument_preview();
    let tool_name = call.tool_name.clone();
    let round = call.round;
    let reasoning = call.reasoning.clone();
    let reasoning_view = if reasoning.is_empty() {
        view! { <div /> }.into_any()
    } else {
        view! { <div class="tool-call-reason">{reasoning.clone()}</div> }.into_any()
    };
    let preview_view = if preview.is_empty() || preview == "null" {
        view! { <div /> }.into_any()
    } else {
        view! { <code class="tool-call-args">{preview.clone()}</code> }.into_any()
    };

    view! {
        <div class="tool-call-badge">
            <div class="tool-call-icon" title="工具调用">"⌁"</div>
            <div class="tool-call-body">
                <div class="tool-call-header">
                    <span class="tool-call-label">"Tool"</span>
                    <span class="tool-call-name">{tool_name}</span>
                    <span class="tool-call-round">{format!("#{}", round)}</span>
                </div>
                {reasoning_view}
                {preview_view}
            </div>
        </div>
    }
}
