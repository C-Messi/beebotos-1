//! WebSocket Hook
//!
//! 将 WebChat 页面中的原始 WebSocket 逻辑封装为可复用的 Leptos Hook。
//! 提供自动重连、订阅管理、消息/流式/错误回调。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use leptos::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::MessageEvent;

use crate::api::{create_client, create_webchat_service};
use crate::state::{use_auth_state, use_webchat_state};
use crate::utils::get_user_id;
use crate::webchat::ChatMessage;

fn merge_messages(
    chat_state: &crate::state::WebchatState,
    session_id: &str,
    messages: Vec<ChatMessage>,
) {
    chat_state.current_messages.update(|current| {
        let mut existing: std::collections::HashSet<String> =
            current.iter().map(|m| m.id.clone()).collect();
        for message in messages {
            if existing.insert(message.id.clone()) {
                current.push(message);
            }
        }
    });
    let snapshot = chat_state.current_messages.get_untracked();
    chat_state.message_cache.update(|cache| {
        cache.insert(session_id.to_string(), snapshot);
    });
}

/// WebSocket 连接状态
#[derive(Clone, Debug, PartialEq)]
pub enum WsConnectionStatus {
    Connecting,
    Connected,
    Disconnected,
    Error(String),
}

/// WebSocket 消息类型
#[derive(Clone, Debug)]
pub enum WsMessage {
    /// 完整聊天消息
    ChatMessage(ChatMessage),
    /// 流式内容增量
    StreamChunk {
        session_id: String,
        content: String,
        finished: bool,
    },
    /// 原始 JSON（未识别类型）
    Raw(serde_json::Value),
}

/// WebSocket Hook 配置
pub struct WebSocketConfig {
    /// Gateway WebSocket URL（如 ws://localhost:8000/ws）
    pub url: String,
    /// 订阅频道
    pub channel: String,
    /// 用户 ID
    pub user_id: String,
    /// 认证 Token
    pub token: Option<String>,
}

