//! WebChat 组件
//!
//! 提供聊天界面相关的 UI 组件，包括：
//! - 消息列表与消息项
//! - 内容渲染器（Markdown / JSON / 纯文本 / 附件）
//! - 流式消息占位
//! - WebSocket 连接 Hook
//! - 会话列表、输入框、侧边面板等

pub mod content_renderer;
pub mod json_tree;
pub mod markdown_view;
pub mod message_input;
pub mod message_item;
pub mod message_list;
pub mod session_item;
pub mod session_list;
pub mod side_panel;
pub mod streaming_message;
pub mod usage_panel;
pub mod websocket_hook;

pub use content_renderer::ContentRenderer;
pub use json_tree::JsonTreeView;
pub use markdown_view::MarkdownView;
pub use message_input::MessageInput;
pub use message_item::MessageItem;
pub use message_list::MessageList;
pub use session_item::SessionItem;
pub use session_list::SessionList;
pub use side_panel::SidePanel;
pub use streaming_message::{StreamingMessage, ToolCallList, WaitingMessage};
pub use usage_panel::UsagePanelComponent;
pub use websocket_hook::use_websocket_chat;
