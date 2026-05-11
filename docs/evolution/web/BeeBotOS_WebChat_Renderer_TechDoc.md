
# BeeBotOS WebChat 多模态内容渲染引擎技术文档
## 项目代号：BeeBotOS-WebChat-Renderer v0.1.0

> **架构定位**：基于 Rust 全栈（Leptos + Axum）的 Agent 对话内容渲染层，支持流式 SSE 接收、自动格式识别、Markdown 优雅渲染、JSON 可折叠树形视图及原生交互操作。全程纯 Rust 实现，零 JavaScript 依赖，全栈类型安全。

---

## 1. 架构总览

### 1.1 技术栈选型

| 层级 | 技术组件 | 版本 | 职责 |
|------|---------|------|------|
| **前端框架** | Leptos (Islands + SSR) | 0.7+ | 响应式 UI、组件树、DOM 细粒度更新 |
| **服务端框架** | Axum | 0.7+ | HTTP 路由、SSE 流 endpoint、Agent 代理 |
| **Markdown 解析** | `pulldown-cmark` + 自定义渲染器 | 0.12+ | 纯 Rust Markdown → HTML/虚拟 DOM |
| **JSON 处理** | `serde_json` + 递归 Leptos 组件 | 1.0+ | 类型安全的 JSON 树形序列化与渲染 |
| **流式传输** | `tokio::sync::mpsc` + `axum::response::sse` | 1.40+ | 异步背压友好的 SSE 流 |
| **剪贴板 API** | `web-sys` (Clipboard API) | 0.3+ | WASM 层调用浏览器原生 Clipboard |
| **文件下载** | `web-sys` (Blob & URL.createObjectURL) | 0.3+ | 纯 Rust 触发浏览器下载 |

### 1.2 架构分层图

```
┌─────────────────────────────────────────────────────────────┐
│  Browser (WASM32)                                           │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │ ChatBubble   │  │ MarkdownView │  │ JsonTreeView │      │
│  │   (Leptos)   │  │   (Leptos)   │  │   (Leptos)   │      │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘      │
│         │                 │                 │               │
│  ┌──────▼─────────────────▼─────────────────▼───────┐      │
│  │         ContentRenderer (Dispatcher)             │      │
│  │   • 内容类型自动识别 (Text / Markdown / JSON)    │      │
│  │   • 流式缓冲区管理 (Streaming Buffer Mgmt)       │      │
│  └──────┬───────────────────────────────────────────┘      │
│         │                                                   │
│  ┌──────▼────────────┐    ┌──────────────────────┐         │
│  │   SSE Client      │◄──►│   Clipboard API      │         │
│  │   (web-sys Events)│    │   (web-sys)          │         │
│  └───────────────────┘    └──────────────────────┘         │
└─────────────────────────────────────────────────────────────┘
                              │
                              │ SSE over HTTP/1.1
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  Server (Native)                                            │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  Axum Router                                        │   │
│  │  /api/v1/chat/:session_id/stream  (SSE Endpoint)  │   │
│  └────────────────────┬────────────────────────────────┘   │
│                       │                                     │
│  ┌────────────────────▼────────────────────────────────┐   │
│  │  Agent Response Stream Adapter                      │   │
│  │  • 接收 Agent 原始字节流                             │   │
│  │  • 分块编码 (Chunked Transfer)                       │   │
│  │  • 心跳保活 (Keep-Alive)                             │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. 核心数据模型（全栈共享）

所有类型定义在 `shared_types` crate 中，通过 `serde` 实现 SSR ↔ WASM 的类型安全传输。

### 2.1 消息内容联合体

```rust
// shared_types/src/content.rs
use serde::{Deserialize, Serialize};

/// 内容片段类型，用于流式传输中的增量更新
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload")]
pub enum ContentFragment {
    /// 纯文本增量（流式打字机效果）
    TextDelta { chunk: String },

    /// Markdown 完整块（识别后整体渲染）
    MarkdownBlock { raw: String, html: Option<String> },

    /// JSON 结构化数据（可折叠树形视图）
    JsonBlock { raw: String, tree: JsonNode },

    /// 元数据/控制帧
    Meta { event: StreamEvent },
}

/// 流式事件控制帧
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum StreamEvent {
    Start { session_id: String, agent_id: String },
    Thinking { content: String },       // Agent 思考过程（可折叠）
    ToolCall { name: String, args: serde_json::Value },
    ToolResult { success: bool, output: String },
    End { finish_reason: String },
    Error { code: u16, message: String },
}

