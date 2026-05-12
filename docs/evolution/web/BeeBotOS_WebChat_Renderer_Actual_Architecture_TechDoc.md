# BeeBotOS WebChat 多模态内容渲染引擎技术文档（实际架构版）
## 项目代号：BeeBotOS-WebChat-Renderer-Actual v1.0.0

> **架构定位**：基于 Rust 全栈（Leptos CSR + Axum Gateway）的 Agent 对话系统实际实现。前端采用纯客户端渲染（CSR）+ WebSocket 实时推送，服务端以 SQLite 持久化、多模态内容处理、LLM 故障转移为核心。本文档严格对应 `beebotos/apps/web` 与 `beebotos/apps/gateway` 的实际代码状态，标注当前实现与设计愿景的差异。

---

## 1. 架构总览

### 1.1 技术栈选型（实际）

| 层级 | 技术组件 | 版本 | 职责 |
|------|---------|------|------|
| **前端框架** | Leptos (CSR 模式) | 0.8.6 | 响应式 UI、组件树、DOM 细粒度更新 |
| **前端构建工具** | Trunk | 0.21+ | WASM 打包、资源内联、开发服务器 |
| **前端路由** | leptos_router | 0.8.6 | 客户端路由（SPA） |
| **HTTP 客户端** | gloo-net | 0.5+ | WASM 端异步 HTTP 请求 |
| **WebSocket 客户端** | web-sys (原生 WebSocket) | 0.3+ | 直连 Gateway 的原始 WebSocket API |
| **服务端框架** | Axum | 0.7+ | HTTP 路由、WebSocket upgrade、REST API |
| **数据库** | sqlx (SQLite) | 0.8+ | 异步 SQLite，迁移管理 |
| **LLM 集成** | beebotos_agents::llm | 内部 crate | 多提供商统一接口（Kimi/OpenAI/DeepSeek 等） |
| **序列化** | serde / serde_json / serde_yaml | 1.0+ | 全栈类型安全传输 |
| **内存与状态** | Leptos RwSignal / LocalStorage | — | 前端响应式状态 + 会话本地持久化 |
| **代理服务器** | Axum + reqwest | 0.12+ | Web Server 对 Gateway 的 HTTP 反向代理 |

> **与 v0.1.0 设计文档的差异**：实际架构未采用 Leptos SSR/Islands 模式，也未使用 SSE（Server-Sent Events）作为流式传输协议，而是选择了 **CSR + 原生 WebSocket** 的组合。前端没有引入 `pulldown-cmark`、`comrak` 等 Markdown 解析器，消息内容以**纯文本字符串**直接渲染。

### 1.2 架构分层图（实际部署拓扑）

```
┌─────────────────────────────────────────────────────────────┐
│  Browser (WASM32) — Port 8090                               │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  Leptos CSR App (Trunk 构建)                        │   │
│  │  ┌──────────────┐  ┌──────────────┐                │   │
│  │  │ MessageList  │  │ MessageItem  │  ...           │   │
│  │  │  (纯文本渲染) │  │ (white-space │                │   │
│  │  │              │  │  : pre-wrap) │                │   │
│  │  └──────┬───────┘  └──────┬───────┘                │   │
│  │         │                  │                        │   │
│  │  ┌──────▼──────────────────▼───────┐               │   │
│  │  │      WebchatState (Signals)    │               │   │
│  │  │  • current_messages: Vec       │               │   │
│  │  │  • streaming_content: String   │               │   │
│  │  │  • is_streaming: bool          │               │   │
│  │  └──────┬─────────────────────────┘               │   │
│  │         │                                          │   │
│  │  ┌──────▼──────────────────┐  ┌──────────────┐   │   │
│  │  │  HTTP Client (gloo-net) │  │  WebSocket   │   │   │
│  │  │  → /api/v1/webchat/*    │  │  (web-sys)   │   │   │
│  │  │  (经 Web Server 代理)   │  │  ws://:8000  │   │   │
│  │  └─────────────────────────┘  └──────────────┘   │   │
│  └─────────────────────────────────────────────────────┘   │
└──────────────────────────┬──────────────────────────────────┘
                           │ HTTP /api/* (proxy)
                           │ WebSocket (direct)
┌──────────────────────────▼──────────────────────────────────┐
│  Web Server (Native) — Port 8090                            │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  Axum Router                                        │   │
│  │  • /*  → ServeDir (SPA fallback index.html)        │   │
│  │  • /api/* → proxy_handler → Gateway (port 3000)   │   │
│  └─────────────────────────────────────────────────────┘   │
└──────────────────────────┬──────────────────────────────────┘
                           │ HTTP REST
┌──────────────────────────▼──────────────────────────────────┐
│  Gateway (Native) — Port 3000 / 8000                        │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  Axum Router                                        │   │
│  │  • /api/v1/webchat/*   (REST 会话管理)              │   │
│  │  • /api/v1/channels/webchat/messages (消息注入)     │   │
│  │  • /ws                 (WebSocket Upgrade)          │   │
│  └────────────────────┬────────────────────────────────┘   │
│                       │                                     │
│  ┌────────────────────▼────────────────────────────────┐   │
│  │  MessageProcessor                                   │   │
│  │  • 去重 → 会话解析 → 多模态处理 → 记忆检索 → LLM    │   │
│  │  • Markdown 图片提取 → 混合消息拆分                 │   │
│  └────────────────────┬────────────────────────────────┘   │
│                       │                                     │
│  ┌────────────────────▼────────────────────────────────┐   │
│  │  Channel Registry → WebSocketManager → Frontend     │   │
│  │  SQLite (sqlx) 持久化 + 未送达消息恢复              │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. 核心数据模型

### 2.1 前端数据模型（`apps/web/src/webchat/mod.rs`）

```rust
use serde::{Deserialize, Serialize};

