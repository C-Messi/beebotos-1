//! 流式消息组件
//!
//! 显示正在生成的内容，附带闪烁光标指示器。
//! 内容通过 ContentRenderer 实时渲染，支持 Markdown / JSON / 纯文本。

use leptos::prelude::*;

use crate::components::webchat::ContentRenderer;

/// 流式消息组件：显示打字机缓冲内容 + 闪烁光标
#[component]
pub fn StreamingMessage(
    /// 当前已接收的流式内容
    content: String,
) -> impl IntoView {
    view! {
        <div class="message assistant streaming">
            <div class="message-avatar">"🤖"</div>
            <div class="message-content-wrapper">
                <ContentRenderer content=content attachments=vec![] />
            </div>
        </div>
    }
}