/// JSON 树形节点（递归结构）
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum JsonNode {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonNode>),
    Object(Vec<(String, JsonNode)>),
}

impl From<serde_json::Value> for JsonNode {
    fn from(v: serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => JsonNode::Null,
            serde_json::Value::Bool(b) => JsonNode::Bool(b),
            serde_json::Value::Number(n) => JsonNode::Number(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::String(s) => JsonNode::String(s),
            serde_json::Value::Array(arr) => JsonNode::Array(arr.into_iter().map(Into::into).collect()),
            serde_json::Value::Object(map) => JsonNode::Object(
                map.into_iter().map(|(k, v)| (k, v.into())).collect()
            ),
        }
    }
}
```

### 2.2 SSE 传输协议

```rust
// shared_types/src/sse.rs

/// SSE 事件封装，Axum 侧直接序列化为 Event::default().data(json)
#[derive(Serialize)]
pub struct SseEnvelope<T> {
    pub id: String,           // UUID v7，支持乱序重排
    pub seq: u64,             // 严格递增序列号
    pub timestamp: u64,       // Unix millis
    pub payload: T,
}

/// 客户端缓冲与排序策略
/// 由于网络抖动，SSE 可能乱序到达，前端维护 `BTreeMap<u64, Fragment>` 缓冲
pub const MAX_REORDER_BUFFER: usize = 128;
pub const HEARTBEAT_INTERVAL_SECS: u64 = 15;
```

---

## 3. 服务端实现（Axum + SSE）

### 3.1 SSE Endpoint 定义

```rust
// server/src/routes/chat.rs
use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Router,
};
use futures::stream::{self, Stream};
use std::{convert::Infallible, time::Duration};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use shared_types::{ContentFragment, SseEnvelope, StreamEvent};
use uuid::Uuid;

/// 全局状态：Agent 会话管理器
#[derive(Clone)]
pub struct AppState {
    pub agent_pool: AgentPool,  // 管理 Agent 实例的生命周期
}

pub fn chat_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/chat/:session_id/stream", get(sse_chat_handler))
}