/// 聊天消息（前端与 Gateway 共享的 JSON 结构）
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: String,            // <-- 纯文本字符串，前端直接渲染
    pub timestamp: String,          // RFC3339
    pub attachments: Vec<Attachment>,
    pub metadata: MessageMetadata,
    pub token_usage: Option<TokenUsage>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct MessageMetadata {
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub is_streaming: bool,
    pub model: Option<String>,
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub edits: Vec<MessageEdit>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MessageEdit {
    pub timestamp: String,
    pub previous_content: String,
}

/// 附件（数据模型已定义，但前端尚未实现渲染组件）
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Attachment {
    pub id: String,
    pub file_name: String,
    pub file_type: String,
    pub file_size: u64,
    pub url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub is_image: bool,
}

/// Token 用量统计
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

impl TokenUsage {
    pub fn format(&self) -> String {
        format!("{} tokens", self.total_tokens)
    }
}
```

> **现状说明**：与 v0.1.0 设计中预想的 `ContentFragment` 联合体（`TextDelta` / `MarkdownBlock` / `JsonBlock`）不同，实际系统中 `ChatMessage.content` 始终是一个 `String`。Gateway 负责将 LLM 的输出以文本形式存入 SQLite，前端直接将该字符串放入 DOM。不存在自动格式识别或结构化片段分发机制。

### 2.2 Gateway 数据库模型（`apps/gateway/src/models.rs`）

```rust
/// 对应 SQLite `chat_messages` 表
pub struct ChatMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,           // "user" | "assistant" | "system"
    pub content: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub metadata: Option<serde_json::Value>,
    pub token_usage: Option<serde_json::Value>,
    pub ws_delivered_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// 对应 SQLite `chat_sessions` 表
pub struct ChatSession {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub is_pinned: bool,
    pub is_archived: bool,
    pub metadata: Option<serde_json::Value>,
}
```

### 2.3 Gateway 响应内容类型（`apps/gateway/src/services/llm_response.rs`）

```rust
/// LLM 响应的内容分类
pub enum MessageType {
    Text(String),
    Image { url: String, alt: String },
    Mixed(Vec<MessagePart>),
}

pub enum MessagePart {
    Text(String),
    Image { url: String, alt: String },
}

/// 从 Markdown 文本中提取图片语法 `![alt](url)`
pub fn extract_images_from_markdown(text: &str) -> Vec<(String, String)> {
    let re = regex::Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").expect("Invalid markdown regex");
    // ...
}
```

> **说明**：Gateway 服务端具备将 LLM 返回的 Markdown 中的图片语法提取出来，拆分为 `Text` + `Image` 混合消息的能力。但当前前端仅渲染 `Text` 部分，丢弃或忽略图片消息。

---

## 3. 前端实现（Leptos CSR / WASM）

### 3.1 项目构建配置

**`apps/web/Trunk.toml`**
```toml
[build]
target = "index.html"
dist = "dist"
public_url = "/"

[serve]
port = 8090
open = false

[[hooks]]
stage = "pre_build"
command = "sh"
command_arguments = ["-c", "echo 'Building WebChat SPA...'"]
```

**`apps/web/Cargo.toml`（关键依赖）**
```toml
[dependencies]
leptos = { version = "0.8.6", features = ["csr"] }
leptos_router = { version = "0.8.6" }
leptos_meta = { version = "0.8.6" }
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
web-sys = { version = "0.3", features = ["console", "Window", "Document", "HtmlElement", "WebSocket", "MessageEvent", "BinaryType", "Location", "Navigator", "Clipboard"] }
js-sys = "0.3"
gloo-net = "0.5"
gloo-storage = "0.3"
gloo-timers = "0.3"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = { version = "0.4", features = ["wasmbind"] }
```

> **注意**：`Cargo.toml` 中没有 `pulldown-cmark`、`comrak`、`syntect` 或任何 Markdown / 语法高亮库。前端不具备 Markdown 解析或代码高亮能力。

### 3.2 页面入口与 WebSocket 连接

**`apps/web/src/pages/webchat.rs`**

```rust
//! 已接入 WebChat Channel：通过 WebSocket 接收 Agent 回复，通过 HTTP POST 发送用户消息

use leptos::prelude::*;
use web_sys::{WebSocket, MessageEvent, BinaryType};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

/// WebChat 主页面
#[component]
pub fn WebChatPage() -> impl IntoView {
    // ... 状态初始化 ...

    // ═══════════════════════════════════════════════════════
    // WebSocket 连接：订阅 webchat 频道接收 Agent 回复
    // ═══════════════════════════════════════════════════════
    let auth_state_for_ws = auth_state.clone();
    let ws_needs_reconnect = RwSignal::new(true);

    create_effect(move |_| {
        if !ws_needs_reconnect.get() {
            return;
        }
        ws_needs_reconnect.set(false);

        let window = web_sys::window()?;
        let location = window.location();
        let protocol = location.protocol().ok()?;
        let host = location.host().ok()?;
        let port = location.port().ok()?;

        let ws_protocol = if protocol == "https:" { "wss" } else { "ws" };

        // Web 服务器(8090)不代理 WebSocket，需要直连 Gateway(8000)
        let ws_host = if port == "8090" {
            // 硬编码切换至 Gateway WebSocket 端口
            format!("{}:{}", host.trim_end_matches(":8090"), "8000")
        } else {
            host
        };

        let ws_url = format!("{}://{}/ws", ws_protocol, ws_host);
        let ws = WebSocket::new(&ws_url).ok()?;
        ws.set_binary_type(BinaryType::Arraybuffer);

        // ── onmessage: 接收 Gateway 推送 ──
        let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Some(text) = e.data().as_string() {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    match json.get("type").and_then(|v| v.as_str()) {
                        Some("chat_message") => {
                            if let Some(msg_json) = json.get("message") {
                                if let Ok(message) = serde_json::from_value::<ChatMessage>(msg_json.clone()) {
                                    chat_state.add_message(message);
                                    chat_state.set_is_sending(false);
                                    chat_state.set_is_streaming(false);
                                    chat_state.clear_streaming_content();
                                }
                            }
                        }
                        Some("chat_stream") => {
                            // 流式内容增量（若 Gateway 支持）
                            if let Some(chunk) = json.get("content").and_then(|v| v.as_str()) {
                                chat_state.append_streaming_content(chunk);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();

        // ── onopen: 发送订阅帧 ──
        let ws_for_open = ws.clone();
        let user_id = auth_state_for_ws.user_id().unwrap_or_default();
        let onopen = Closure::wrap(Box::new(move |_e: web_sys::Event| {
            let subscribe = serde_json::json!({
                "type": "subscribe",
                "channel": "webchat",
                "user_id": user_id
            });
            let _ = ws_for_open.send_with_str(&subscribe.to_string());

            // 2. 拉取 WebSocket 断开期间未投递的助手消息并补全
            //    调用 GET /api/v1/webchat/sessions/{id}/undelivered
            //    对每条消息调用 POST /api/v1/webchat/messages/{id}/ack
        }) as Box<dyn FnMut(_)>);
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();

        // ── onclose: 3 秒后自动重连 ──
        let ws_needs_reconnect_err = ws_needs_reconnect.clone();
        let onclose = Closure::wrap(Box::new(move |_e: web_sys::Event| {
            chat_state_err.set_error(Some("WebSocket connection closed".to_string()));
            let ws_needs_reconnect = ws_needs_reconnect_err.clone();
            let _ = gloo_timers::callback::Timeout::new(3000, move || {
                ws_needs_reconnect.set(true);
            });
        }) as Box<dyn FnMut(_)>);
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose.forget();

        // ── onerror ──
        let onerror = Closure::wrap(Box::new(move |_e: web_sys::Event| {
            chat_state_err.set_error(Some("WebSocket connection error".to_string()));
        }) as Box<dyn FnMut(_)>);
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onerror.forget();

        // 清理函数
        move || {
            let _ = ws.close();
        }
    });

    // ... 页面布局 ...
}
```

> **与 v0.1.0 设计的关键差异**：
> - 没有使用 `EventSource` / SSE，而是使用原生 `WebSocket`。
> - 没有 `SseEnvelope`、`seq` 序列号、乱序缓冲等概念。
> - 消息通过 JSON 对象直接传递，格式为 `{"type":"chat_message","message":{...}}`。
> - WebSocket 连接逻辑直接写在页面组件中，未封装为可复用的 `WebSocketClient` Hook（虽然 `src/gateway/websocket.rs` 中存在抽象定义，但 WebChat 页面未使用）。

### 3.3 消息发送流程

```rust
// pages/webchat.rs 中的发送处理
async fn handle_send(content: String, session_id: String, user_id: String) {
    // 1. 乐观更新：立即将用户消息加入本地列表
    let optimistic_msg = ChatMessage {
        id: format!("temp_{}", uuid::Uuid::new_v4()),
        role: MessageRole::User,
        content: content.clone(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        attachments: vec![],
        metadata: MessageMetadata::default(),
        token_usage: None,
    };
    chat_state.add_message(optimistic_msg);

    // 2. 设置流式/发送状态
    chat_state.set_is_sending(true);
    chat_state.set_is_streaming(true);
    chat_state.clear_streaming_content();

    // 3. 通过 HTTP POST 发送消息到 Gateway
    //    POST /api/v1/channels/webchat/messages
    //    Body: { user_id, content, session_id }
    match service.send_message(&session_id, &content, &user_id).await {
        Ok(_) => {
            // 等待 WebSocket 回推 assistant 消息
        }
        Err(e) => {
            chat_state.set_error(Some(e.to_string()));
            chat_state.set_is_sending(false);
            chat_state.set_is_streaming(false);
        }
    }
}
```

> **注意**：`api/webchat.rs` 中虽然定义了 `send_message_streaming()` 方法（POST `/webchat/sessions/{id}/messages/stream`），但 `WebChatPage` 并未调用它。Gateway 端的流式端点也是 stub 实现，返回 `{ "status": "started", "message": "Streaming endpoint is a stub..." }`。

### 3.4 消息列表与流式渲染组件

**`apps/web/src/components/webchat/message_list.rs`**

```rust
use leptos::prelude::*;
use crate::webchat::ChatMessage;

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
                    view! { <MessageItem message=message /> }
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

/// 流式消息组件：显示打字机缓冲内容 + 闪烁光标
#[component]
fn StreamingMessage(content: String) -> impl IntoView {
    view! {
        <div class="message assistant streaming">
            <div class="message-content">
                {content}
            </div>
            <div class="streaming-indicator">
                <span class="cursor">"▋"</span>
            </div>
        </div>
    }
}
```

**`apps/web/src/components/webchat/message_item.rs`**

```rust
use leptos::prelude::*;
use crate::webchat::ChatMessage;

#[component]
pub fn MessageItem(
    message: ChatMessage,
    #[prop(optional)] is_streaming: Option<bool>,
) -> impl IntoView {
    let is_streaming = is_streaming.unwrap_or(false);
    let is_user = matches!(message.role, crate::webchat::MessageRole::User);

    let class = format!(
        "message {} {}",
        if is_user { "user" } else { "assistant" },
        if is_streaming { "streaming" } else { "" }
    );

    view! {
        <div class=class>
            <div class="message-avatar">
                {if is_user { "👤" } else { "🤖" }}
            </div>
            <div class="message-content-wrapper">
                <div class="message-content">
                    // 纯文本直接渲染，无 Markdown / JSON / 代码高亮
                    {message.content.clone()}
                </div>
                <div class="message-meta">
                    <span class="message-time">{format_timestamp(&message.timestamp)}</span>
                    {if let Some(usage) = &message.token_usage {
                        view! {
                            <span class="token-usage">{usage.format()}</span>
                        }.into_any()
                    } else {
                        view! { <div /> }.into_any()
                    }}
                </div>
            </div>
        </div>
    }
}
```

> **渲染策略分析**：
> - `message.content` 直接作为文本节点放入 DOM，未经过任何解析器。
> - CSS 类 `.message-content` 设置了 `white-space: pre-wrap;`，保留了 LLM 输出中的换行和缩进，使代码块在视觉上具有一定可读性，但没有语法高亮、没有代码块背景、没有行号。
> - 不存在 v0.1.0 设计中描述的 `ContentDispatcher`、`MarkdownView`、`JsonTreeView` 等组件。

---

## 4. 前端状态管理

### 4.1 WebchatState 信号定义

**`apps/web/src/state/webchat.rs`**

```rust
use leptos::prelude::*;
use crate::webchat::{ChatMessage, ChatSession, SideQuestion};

#[derive(Clone)]
pub struct WebchatState {
    // 消息与流式状态
    pub current_messages: RwSignal<Vec<ChatMessage>>,
    pub streaming_content: RwSignal<String>,
    pub is_streaming: RwSignal<bool>,
    pub is_sending: RwSignal<bool>,

    // 会话状态
    pub sessions: RwSignal<Vec<ChatSession>>,
    pub current_session_id: RwSignal<Option<String>>,

    // 侧边提问 (/btw)
    pub side_questions: RwSignal<Vec<SideQuestion>>,

    // UI 状态
    pub show_side_panel: RwSignal<bool>,
    pub show_usage_panel: RwSignal<bool>,
    pub error_message: RwSignal<Option<String>>,
}

impl WebchatState {
    pub fn add_message(&self, msg: ChatMessage) {
        self.current_messages.update(|msgs| msgs.push(msg));
    }

    pub fn append_streaming_content(&self, chunk: &str) {
        self.streaming_content.update(|s| s.push_str(chunk));
    }

    pub fn clear_streaming_content(&self) {
        self.streaming_content.set(String::new());
    }

    pub fn set_is_streaming(&self, v: bool) {
        self.is_streaming.set(v);
    }

    pub fn set_is_sending(&self, v: bool) {
        self.is_sending.set(v);
    }

    pub fn set_error(&self, err: Option<String>) {
        self.error_message.set(err);
    }
}
```

> **流式模拟机制**：当前端调用 `send_message()` 后，`is_streaming` 被设为 `true`，`StreamingMessage` 组件显示一个带闪烁光标的占位气泡。但实际的字节增量**并非来自真正的流式传输**——如果 Gateway 通过 WebSocket 发送 `chat_stream` 事件，前端会追加内容；否则前端只是等待完整的 `chat_message` 事件到达后，一次性替换为最终消息。

---

## 5. 服务端实现（Gateway）

### 5.1 WebChat REST API 路由

**`apps/gateway/src/main.rs`**（路由注册片段）

```rust
let app = Router::new()
    // ... 其他路由 ...
    .route("/api/v1/webchat/sessions", get(list_sessions).post(create_session))
    .route("/api/v1/webchat/sessions/:id", delete(delete_session))
    .route("/api/v1/webchat/sessions/:id/messages", get(get_messages))
    .route("/api/v1/webchat/sessions/:id/title", put(update_title))
    .route("/api/v1/webchat/sessions/:id/pin", post(toggle_pin))
    .route("/api/v1/webchat/sessions/:id/archive", post(archive_session))
    .route("/api/v1/webchat/sessions/:id/clear", post(clear_messages))
    .route("/api/v1/webchat/sessions/:id/export", get(export_session))
    .route("/api/v1/webchat/sessions/import", post(import_session))
    .route("/api/v1/webchat/sessions/:id/messages/stream", post(send_message_streaming)) // STUB
    .route("/api/v1/webchat/sessions/:id/undelivered", get(get_undelivered_messages))
    .route("/api/v1/webchat/usage", get(get_usage))
    .route("/api/v1/webchat/side-questions", post(create_side_question))
    .route("/api/v1/webchat/messages/:id/ack", post(ack_message))
    // 消息注入端点（前端实际发送消息）
    .route("/api/v1/channels/webchat/messages", post(send_webchat_message))
    // WebSocket
    .route("/ws", get(ws_handler))
    // ...
```

### 5.2 消息注入与异步处理管道

**`apps/gateway/src/handlers/http/channels.rs`**

```rust
/// 前端发送用户消息的入口
pub async fn send_webchat_message(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = payload.get("user_id").and_then(|v| v.as_str()).unwrap_or("anonymous");
    let content = payload.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let session_id = payload.get("session_id").and_then(|v| v.as_str());

    // 构造 ChannelEvent 推入异步事件总线
    let event = ChannelEvent::MessageReceived {
        platform: PlatformType::WebChat,
        channel_id: session_id.unwrap_or("default").to_string(),
        user_id: user_id.to_string(),
        message_id: format!("msg_{}", uuid::Uuid::new_v4()),
        content: content.to_string(),
        timestamp: chrono::Utc::now(),
    };

    state.event_bus.send(event).await?;

    Ok(Json(serde_json::json!({
        "status": "received",
        "message_id": event.message_id()
    })))
}
```

### 5.3 核心消息处理器

**`apps/gateway/src/services/message_processor.rs`**（2563 行，核心编排器）

```rust
pub struct MessageProcessor {
    pub agent_runtime: Arc<dyn AgentRuntime>,
    pub webchat_service: Arc<WebchatService>,
    pub llm_service: Arc<LlmService>,
    pub memory_system: Arc<UnifiedMemorySystem>,
    pub channel_registry: Arc<ChannelRegistry>,
    pub deduplicator: MessageDeduplicator,
}

impl MessageProcessor {
    /// 主处理管道
    pub async fn handle_message_via_agent(&self, event: ChannelEvent) -> Result<(), AppError> {
        // 1. 去重检查
        if self.deduplicator.is_duplicate(&event) {
            return Ok(());
        }

        // 2. 会话解析：获取或创建 DB 会话
        let session = self.webchat_service
            .get_or_create_session(&event.user_id, event.session_id())
            .await?;

        // 3. 多模态处理：检测 image_key 标记，下载并编码为 base64 data URL
        let (content, images) = self.process_multimodal(&event).await?;

        // 4. 元问题快速路径（如 "有哪些 skills"）
        if let Some(answer) = self.try_meta_question(&content) {
            self.send_reply(session.id, answer, &event).await?;
            return Ok(());
        }

        // 5. 工作流命令匹配 /workflow <id>
        if let Some(workflow_result) = self.try_workflow_command(&content).await? {
            self.send_reply(session.id, workflow_result, &event).await?;
            return Ok(());
        }

        // 6. 技能匹配（当前已禁用，返回 None）
        // let skill_match = self.try_match_skill(&content).await?;

        // 7. 记忆检索：加载 USER.md / SOUL.md，搜索 UnifiedMemorySystem
        //    实现预算系统：简单查询 300-400 chars，复杂查询 1000-1200 chars
        let memory_context = self.build_memory_context(&event.user_id, &content).await?;

        // 8. 直接回答快速路径：若记忆中有精确 Q&A 对，直接返回
        if let Some(direct_answer) = self.try_direct_answer(&memory_context, &content) {
            self.send_reply(session.id, direct_answer, &event).await?;
            return Ok(());
        }

        // 9. 构造 LLM 请求（最多 6 轮历史，每轮 300 chars 上限）
        let task_config = TaskConfig {
            message: content,
            history: self.build_history(&session.id, 6, 300).await?,
            images,
            memory_context: Some(memory_context),
            // ...
        };

        // 10. 先发送占位提示 "🤖 正在思考，请稍候..."
        let placeholder_id = self.send_thinking_placeholder(&session.id, &event).await?;

        // 11. 后台执行 Agent 任务
        let processor = self.clone();
        let session_id = session.id.clone();
        tokio::spawn(async move {
            match processor.agent_runtime.execute_task(task_config).await {
                Ok(response) => {
                    // 解析 Markdown 图片，拆分为混合消息
                    let parts = parse_mixed_content(&response.text);
                    processor.send_mixed_message(&session_id, parts, &event).await?;
                }
                Err(e) => {
                    processor.send_error(&session_id, &e.to_string(), &event).await?;
                }
            }
            // 删除占位消息
            let _ = processor.webchat_service.delete_message(&placeholder_id).await;
            Ok::<(), AppError>(())
        });

        Ok(())
    }
}
```

### 5.4 多模态内容处理

```rust
/// 处理用户消息中的图片标记
async fn process_multimodal(&self, event: &ChannelEvent) -> Result<(String, Vec<ImageData>), AppError> {
    let mut content = event.content.clone();
    let mut images = Vec::new();

    // 检测 image_key: xxx 格式
    if let Some(image_key) = self.extract_image_key(&content) {
        info!("🖼️ 检测到图片: {}", image_key);

        // 通过 ChannelRegistry 下载图片字节
        if let Some(bytes) = self.channel_registry
            .download_image(&image_key, &event.message_id())
            .await?
        {
            // 根据 Magic Bytes 识别格式
            let format = detect_image_format(&bytes);
            let mime = format_to_mime(format);

            // Base64 编码为 data URL，供 LLM 视觉模型使用
            let base64_data = base64::engine::general_purpose::STANDARD.encode(&bytes);
            images.push(ImageData {
                mime_type: mime,
                data: base64_data,
            });
        }

        // 清除原始消息中的 image_key 标记
        content = self.clean_image_markers(&content);
    }

    Ok((content, images))
}

fn detect_image_format(data: &[u8]) -> ImageFormat {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        ImageFormat::Png
    } else if data.starts_with(b"\xFF\xD8\xFF") {
        ImageFormat::Jpeg
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        ImageFormat::Gif
    } else if data.len() > 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        ImageFormat::Webp
    } else {
        ImageFormat::Png // fallback
    }
}
```

### 5.5 LLM 内部流式接口（未暴露为 HTTP SSE）

**`apps/gateway/src/services/llm_service.rs`**

```rust
impl LlmService {
    /// 内部流式补全，返回 mpsc::Receiver<String> 字符块
    pub async fn process_message_stream(
        &self,
        request: LlmRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<String>, LlmError> {
        let (tx, rx) = tokio::sync::mpsc::channel::<String>(64);

        let provider = self.failover_provider().await?;
        let mut stream = provider.complete_stream(request).await?;

        tokio::spawn(async move {
            while let Some(chunk) = stream.recv().await {
                let _ = tx.send(chunk).await;
            }
        });

        Ok(rx)
    }
}
```

> **重要**：`complete_stream` 返回的 `Receiver` 仅在 Gateway 内部使用，用于向 WebSocketManager 逐字符/逐句推送。前端理论上可以接收 `chat_stream` 事件，但当前 `MessageProcessor` 的默认路径是等待完整响应后一次性发送 `chat_message`。

### 5.6 流式端点 Stub

**`apps/gateway/src/handlers/http/webchat.rs`**

```rust
/// Send a streaming message (stub — returns a stream ID)
pub async fn send_message_streaming(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let stream_id = format!("stream_{}", uuid::Uuid::new_v4());
    info!("Streaming message request for session {} (stub)", id);

    Ok(Json(serde_json::json!({
        "stream_id": stream_id,
        "status": "started",
        "message": "Streaming endpoint is a stub. Use regular message send for now."
    })))
}
```

---

## 6. 通信协议详解

### 6.1 HTTP REST API（会话管理）

Web 前端通过 `gloo-net` 向 `/api/v1/*` 发送请求，经 Web Server (port 8090) 代理至 Gateway (port 3000)。

| 端点 | 方法 | 用途 |
|------|------|------|
| `/api/v1/webchat/sessions` | GET/POST | 列表 / 创建会话 |
| `/api/v1/webchat/sessions/:id/messages` | GET | 获取历史消息 |
| `/api/v1/webchat/sessions/:id/undelivered` | GET | 获取 WebSocket 断线期间未送达消息 |
| `/api/v1/webchat/messages/:id/ack` | POST | 确认消息已通过 WebSocket 送达 |
| `/api/v1/channels/webchat/messages` | POST | **发送用户消息**（主入口） |

### 6.2 WebSocket 协议（实时推送）

**连接 URL**：`ws://<gateway_host>:8000/ws`

**订阅帧（前端 → Gateway）**：
```json
{
  "type": "subscribe",
  "channel": "webchat",
  "user_id": "user_xxx"
}
```

**消息推送帧（Gateway → 前端）**：
```json
{
  "type": "chat_message",
  "message": {
    "id": "msg_xxx",
    "role": "assistant",
    "content": "你好！有什么可以帮你的吗？",
    "timestamp": "2026-05-11T10:30:00Z",
    "attachments": [],
    "metadata": { "is_streaming": false },
    "token_usage": { "prompt_tokens": 12, "completion_tokens": 8, "total_tokens": 20 }
  }
}
```

**流式增量帧（Gateway → 前端，可选）**：
```json
{
  "type": "chat_stream",
  "session_id": "sess_xxx",
  "content": "下一",
  "finished": false
}
```

> **与 v0.1.0 SSE 协议的差异**：
> - 无 `SseEnvelope` 包装层，无 `seq` 序列号，无 `id` UUIDv7。
> - 无乱序重排缓冲区，WebSocket 基于 TCP 保证顺序。
> - 无心跳帧（依赖 TCP keepalive 或应用层 WebSocket ping/pong）。

---

## 7. 样式系统（实际 CSS）

### 7.1 消息样式

**`apps/web/style/main.css`**（约 2300 行，提取关键片段）

```css
/* 消息列表容器 */
.message-list {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1rem;
    overflow-y: auto;
    height: 100%;
}

/* 消息气泡基类 */
.message {
    display: flex;
    gap: 0.75rem;
    max-width: 85%;
    animation: messageAppear 0.3s ease-out;
}

@keyframes messageAppear {
    from { opacity: 0; transform: translateY(10px); }
    to   { opacity: 1; transform: translateY(0); }
}

/* 用户消息 */
.message.user {
    align-self: flex-end;
    flex-direction: row-reverse;
}
.message.user .message-content {
    background: linear-gradient(135deg, #3b82f6, #2563eb);
    color: white;
    border-radius: 1rem 1rem 0.25rem 1rem;
}

/* 助手消息 */
.message.assistant {
    align-self: flex-start;
}
.message.assistant .message-content {
    background: rgba(30, 41, 59, 0.7);
    backdrop-filter: blur(12px);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: #e2e8f0;
    border-radius: 1rem 1rem 1rem 0.25rem;
}

/* 流式状态 */
.message.assistant.streaming .message-content {
    border-left: 3px solid #3b82f6;
}

/* 内容区域：保留空白与换行 */
.message-content {
    padding: 0.875rem 1.125rem;
    font-size: 0.9375rem;
    line-height: 1.6;
    white-space: pre-wrap;        /* 关键：保留换行和缩进 */
    word-break: break-word;
}

/* 流式光标 */
.streaming-indicator .cursor {
    display: inline-block;
    color: #3b82f6;
    animation: blink 1s step-end infinite;
}
@keyframes blink {
    0%, 100% { opacity: 1; }
    50%      { opacity: 0; }
}

/* 元信息 */
.message-meta {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.25rem;
    font-size: 0.75rem;
    color: #64748b;
}
```

> **现状**：不存在 `.markdown-body`、`.code-block-wrapper`、`.json-tree-view`、`.hljs` 等类名。所有消息内容统一使用 `.message-content` + `white-space: pre-wrap` 渲染。

---

## 8. 目录结构（实际）

```
beebotos/
├── apps/
│   ├── web/                              # Leptos CSR 前端
│   │   ├── Cargo.toml                    # Leptos 0.8.6, gloo-net, web-sys
│   │   ├── Trunk.toml                    # Trunk 构建配置
│   │   ├── index.html                    # SPA 入口
│   │   ├── style/
│   │   │   ├── main.css                  # 主样式（~2300 行，含消息/流式样式）
│   │   │   └── components.css            # 组件样式（搜索/命令面板）
│   │   └── src/
│   │       ├── lib.rs                    # App 组件、路由、上下文提供
│   │       ├── main.rs                   # WASM 客户端入口
│   │       ├── bin/server.rs             # Axum 静态文件+代理服务器
│   │       ├── pages/
│   │       │   └── webchat.rs            # 主页面：WS 连接、HTTP 发送、布局
│   │       ├── components/webchat/
│   │       │   ├── mod.rs                # 组件导出
│   │       │   ├── message_list.rs       # 消息列表 + StreamingMessage
│   │       │   ├── message_item.rs       # 单条气泡（纯文本渲染）
│   │       │   ├── message_input.rs      # 文本输入框
│   │       │   ├── side_panel.rs         # /btw 侧边提问面板
│   │       │   ├── usage_panel.rs        # Token 用量统计
│   │       │   ├── session_list.rs       # 会话列表
│   │       │   └── session_item.rs       # 单条会话行
│   │       ├── webchat/
│   │       │   ├── mod.rs                # ChatMessage, Attachment, TokenUsage
│   │       │   ├── chat.rs               # ChatInterface, MessageComposer, slash 命令
│   │       │   ├── session.rs            # SessionManager + LocalStorage 持久化
│   │       │   ├── sidebar.rs            # SideQuestion 管理
│   │       │   └── mobile.rs             # 移动端适配桩
│   │       ├── state/
│   │       │   ├── mod.rs                # 状态提供者统一导出
│   │       │   └── webchat.rs            # WebchatState（Signals）
│   │       ├── api/
│   │       │   ├── mod.rs                # create_client, create_webchat_service
│   │       │   ├── client.rs             # ApiClient（gloo-net, auth, cache, CSRF）
│   │       │   ├── webchat.rs            # WebchatApiService（REST 端点封装）
│   │       │   └── gateway.rs            # Gateway 配置与端点常量
│   │       ├── gateway/
│   │       │   ├── mod.rs                # GatewayConfig, GatewayClient
│   │       │   ├── websocket.rs          # WebSocketMessage 抽象（未在页面使用）
│   │       │   ├── auth.rs               # TokenManager
│   │       │   └── scopes.rs             # GatewayScope 权限枚举
│   │       └── server/
│   │           ├── mod.rs                # Axum 路由 + SPA fallback
│   │           ├── proxy.rs              # /api/* 反向代理到 Gateway
│   │           └── config.rs             # AppConfig（TOML + 环境变量）
│   │
│   └── gateway/                          # Axum API Gateway
│       ├── Cargo.toml                    # Axum, sqlx, tokio, beebotos_agents
│       ├── migrations/001_initial.sql    # SQLite 初始 Schema
│       └── src/
│           ├── main.rs                   # AppState, 路由注册, 事件循环启动
│           ├── models.rs                 # AgentRecord, ApiKey, Session, AuditLog
│           ├── config.rs                 # BeeBotOSConfig 配置树
│           ├── auth.rs                   # JWT 鉴权
│           ├── middleware.rs             # 自定义 Tower 中间件
│           ├── telemetry.rs              # Metrics, tracing
│           ├── health.rs                 # 健康检查
│           ├── message_bus.rs            # 内存消息总线封装
│           ├── handlers/
│           │   ├── http/mod.rs           # HTTP 处理器模块列表
│           │   ├── http/webchat.rs       # WebChat REST API（会话/消息/用量）
│           │   ├── http/channels.rs      # 频道管理与消息注入
│           │   ├── http/agents_v2.rs     # Agent CRUD（AgentRuntime trait）
│           │   ├── http/workflows.rs     # 工作流编排
│           │   ├── http/skills.rs        # Skill 注册与执行
│           │   ├── http/mcp.rs           # MCP 服务器管理
│           │   ├── http/cron_jobs.rs     # 定时任务 API
│           │   └── websocket/mod.rs      # WebSocket upgrade 处理器
│           ├── services/
│           │   ├── message_processor.rs  # 核心消息管道（去重/多模态/记忆/LLM）
│           │   ├── llm_service.rs        # LLM 提供商管理与故障转移
│           │   ├── webchat_service.rs    # SQLite 会话与消息持久化
│           │   ├── agent_resolver.rs     # 频道→Agent 映射
│           │   ├── agent_runtime_adapter.rs # 新旧运行时桥接
│           │   ├── agent_runtime_manager.rs # 遗留运行时管理
│           │   ├── agent_service.rs      # Agent 内核集成
│           │   └── llm_response.rs       # 响应类型与 Markdown 图片提取
│           ├── clients/
│           │   ├── clawhub.rs            # ClawHub Skill 市场客户端
│           │   └── beehub.rs             # BeeHub 客户端
│           └── grpc/
│               └── skills.rs             # Skill 注册 gRPC 服务
```

---

## 9. 多模态支持矩阵（当前 vs 设计愿景）

| 模态 | Gateway 支持 | 前端渲染 | 说明 |
|------|-------------|---------|------|
| **纯文本** | ✅ | ✅ | 基础能力，`white-space: pre-wrap` 保留格式 |
| **Markdown 文本** | ⚠️ 部分 | ❌ | Gateway 可提取 `![alt](url)` 图片语法；前端不解析 Markdown，纯文本显示 |
| **代码块** | ✅（文本形式）| ⚠️ 无高亮 | 代码以缩进/围栏形式存在于文本中，无语法高亮、无复制按钮 |
| **JSON 结构化** | ✅（文本形式）| ❌ | Gateway 内部大量用 `serde_json::Value`；前端无 JSON Tree Viewer |
| **图片（用户上传）** | ✅ | ❌ | Gateway 通过 `image_key:` 标记识别、下载、base64 编码后送 LLM；前端不渲染用户附件 |
| **图片（LLM 返回）** | ✅ | ❌ | Gateway 通过 `extract_images_from_markdown` 拆分图片消息；前端忽略 `MessageType::Image` |
| **附件（文件）** | ⚠️ 数据模型 | ❌ | `Attachment` 结构已定义，但上传按钮无功能，无下载渲染 |
| **流式打字机** | ⚠️ 内部 | ⚠️ 模拟 | Gateway LLM 内部有流式接口，但默认等待完整响应；前端 `StreamingMessage` 多为占位符 |

---

## 10. 性能与可靠性机制

### 10.1 前端性能

- **虚拟列表**：当前未实现。消息使用 Leptos `<For>` 渲染，全部挂载在 DOM 中，长会话可能产生性能瓶颈。
- **本地持久化**：`SessionManager` 使用 `gloo-storage`（LocalStorage）保存会话列表、置顶/归档状态，刷新页面不丢失。
- **乐观更新**：用户消息在 HTTP 请求发送前即加入本地列表，减少感知延迟。

### 10.2 服务端性能

- **异步非阻塞**：所有 I/O（SQLite、LLM API、图片下载）均为 `async/await`，运行于 Tokio 线程池。
- **Agent 后台执行**：`tokio::spawn` 将 LLM 调用置于后台任务，HTTP 请求立即返回 `{"status":"received"}`，避免阻塞客户端。
- **消息去重**：`MessageDeduplicator` 基于 `(platform, message_id)` 防止重复处理。

### 10.3 可靠性

- **WebSocket 断线重连**：前端在 `onclose` 后等待 3 秒自动触发重连（`ws_needs_reconnect` Signal）。
- **未送达消息恢复**：重连后前端调用 `GET /undelivered` 获取断线期间的助手消息，并 `POST /ack` 确认。
- **占位消息清理**：若 Agent 执行失败，Gateway 会删除 `"🤖 正在思考，请稍候..."` 占位消息，避免僵尸消息。

---

## 11. 已知局限与演进方向

### 11.1 当前局限

1. **无 Markdown 渲染引擎**：前端直接渲染原始字符串，代码无高亮、表格无样式、标题无层级。
2. **无结构化内容识别**：不存在自动区分 `Text` / `Markdown` / `JSON` / `Code` 的分发器。
3. **无图片渲染**：`Attachment` 数据模型和 Gateway 的图片拆分逻辑已就绪，但前端 DOM 中不生成 `<img>`。
4. **SSE 流式未实现**：Gateway 的 `/messages/stream` 是 stub；前端也没有 SSE 客户端。
5. **WebSocket 抽象未被复用**：`src/gateway/websocket.rs` 中定义的 `WebSocketClient` 未被 `WebChatPage` 使用，页面直接操作 `web_sys::WebSocket`。
6. **无消息编辑/重新生成的 UI 反馈**：`MessageMetadata::edits` 字段存在，但 UI 未展示编辑历史。

### 11.2 演进建议（向 v0.1.0 设计对齐）

| 阶段 | 目标 | 关键工作 |
|------|------|---------|
| **Phase 1** | Markdown 基础渲染 | 前端引入 `pulldown-cmark`（WASM 兼容），添加 `MarkdownView` 组件，将 `.message-content` 的纯文本替换为解析后的 HTML（需 `sanitize`）。 |
| **Phase 2** | 代码块增强 | 在 `MarkdownView` 基础上，用 `web-sys` DOM 后处理为 `<pre>` 添加复制按钮；可引入 `syntect` 或纯 CSS 高亮。 |
| **Phase 3** | JSON Tree Viewer | 实现 `JsonTreeView` 递归组件，对可识别的 JSON 内容（如工具调用结果）提供折叠/展开、路径追踪。 |
| **Phase 4** | 图片与附件渲染 | 前端识别 `MessageType::Image` 和 `Attachment::is_image`，生成 `<img>` 标签；附件提供下载链接。 |
| **Phase 5** | 真流式传输 | Gateway 将 `process_message_stream` 接入 WebSocket，逐 token 推送 `chat_stream`；前端 `StreamingMessage` 改为增量追加。 |
| **Phase 6** | 内容自动识别 | 在 Gateway 或前端引入 `ContentRecognizer`，自动将 LLM 输出分类为 `TextDelta` / `MarkdownBlock` / `JsonBlock`。 |

---

## 12. 构建与部署

### 12.1 前端构建（Trunk）

```bash
cd beebotos/apps/web

# 开发模式（热重载 + 本地服务器 :8090）
trunk serve

# 生产构建（WASM 优化 + 资源内联）
trunk build --release

# 产物位于 dist/ 目录，包含 index.html + *.wasm + *.js
cp -r dist/ /var/www/beebotos-web/
```

### 12.2 Web Server 运行

```bash
# Web Server 同时作为静态文件服务器和 API 代理
cd beebotos/apps/web
cargo run --bin server

# 默认配置
# - 监听 0.0.0.0:8090
# - 静态文件从 dist/ 提供
# - /api/* 代理到 http://localhost:3000
```

### 12.3 Gateway 运行

```bash
cd beebotos/apps/gateway
cargo run

# 默认配置
# - HTTP API 监听 0.0.0.0:3000
# - WebSocket 监听 0.0.0.0:8000（或同端口不同 path）
# - SQLite 数据库位于 ./beebotos.db
```

---

## 附录 A：前端与 Gateway 版本对应关系

| 组件 | 实际版本 | v0.1.0 设计版本 | 差异说明 |
|------|---------|----------------|---------|
| Leptos | 0.8.6 | 0.7+ | 已升级，使用 `leptos::prelude::*` 新模块系统 |
| Axum | 0.7+ | 0.7+ | 一致 |
| pulldown-cmark | **未引入** | 0.12+ | 前端无 Markdown 解析能力 |
| web-sys | 0.3+ | 0.3+ | 一致，直接使用原生 WebSocket 而非 EventSource |
| SSE/EventSource | **未使用** | 核心传输协议 | 实际使用 WebSocket |
| SSR/Islands | **未使用** | 核心架构 | 实际使用纯 CSR |

---

## 附录 B：关键文件速查表

| 文件路径 | 职责 | 是否与设计文档一致 |
|---------|------|------------------|
| `apps/web/src/pages/webchat.rs` | WebSocket 连接、消息发送、页面布局 | ❌ 未使用 SSE/ContentDispatcher |
| `apps/web/src/components/webchat/message_item.rs` | 消息气泡渲染 | ❌ 纯文本，无 Markdown/JSON |
| `apps/web/src/components/webchat/message_list.rs` | 消息列表 + 流式占位 | ⚠️ StreamingMessage 为模拟态 |
| `apps/web/src/state/webchat.rs` | 响应式状态 | ✅ 符合 Leptos 信号模式 |
| `apps/web/src/api/webchat.rs` | HTTP API 封装 | ⚠️ streaming 方法未使用 |
| `apps/gateway/src/handlers/http/webchat.rs` | WebChat REST | ⚠️ stream 端点为 stub |
| `apps/gateway/src/handlers/http/channels.rs` | 消息注入 | ✅ 实际主入口 |
| `apps/gateway/src/services/message_processor.rs` | 核心处理管道 | ✅ 功能完整 |
| `apps/gateway/src/services/llm_response.rs` | Markdown 图片提取 | ✅ 但前端未消费 Image 类型 |
| `apps/gateway/src/services/webchat_service.rs` | SQLite 持久化 | ✅ 含 undelivered/ack 机制 |

---

*文档版本：v1.0.0*  
*适用项目：BeeBotOS WebChat 模块（apps/web + apps/gateway）*  
*编写规范：Rust Edition 2021, Leptos 0.8.6, Axum 0.7+*  
*基于代码提交时间：2026-05-11*