/// 初始化并管理 WebSocket 连接
///
/// 返回当前连接状态和一个发送消息的回调（如果需要）
pub fn use_websocket_chat() -> ReadSignal<WsConnectionStatus> {
    let chat_state = use_webchat_state();
    let auth_state = use_auth_state();
    let (status, status_set) = signal(WsConnectionStatus::Connecting);
    let reconnect_generation = RwSignal::new(0_u64);

    // 在 Effect 外部计算 user_id，避免 auth_state.user 变化触发 Effect 重新运行
    let user_id = auth_state
        .user
        .get_untracked()
        .as_ref()
        .map(|u| u.id.clone())
        .unwrap_or_else(get_user_id);

    Effect::new(move |_| {
        let _generation = reconnect_generation.get();
        status_set.set(WsConnectionStatus::Connecting);

        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let location = window.location();
        let protocol = match location.protocol() {
            Ok(p) => p,
            Err(_) => return,
        };
        let hostname = match location.hostname() {
            Ok(h) => h,
            Err(_) => return,
        };
        let port = location.port().ok().unwrap_or_default();
        let ws_protocol = if protocol == "https:" { "wss" } else { "ws" };

        // Web 服务器(8090)不代理 WebSocket，需要直连 Gateway(8000)
        let ws_host = if port == "8090" {
            format!("{}:8000", hostname)
        } else if port.is_empty() {
            hostname
        } else {
            format!("{}:{}", hostname, port)
        };

        let ws_url = format!("{}://{}/ws", ws_protocol, ws_host);
        let _ = web_sys::console::log_1(&format!("[websocket] connecting to {}", ws_url).into());

        let ws = match web_sys::WebSocket::new(&ws_url) {
            Ok(w) => w,
            Err(e) => {
                let _ = web_sys::console::error_1(
                    &format!("[websocket] failed to create: {:?}", e).into(),
                );
                status_set.set(WsConnectionStatus::Error(
                    "Failed to create WebSocket".into(),
                ));
                return;
            }
        };
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
        let closed_by_cleanup = Arc::new(AtomicBool::new(false));

        let chat_state_msg = chat_state.clone();
        let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(text) = e.data().dyn_into::<js_sys::JsString>() {
                let text_str = text.as_string().unwrap_or_default();
                let _ = web_sys::console::log_1(
                    &format!(
                        "[websocket] received: {}",
                        &text_str[..text_str.len().min(200)]
                    )
                    .into(),
                );
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text_str) {
                    match json.get("type").and_then(|v| v.as_str()) {
                        Some("chat_message") => {
                            let _ = web_sys::console::log_1(
                                &"[websocket] chat_message received".into(),
                            );
                            let session_id = json
                                .get("session_id")
                                .and_then(|v| v.as_str())
                                .map(str::to_string);
                            if session_id != chat_state_msg.current_session_id.get_untracked() {
                                return;
                            }
                            if let Some(msg_json) = json.get("message") {
                                match serde_json::from_value::<ChatMessage>(msg_json.clone()) {
                                    Ok(message) => {
                                        let msg_id = message.id.clone();
                                        if let Some(session_id) = session_id {
                                            merge_messages(
                                                &chat_state_msg,
                                                &session_id,
                                                vec![message],
                                            );
                                        }
                                        chat_state_msg.is_sending.set(false);
                                        chat_state_msg.is_streaming.set(false);
                                        let _ = web_sys::console::log_1(
                                            &format!("[websocket] message added: {}", msg_id)
                                                .into(),
                                        );
                                    }
                                    Err(e) => {
                                        let _ = web_sys::console::warn_1(
                                            &format!(
                                                "[websocket] failed to parse ChatMessage: {}",
                                                e
                                            )
                                            .into(),
                                        );
                                    }
                                }
                            }
                        }
                        Some("chat_stream") => {
                            if json.get("finished").and_then(|v| v.as_bool()) == Some(true) {
                                chat_state_msg.finish_streaming();
                                chat_state_msg.is_sending.set(false);
                                return;
                            }
                            if let Some(content) = json.get("content").and_then(|v| v.as_str()) {
                                if !content.is_empty() {
                                    if chat_state_msg.is_streaming.get() == false {
                                        chat_state_msg.start_streaming();
                                    }
                                    chat_state_msg.append_streaming_content(content);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();

        let ws_for_open = ws.clone();
        let chat_state_open = chat_state.clone();
        let auth_state_open = auth_state.clone();
        let user_id_for_open = user_id.clone();
        let onopen =
            Closure::wrap(Box::new(move |_e: web_sys::Event| {
                let _ = web_sys::console::log_1(&"[websocket] connected".into());
                status_set.set(WsConnectionStatus::Connected);
                let subscribe = serde_json::json!({
                    "type": "subscribe",
                    "channel": "webchat",
                    "user_id": user_id_for_open
                });
                let _ = ws_for_open.send_with_str(&subscribe.to_string());

                // 重连后刷新消息并拉取未送达消息
                let chat_state_refresh = chat_state_open.clone();
                let auth_state_refresh = auth_state_open.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    gloo_timers::future::TimeoutFuture::new(500).await;
                    let client = create_client();
                    client.set_auth_token(auth_state_refresh.get_token());
                    let service = create_webchat_service(client);
                    if let Some(session_id) = chat_state_refresh.current_session_id.get() {
                        // 刷新全部消息
                        let _ = service.get_messages(&session_id).await.map(|msgs| {
                            chat_state_refresh.current_messages.set(msgs.clone());
                            chat_state_refresh.message_cache.update(|cache| {
                                cache.insert(session_id.clone(), msgs);
                            });
                        });

                        // 拉取未送达消息
                        let _ = service.get_undelivered_messages(&session_id).await.map(
                            |undelivered| {
                                if !undelivered.is_empty() {
                                    for msg in undelivered.iter() {
                                        let msg_id = msg.id.clone();
                                        let svc = create_webchat_service(create_client());
                                        wasm_bindgen_futures::spawn_local(async move {
                                            let _ = svc.ack_message(&msg_id).await;
                                        });
                                    }
                                    merge_messages(&chat_state_refresh, &session_id, undelivered);
                                    chat_state_refresh.is_sending.set(false);
                                }
                            },
                        );
                    }
                });
            }) as Box<dyn FnMut(_)>);
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();

        let status_err = status_set.clone();
        let onerror = Closure::wrap(Box::new(move |_e: web_sys::Event| {
            let _ = web_sys::console::error_1(&"[websocket] error".into());
            status_err.set(WsConnectionStatus::Error(
                "WebSocket connection error".into(),
            ));
        }) as Box<dyn FnMut(_)>);
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onerror.forget();

        let reconnect_generation_close = reconnect_generation.clone();
        let status_close = status_set.clone();
        let closed_by_cleanup_close = closed_by_cleanup.clone();
        let onclose = Closure::wrap(Box::new(move |_e: web_sys::Event| {
            if closed_by_cleanup_close.load(Ordering::Relaxed) {
                return;
            }
            let _ = web_sys::console::warn_1(&"[websocket] closed, reconnecting in 3s...".into());
            status_close.set(WsConnectionStatus::Disconnected);
            wasm_bindgen_futures::spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(3_000).await;
                reconnect_generation_close.update(|generation| {
                    *generation = generation.saturating_add(1);
                });
            });
        }) as Box<dyn FnMut(_)>);
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose.forget();

        // 组件卸载时关闭 WebSocket
        let ws_cleanup = ws.clone();
        let closed_by_cleanup_cleanup = closed_by_cleanup.clone();
        on_cleanup(move || {
            closed_by_cleanup_cleanup.store(true, Ordering::Relaxed);
            let _ = ws_cleanup.close();
        });
    });

    status
}