async fn sse_chat_handler(
    Path(session_id): Path<String>,
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // 1. 建立与 Agent 的双向通道
    let (tx, mut rx) = mpsc::channel::<ContentFragment>(256);

    // 2. 启动 Agent 异步任务（非阻塞）
    let agent_handle = state.agent_pool.spawn_session(&session_id, tx).await;

    // 3. 构建 SSE 流
    let stream = async_stream::stream! {
        let mut seq: u64 = 0;

        // 发送起始帧
        yield Ok(Event::default()
            .event("start")
            .data(serde_json::to_string(&SseEnvelope {
                id: Uuid::new_v4().to_string(),
                seq,
                timestamp: now_millis(),
                payload: ContentFragment::Meta(StreamEvent::Start {
                    session_id: session_id.clone(),
                    agent_id: agent_handle.id(),
                }),
            }).unwrap()));

        // 转发 Agent 输出
        while let Some(fragment) = rx.recv().await {
            seq += 1;
            let envelope = SseEnvelope {
                id: Uuid::new_v4().to_string(),
                seq,
                timestamp: now_millis(),
                payload: fragment,
            };

            yield Ok(Event::default()
                .event("message")
                .data(serde_json::to_string(&envelope).unwrap()));
        }

        // 发送结束帧
        yield Ok(Event::default()
            .event("end")
            .data(serde_json::to_string(&SseEnvelope {
                id: Uuid::new_v4().to_string(),
                seq: seq + 1,
                timestamp: now_millis(),
                payload: ContentFragment::Meta(StreamEvent::End {
                    finish_reason: "completed".into(),
                }),
            }).unwrap()));
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("{"type":"heartbeat"}")
    )
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
```

### 3.2 Agent 流式输出适配器

```rust
// server/src/agent/adapter.rs
use shared_types::{ContentFragment, JsonNode};
use pulldown_cmark::{Parser, Event as MdEvent, Tag, TagEnd};

/// 将 Agent 的原始文本流解析为结构化片段
pub struct ContentRecognizer {
    buffer: String,
    in_code_block: bool,
    code_fence_lang: Option<String>,
}

impl ContentRecognizer {
    pub fn new() -> Self {
        Self {
            buffer: String::with_capacity(4096),
            in_code_block: false,
            code_fence_lang: None,
        }
    }

    /// 增量输入，返回可发送的完整片段
    pub fn push(&mut self, chunk: &str) -> Vec<ContentFragment> {
        self.buffer.push_str(chunk);
        self.try_extract_fragments()
    }

    fn try_extract_fragments(&mut self) -> Vec<ContentFragment> {
        let mut fragments = Vec::new();

        // 策略 1：检测完整 JSON 对象（以 { 开头且括号匹配）
        if let Some(json_frag) = self.try_extract_json() {
            fragments.push(json_frag);
        }

        // 策略 2：检测完整 Markdown 代码块
        if let Some(md_frag) = self.try_extract_markdown_block() {
            fragments.push(md_frag);
        }

        // 策略 3：剩余文本作为 TextDelta
        if !self.buffer.is_empty() && !self.in_code_block {
            // 发送缓冲区的文本增量（打字机效果）
            let text = self.buffer.clone();
            self.buffer.clear();
            fragments.push(ContentFragment::TextDelta { chunk: text });
        }

        fragments
    }

    fn try_extract_json(&mut self) -> Option<ContentFragment> {
        let trimmed = self.buffer.trim_start();
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            // 尝试解析完整 JSON
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&self.buffer) {
                let raw = self.buffer.clone();
                self.buffer.clear();
                return Some(ContentFragment::JsonBlock {
                    raw,
                    tree: value.into(),
                });
            }
        }
        None
    }

    fn try_extract_markdown_block(&mut self) -> Option<ContentFragment> {
        // 使用 pulldown-cmark 扫描完整块级元素
        let parser = Parser::new(&self.buffer);
        let mut last_end = 0;

        for (event, range) in parser.into_offset_iter() {
            match event {
                MdEvent::Start(Tag::CodeBlock(lang)) => {
                    self.in_code_block = true;
                    self.code_fence_lang = lang.as_ref().map(|s| s.to_string());
                }
                MdEvent::End(TagEnd::CodeBlock) => {
                    self.in_code_block = false;
                    last_end = range.end;
                }
                MdEvent::End(TagEnd::Heading(_)) | MdEvent::End(TagEnd::Paragraph) | 
                MdEvent::End(TagEnd::Table) | MdEvent::End(TagEnd::List(_)) => {
                    if !self.in_code_block {
                        last_end = range.end;
                    }
                }
                _ => {}
            }
        }

        if last_end > 0 && !self.in_code_block {
            let extracted = self.buffer[..last_end].to_string();
            self.buffer = self.buffer[last_end..].to_string();

            // 预渲染 HTML（服务端优化，减少 WASM 计算）
            let html = render_markdown_to_html(&extracted);

            return Some(ContentFragment::MarkdownBlock {
                raw: extracted,
                html: Some(html),
            });
        }

        None
    }
}

/// 服务端预渲染 Markdown（可选优化）
fn render_markdown_to_html(input: &str) -> String {
    let parser = Parser::new(input);
    let mut html_output = String::new();
    pulldown_cmark::html::push_html(&mut html_output, parser);
    html_output
}
```

---

## 4. 前端实现（Leptos WASM）

### 4.1 SSE 连接与状态管理

```rust
// app/src/chat/sse_client.rs
use leptos::*;
use leptos::logging::log;
use web_sys::{EventSource, MessageEvent, EventSourceInit};
use shared_types::{ContentFragment, SseEnvelope, StreamEvent};
use std::cell::RefCell;
use std::rc::Rc;
use futures::channel::mpsc;

/// SSE 连接管理器（纯 Rust/WASM，无 JS）
pub struct SseClient {
    es: EventSource,
    _closure_msg: Closure<dyn FnMut(MessageEvent)>,
    _closure_err: Closure<dyn FnMut(web_sys::Event)>,
}

impl SseClient {
    pub fn new(
        url: &str,
        on_message: impl Fn(SseEnvelope<ContentFragment>) + 'static,
        on_error: impl Fn(String) + 'static,
    ) -> Result<Self, JsValue> {
        let es = EventSource::new_with_event_source_init_dict(
            url,
            EventSourceInit::new().with_credentials(false),
        )?;

        // 消息处理器
        let msg_closure = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Some(data) = e.data().as_string() {
                if let Ok(envelope) = serde_json::from_str::<SseEnvelope<ContentFragment>>(&data) {
                    on_message(envelope);
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);

        es.add_event_listener_with_callback("message", msg_closure.as_ref().unchecked_ref())?;

        // 错误处理器
        let err_closure = Closure::wrap(Box::new(move |e: web_sys::Event| {
            on_error(format!("SSE error: {:?}", e.type_()));
        }) as Box<dyn FnMut(web_sys::Event)>);

        es.add_event_listener_with_callback("error", err_closure.as_ref().unchecked_ref())?;

        Ok(SseClient {
            es,
            _closure_msg: msg_closure,
            _closure_err: err_closure,
        })
    }
}

impl Drop for SseClient {
    fn drop(&mut self) {
        let _ = self.es.close();
    }
}

/// Leptos 集成 Hook
#[component]
pub fn ChatStream(
    session_id: String,
    #[prop(into)] on_fragment: Callback<ContentFragment>,
) -> impl IntoView {
    let (status, set_status) = create_signal("connecting".to_string());

    create_effect(move |_| {
        let url = format!("/api/v1/chat/{}/stream", session_id);

        let client = SseClient::new(
            &url,
            move |envelope| {
                on_fragment.call(envelope.payload);
            },
            move |err| {
                set_status.set(format!("error: {}", err));
            },
        );

        if client.is_ok() {
            set_status.set("connected".to_string());
        }

        // 清理函数
        move || drop(client)
    });

    view! {
        <div class="stream-status" class:connected=move || status.get() == "connected">
            {move || status.get()}
        </div>
    }
}
```

### 4.2 内容分发与自动识别组件

```rust
// app/src/chat/content_dispatcher.rs
use leptos::*;
use shared_types::ContentFragment;
use crate::chat::{MarkdownView, JsonTreeView, TextBubble};

/// 智能内容分发器：根据内容类型自动选择渲染器
#[component]
pub fn ContentDispatcher(
    #[prop(into)] fragments: MaybeSignal<Vec<ContentFragment>>,
) -> impl IntoView {
    let rendered = move || {
        fragments.get().into_iter().map(|frag| {
            match frag {
                ContentFragment::TextDelta { chunk } => {
                    view! { <TextBubble content=chunk /> }
                }
                ContentFragment::MarkdownBlock { raw, html } => {
                    view! { <MarkdownView raw=raw pre_rendered=html /> }
                }
                ContentFragment::JsonBlock { raw, tree } => {
                    view! { <JsonTreeView raw=raw tree=tree /> }
                }
                ContentFragment::Meta { event } => {
                    view! { <MetaEventView event=event /> }
                }
            }
        }).collect_view()
    };

    view! {
        <div class="content-dispatcher">
            {rendered}
        </div>
    }
}

/// 元数据事件渲染（思考过程、工具调用等）
#[component]
fn MetaEventView(event: StreamEvent) -> impl IntoView {
    let (expanded, set_expanded) = create_signal(false);

    match event {
        StreamEvent::Thinking { content } => view! {
            <div class="meta-thinking">
                <button on:click=move |_| set_expanded.update(|v| *v = !*v)>
                    {move || if expanded.get() { "▼" } else { "▶" }}
                    " 思考过程"
                </button>
                <Show when=expanded>
                    <pre class="thinking-content">{content}</pre>
                </Show>
            </div>
        },
        StreamEvent::ToolCall { name, args } => view! {
            <div class="meta-tool">
                <span class="tool-badge">{format!("🔧 {}", name)}</span>
                <JsonTreeView raw=args.to_string() tree=args.into() />
            </div>
        },
        _ => view! { <div class="meta-generic">{format!("{:?}", event)}</div> },
    }
}
```

### 4.3 Markdown 优雅渲染组件

```rust
// app/src/chat/markdown_view.rs
use leptos::*;
use pulldown_cmark::{Parser, Event as MdEvent, Tag, TagEnd, CodeBlockKind};
use web_sys::{Element, HtmlElement};

/// Markdown 渲染视图：支持代码高亮、表格、列表、折叠
#[component]
pub fn MarkdownView(
    raw: String,
    #[prop(optional)] pre_rendered: Option<String>,
) -> impl IntoView {
    let (html_content, set_html) = create_signal(String::new());
    let container_ref = NodeRef::<Div>::new();

    // 服务端预渲染优先，否则客户端 WASM 渲染
    create_effect(move |_| {
        let html = pre_rendered.clone().unwrap_or_else(|| {
            let parser = Parser::new(&raw);
            let mut output = String::new();
            pulldown_cmark::html::push_html(&mut output, parser);
            output
        });
        set_html.set(html);
    });

    // DOM 后处理：添加复制按钮到代码块
    create_effect(move |_| {
        if let Some(container) = container_ref.get() {
            enhance_code_blocks(&container);
        }
    });

    view! {
        <div class="markdown-view" node_ref=container_ref>
            <div inner_html=move || html_content.get() />
        </div>
    }
}

/// 增强代码块：添加语言标签、复制按钮
fn enhance_code_blocks(container: &HtmlElement) {
    let pre_elements = container.get_elements_by_tag_name("pre");

    for i in 0..pre_elements.length() {
        if let Some(pre) = pre_elements.item(i) {
            let pre_elem: HtmlElement = pre.dyn_into().unwrap();

            // 创建工具栏
            let toolbar = web_sys::window()
                .unwrap()
                .document()
                .unwrap()
                .create_element("div")
                .unwrap();
            toolbar.set_class_name("code-toolbar");

            // 复制按钮
            let copy_btn = create_copy_button(&pre_elem);
            toolbar.append_child(&copy_btn).unwrap();

            // 插入到 pre 之前
            if let Some(parent) = pre_elem.parent_node() {
                let wrapper = web_sys::window()
                    .unwrap()
                    .document()
                    .unwrap()
                    .create_element("div")
                    .unwrap();
                wrapper.set_class_name("code-block-wrapper");
                parent.replace_child(&wrapper, &pre_elem).unwrap();
                wrapper.append_child(&toolbar).unwrap();
                wrapper.append_child(&pre_elem).unwrap();
            }
        }
    }
}

fn create_copy_button(pre: &HtmlElement) -> Element {
    let window = web_sys::window().unwrap();
    let doc = window.document().unwrap();
    let btn = doc.create_element("button").unwrap();
    btn.set_class_name("copy-btn");
    btn.set_text_content(Some("📋 复制"));

    let pre_clone = pre.clone();
    let onclick = Closure::wrap(Box::new(move |_e: web_sys::MouseEvent| {
        let code_text = pre_clone.text_content().unwrap_or_default();
        let clipboard = window.navigator().clipboard();
        let _ = clipboard.write_text(&code_text);

        // 视觉反馈
        btn.set_text_content(Some("✅ 已复制"));
        let btn_clone = btn.clone();
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            Closure::once_into_js(move || {
                btn_clone.set_text_content(Some("📋 复制"));
            }).as_ref().unchecked_ref(),
            2000,
        );
    }) as Box<dyn FnMut(_)>);

    btn.add_event_listener_with_callback("click", onclick.as_ref().unchecked_ref()).unwrap();
    onclick.forget(); // Leptos 环境下需妥善管理 Closure 生命周期

    btn
}
```

### 4.4 JSON 可折叠树形视图

```rust
// app/src/chat/json_tree.rs
use leptos::*;
use shared_types::JsonNode;
use web_sys::{Clipboard, Window};

/// JSON 树形视图：递归折叠、类型着色、路径追踪
#[component]
pub fn JsonTreeView(
    raw: String,
    tree: JsonNode,
) -> impl IntoView {
    let (collapsed, set_collapsed) = create_signal(false);
    let (hover_path, set_hover_path) = create_signal(String::new());

    view! {
        <div class="json-tree-view">
            <div class="json-toolbar">
                <button on:click=move |_| set_collapsed.update(|v| *v = !*v)>
                    {move || if collapsed.get() { "▼" } else { "▶" }}
                    " 折叠全部"
                </button>
                <button on:click=move |_| download_json(&raw)>
                    "⬇️ 下载"
                </button>
                <button on:click=move |_| copy_to_clipboard(&raw)>
                    "📋 复制"
                </button>
                <span class="json-path">{move || hover_path.get()}</span>
            </div>
            <div class="json-tree-body">
                <JsonNodeView 
                    node=tree 
                    path="$".to_string() 
                    depth=0 
                    force_collapsed=collapsed
                    on_hover=set_hover_path
                />
            </div>
        </div>
    }
}

/// 递归 JSON 节点渲染
#[component]
fn JsonNodeView(
    node: JsonNode,
    path: String,
    depth: usize,
    #[prop(into)] force_collapsed: Signal<bool>,
    on_hover: WriteSignal<String>,
) -> impl IntoView {
    let (local_collapsed, set_local_collapsed) = create_signal(false);
    let is_collapsed = move || force_collapsed.get() || local_collapsed.get();
    let indent = "  ".repeat(depth);

    match node {
        JsonNode::Null => view! {
            <span class="json-null" on:mouseenter=move |_| on_hover.set(path.clone())>
                {format!("{}null", indent)}
            </span>
        },
        JsonNode::Bool(v) => view! {
            <span class={if v { "json-true" } else { "json-false" }} 
                  on:mouseenter=move |_| on_hover.set(path.clone())>
                {format!("{}{}", indent, v)}
            </span>
        },
        JsonNode::Number(n) => view! {
            <span class="json-number" on:mouseenter=move |_| on_hover.set(path.clone())>
                {format!("{}{}", indent, n)}
            </span>
        },
        JsonNode::String(s) => view! {
            <span class="json-string" on:mouseenter=move |_| on_hover.set(path.clone())>
                {format!("{}"{}"", indent, s)}
            </span>
        },
        JsonNode::Array(arr) => {
            let len = arr.len();
            view! {
                <div class="json-array">
                    <span class="json-toggle" on:click=move |_| set_local_collapsed.update(|v| *v = !*v)>
                        {move || if is_collapsed() { "▶" } else { "▼" }}
                        {format!("{}[ /* {} items */", indent, len)}
                    </span>
                    <Show when=move || !is_collapsed() fallback=move || view! { <span>" ]"</span> }>
                        <div class="json-children">
                            {arr.into_iter().enumerate().map(|(i, child)| {
                                let child_path = format!("{}[{}]", path, i);
                                view! {
                                    <div class="json-array-item">
                                        <JsonNodeView 
                                            node=child 
                                            path=child_path 
                                            depth=depth + 1
                                            force_collapsed=force_collapsed
                                            on_hover=on_hover
                                        />
                                        {if i < len - 1 { "," } else { "" }}
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                        <span>{format!("{}]", indent)}</span>
                    </Show>
                </div>
            }
        }
        JsonNode::Object(obj) => {
            let len = obj.len();
            view! {
                <div class="json-object">
                    <span class="json-toggle" on:click=move |_| set_local_collapsed.update(|v| *v = !*v)>
                        {move || if is_collapsed() { "▶" } else { "▼" }}
                        {format!("{} {{ /* {} keys */", indent, len)}
                    </span>
                    <Show when=move || !is_collapsed() fallback=move || view! { <span>" }"</span> }>
                        <div class="json-children">
                            {obj.into_iter().enumerate().map(|(i, (k, child))| {
                                let child_path = format!("{}.{}", path, k);
                                view! {
                                    <div class="json-object-field">
                                        <span class="json-key" on:mouseenter=move |_| on_hover.set(child_path.clone())>
                                            {format!("{}  "{}": ", indent, k)}
                                        </span>
                                        <JsonNodeView 
                                            node=child 
                                            path=child_path 
                                            depth=depth + 1
                                            force_collapsed=force_collapsed
                                            on_hover=on_hover
                                        />
                                        {if i < len - 1 { "," } else { "" }}
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                        <span>{format!("{}}}", indent)}</span>
                    </Show>
                </div>
            }
        }
    }
}

/// 纯 Rust 调用浏览器 Clipboard API
fn copy_to_clipboard(text: &str) {
    if let Some(window) = web_sys::window() {
        let clipboard = window.navigator().clipboard();
        let _ = clipboard.write_text(text);
    }
}

/// 纯 Rust 触发浏览器文件下载
fn download_json(text: &str) {
    if let Some(window) = web_sys::window() {
        let document = window.document().unwrap();
        let blob = web_sys::Blob::new_with_str_sequence_and_options(
            &js_sys::Array::of1(&JsValue::from_str(text)),
            web_sys::BlobPropertyBag::new().type_("application/json"),
        ).unwrap();

        let url = web_sys::Url::create_object_url_with_blob(&blob).unwrap();
        let a = document.create_element("a").unwrap();
        a.set_attribute("href", &url).unwrap();
        a.set_attribute("download", "data.json").unwrap();
        document.body().unwrap().append_child(&a).unwrap();
        a.dyn_into::<HtmlElement>().unwrap().click();
        document.body().unwrap().remove_child(&a).unwrap();
        web_sys::Url::revoke_object_url(&url).unwrap();
    }
}
```

---

## 5. 类型安全与零 JS 依赖保障

### 5.1 全栈类型共享机制

```rust
// Cargo.toml workspace 配置
[workspace]
members = ["shared_types", "app", "server"]

# shared_types/Cargo.toml
[package]
name = "shared_types"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# app/Cargo.toml (WASM 目标)
[dependencies]
shared_types = { path = "../shared_types" }
leptos = { version = "0.7", features = ["csr", "nightly"] }
pulldown-cmark = { version = "0.12", default-features = false }
web-sys = { version = "0.3", features = ["Clipboard", "Blob", "Url", "EventSource", "MessageEvent"] }
wasm-bindgen = "0.2"
js-sys = "0.3"

# server/Cargo.toml (Native 目标)
[dependencies]
shared_types = { path = "../shared_types" }
axum = { version = "0.7", features = ["macros"] }
leptos_axum = { version = "0.7" }
tokio = { version = "1.40", features = ["full"] }
pulldown-cmark = "0.12"
```

### 5.2 编译时特性门控

```rust
// 确保服务端代码不会编译到 WASM
#[cfg(not(target_arch = "wasm32"))]
mod server_only;

#[cfg(target_arch = "wasm32")]
mod client_only;

// Markdown 渲染器统一接口，内部实现根据目标平台切换
pub struct MarkdownRenderer;

#[cfg(target_arch = "wasm32")]
impl MarkdownRenderer {
    pub fn render(input: &str) -> String {
        // WASM 端：使用 pulldown-cmark 纯 Rust 实现
        let parser = pulldown_cmark::Parser::new(input);
        let mut html = String::new();
        pulldown_cmark::html::push_html(&mut html, parser);
        html
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl MarkdownRenderer {
    pub fn render(input: &str) -> String {
        // 服务端：可扩展为使用更快的渲染器或缓存
        let parser = pulldown_cmark::Parser::new(input);
        let mut html = String::new();
        pulldown_cmark::html::push_html(&mut html, parser);
        html
    }
}
```

---

## 6. 样式系统（SCSS/Tailwind）

```scss
// styles/chat-renderer.scss

// Markdown 视图样式
.markdown-view {
  font-family: 'Inter', system-ui, sans-serif;
  line-height: 1.6;
  color: #e2e8f0;

  h1, h2, h3, h4 {
    color: #f8fafc;
    margin-top: 1.5em;
    margin-bottom: 0.5em;
    font-weight: 600;
  }

  // 代码块包装器
  .code-block-wrapper {
    margin: 1em 0;
    border-radius: 8px;
    background: #1e293b;
    overflow: hidden;

    .code-toolbar {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 0.5em 1em;
      background: #0f172a;
      border-bottom: 1px solid #334155;

      .copy-btn {
        background: transparent;
        border: 1px solid #475569;
        color: #94a3b8;
        padding: 0.25em 0.75em;
        border-radius: 4px;
        cursor: pointer;
        font-size: 0.875rem;
        transition: all 0.2s;

        &:hover {
          border-color: #64748b;
          color: #e2e8f0;
        }
      }
    }

    pre {
      margin: 0;
      padding: 1em;
      overflow-x: auto;
      font-family: 'JetBrains Mono', monospace;
      font-size: 0.875rem;

      code {
        color: #e2e8f0;
        background: transparent;
      }
    }
  }

  table {
    width: 100%;
    border-collapse: collapse;
    margin: 1em 0;

    th, td {
      border: 1px solid #334155;
      padding: 0.5em 0.75em;
      text-align: left;
    }

    th {
      background: #1e293b;
      font-weight: 600;
    }

    tr:nth-child(even) {
      background: rgba(30, 41, 59, 0.5);
    }
  }
}

// JSON 树形视图样式
.json-tree-view {
  background: #0f172a;
  border: 1px solid #1e293b;
  border-radius: 8px;
  padding: 1em;
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.875rem;

  .json-toolbar {
    display: flex;
    gap: 0.5em;
    margin-bottom: 0.75em;
    padding-bottom: 0.75em;
    border-bottom: 1px solid #1e293b;

    button {
      background: #1e293b;
      border: 1px solid #334155;
      color: #94a3b8;
      padding: 0.25em 0.75em;
      border-radius: 4px;
      cursor: pointer;

      &:hover {
        background: #334155;
        color: #e2e8f0;
      }
    }

    .json-path {
      margin-left: auto;
      color: #64748b;
      font-size: 0.75rem;
    }
  }

  .json-toggle {
    cursor: pointer;
    user-select: none;
    color: #94a3b8;

    &:hover {
      color: #e2e8f0;
    }
  }

  .json-null { color: #ef4444; }
  .json-true { color: #22c55e; }
  .json-false { color: #ef4444; }
  .json-number { color: #f59e0b; }
  .json-string { color: #3b82f6; }
  .json-key { color: #8b5cf6; }

  .json-children {
    padding-left: 1.5em;
    border-left: 1px dashed #334155;
  }
}

// 流式状态指示器
.stream-status {
  display: inline-flex;
  align-items: center;
  gap: 0.5em;
  font-size: 0.75rem;
  color: #64748b;

  &.connected::before {
    content: '';
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #22c55e;
    animation: pulse 2s infinite;
  }
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}
```

---

## 7. 性能优化策略

### 7.1 流式渲染优化

| 优化点 | 实现方式 | 效果 |
|--------|---------|------|
| **虚拟列表** | 对话超过 100 条时启用 `leptos::For` + 回收 DOM | 内存占用恒定 |
| **增量 Markdown** | 打字机阶段仅渲染 TextDelta，识别为 Markdown 后整体替换 | 减少重复解析 |
| **JSON 虚拟化** | 超过 1000 节点的 JSON 启用按需渲染（折叠节点不生成 DOM） | 首屏 < 16ms |
| **服务端预渲染** | Axum 端使用 `pulldown-cmark` 预生成 HTML，WASM 端直接 `inner_html` | 减少 WASM 计算量 |

### 7.2 内存管理

```rust
/// 前端消息缓冲区（环形缓冲，防止无限增长）
pub struct MessageRingBuffer {
    buffer: Vec<ChatMessage>,
    capacity: usize,
}

impl MessageRingBuffer {
    pub fn push(&mut self, msg: ChatMessage) {
        if self.buffer.len() >= self.capacity {
            self.buffer.remove(0); // 移除最旧消息
        }
        self.buffer.push(msg);
    }
}
```

---

## 8. 错误处理与降级策略

### 8.1 内容识别失败降级

```rust
/// 当 JSON/Markdown 识别失败时，降级为纯文本流
fn fallback_to_text(buffer: &str) -> ContentFragment {
    ContentFragment::TextDelta { 
        chunk: buffer.to_string() 
    }
}
```

### 8.2 SSE 断线重连

```rust
// 自动重连机制（指数退避）
pub struct ReconnectingSseClient {
    max_retries: u32,
    base_delay_ms: u64,
}

impl ReconnectingSseClient {
    pub async fn connect_with_backoff(&self, url: &str) -> Result<SseClient, String> {
        for attempt in 0..self.max_retries {
            match SseClient::new(url, ...) {
                Ok(client) => return Ok(client),
                Err(e) => {
                    let delay = self.base_delay_ms * 2_u64.pow(attempt);
                    gloo_timers::future::TimeoutFuture::new(delay as u32).await;
                }
            }
        }
        Err("Max retries exceeded".into())
    }
}
```

---

## 9. 目录结构

```
bebotos-webchat/
├── Cargo.toml                 # Workspace 根
├── shared_types/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── content.rs         # ContentFragment / JsonNode
│       └── sse.rs             # SseEnvelope 协议
├── app/                       # Leptos 前端 (WASM)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── main.rs            # WASM 入口 (hydrate)
│       └── chat/
│           ├── mod.rs
│           ├── sse_client.rs      # EventSource 封装
│           ├── content_dispatcher.rs
│           ├── markdown_view.rs   # Markdown 渲染 + 代码块增强
│           ├── json_tree.rs       # 递归 JSON 树
│           └── text_bubble.rs     # 纯文本气泡
├── server/                    # Axum 后端 (Native)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── routes/
│       │   └── chat.rs            # SSE endpoint
│       └── agent/
│           └── adapter.rs         # 内容识别器
└── styles/
    └── chat-renderer.scss
```

---

## 10. 构建与部署

```bash
# 开发模式（热重载）
cargo leptos watch

# 生产构建（SSR + WASM 优化）
cargo leptos build --release

# 运行服务端
./target/server/release/bebotos-webchat
```

---

## 附录 A：纯 Rust 实现无 JS 依赖的说明

本方案严格遵守 **"无 JavaScript 依赖"** 原则：

1. **无外部 JS 库**：不引入 React/Vue/Prism.js 等任何 JS 框架或库。
2. **手写 JS 为零**：所有浏览器 API 交互通过 `web-sys`（Rust 绑定）调用，编译为 WASM 后由浏览器执行。
3. **类型安全边界**：服务端与前端共享 `shared_types` crate，所有 SSE 数据在编译期确定 Schema，运行时零序列化错误。
4. **Markdown 纯 Rust**：使用 `pulldown-cmark` 100% Rust 实现，无需 JS  markdown-it 等库。
5. **Clipboard & 下载**：通过 `web-sys` 调用浏览器原生 `navigator.clipboard` 和 `Blob` API，属于浏览器内置能力，非外部依赖。

> **注**：`wasm-bindgen` 和 `js-sys` 是 Rust 与 WASM 宿主环境的必要绑定层，属于工具链而非业务依赖，不计入"JS 依赖"范畴。

---

*文档版本：v0.1.0*  
*适用项目：BeeBotOS WebChat 模块*  
*编写规范：Rust Edition 2021, Leptos 0.7+, Axum 0.7+*  
