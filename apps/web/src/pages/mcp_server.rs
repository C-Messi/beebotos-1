//! MCP Server Management Page
//!
//! Manage Model Context Protocol client configurations.

use leptos::prelude::*;
use leptos::view;
use leptos_meta::*;

use crate::api::mcp_server::{
    McpServer, McpServerConfig, McpServerService, McpServerStatus, McpTool, McpTransport,
};
use crate::components::Modal;
use crate::state::use_app_state;

#[component]
pub fn McpServerPage() -> impl IntoView {
    let app_state = use_app_state();
    let search_input = RwSignal::new(String::new());
    let show_import_modal = RwSignal::new(false);
    let show_tools_modal = RwSignal::new(None::<String>);
    let show_edit_modal = RwSignal::new(None::<McpServerConfig>);
    let connecting_key = RwSignal::new(None::<String>);
    let disconnecting_key = RwSignal::new(None::<String>);
    let deleting_key = RwSignal::new(None::<String>);

    let service = {
        let client = app_state.api_client();
        McpServerService::new(client)
    };
    let service = StoredValue::new(service);

    // Load servers from local storage
    let servers = RwSignal::new(Vec::<McpServer>::new());

    let load_servers = {
        let servers = servers.clone();
        move || {
            match service.with_value(|s| s.list()) {
                Ok(data) => {
                    servers.set(data);
                }
                Err(e) => {
                    app_state.notify(
                        crate::state::notification::NotificationType::Error,
                        "Load Failed",
                        format!("Failed to load MCP servers: {}", e),
                    );
                }
            }
        }
    };
    let load_servers = StoredValue::new(load_servers);

    // Initial load
    load_servers.with_value(|f| f());

    // Demo data initialization: if empty, add sample MCP servers
    {
        let servers_val = servers.get_untracked();
        if servers_val.is_empty() {
            let demo1 = McpServerConfig {
                key: "metatrader".to_string(),
                name: "metatrader".to_string(),
                enabled: true,
                transport: McpTransport::Stdio {
                    command: "metatrader-mcp-server".to_string(),
                    args: vec![
                        "--login".to_string(),
                        "5050937026".to_string(),
                        "--password".to_string(),
                        "Mk*rw1Cg".to_string(),
                        "--server".to_string(),
                        "MetaQuotes-Demo".to_string(),
                        "--transport".to_string(),
                        "stdio".to_string(),
                        "--path".to_string(),
                        "D:\\Program\\MetaTrader 5\\terminal64.exe".to_string(),
                    ],
                    env: None,
                },
                description: None,
            };
            let _ = service.with_value(|s| s.save(demo1));
            load_servers.with_value(|f| f());

            // Set demo states: metatrader as connected
            servers.update(|list| {
                for s in list.iter_mut() {
                    if s.config.key == "metatrader" {
                        s.status = McpServerStatus::Connected;
                    }
                }
            });
        }
    }

    let filtered_servers = Signal::derive(move || {
        let search = search_input.get().to_lowercase();
        servers
            .get()
            .into_iter()
            .filter(|s| {
                if search.is_empty() {
                    true
                } else {
                    s.config.key.to_lowercase().contains(&search)
                        || s.config.name.to_lowercase().contains(&search)
                        || format!("{:?}", s.status).to_lowercase().contains(&search)
                        || transport_display(&s.config.transport)
                            .to_lowercase()
                            .contains(&search)
                }
            })
            .collect::<Vec<_>>()
    });

    let handle_refresh = {
        move || {
            load_servers.with_value(|f| f());
        }
    };

    view! {
        <Title text="MCP Server - BeeBotOS" />
        <div class="page mcp-server-page">
            // Page Header
            <div class="page-header">
                <div>
                    <h1>"MCP 客户端"</h1>
                    <p class="page-description">"管理当前用户可用的 Model Context Protocol 客户端。"</p>
                </div>
                <div class="page-header-actions">
                    <button
                        class="btn btn-secondary"
                        on:click=move |_| handle_refresh()
                    >
                        "刷新"
                    </button>
                    <button
                        class="btn btn-primary"
                        on:click=move |_| show_import_modal.set(true)
                    >
                        "+ 导入配置"
                    </button>
                </div>
            </div>

            // Search Bar
            <div class="mcp-search-bar">
                <span class="search-icon">"🔍"</span>
                <input
                    type="text"
                    placeholder="搜索 key、名称、状态或地址..."
                    prop:value=search_input
                    on:input=move |e| search_input.set(event_target_value(&e))
                />
            </div>

            // Server List
            <div class="mcp-server-list">
                {move || {
                    let list = filtered_servers.get();
                    if list.is_empty() {
                        view! { <McpServerEmpty /> }.into_any()
                    } else {
                        view! {
                            <div class="mcp-server-grid">
                                {list.into_iter().map(|server| {
                                    let key = server.config.key.clone();
                                    let config_clone = server.config.clone();
                                    let is_connecting = {
                                        let key = key.clone();
                                        let connecting_key = connecting_key.clone();
                                        move || connecting_key.get().as_ref() == Some(&key)
                                    };
                                    let is_disconnecting = {
                                        let key = key.clone();
                                        let disconnecting_key = disconnecting_key.clone();
                                        move || disconnecting_key.get().as_ref() == Some(&key)
                                    };
                                    let is_deleting = {
                                        let key = key.clone();
                                        let deleting_key = deleting_key.clone();
                                        move || deleting_key.get().as_ref() == Some(&key)
                                    };

                                    let on_tools = {
                                        let key = key.clone();
                                        let show_tools_modal = show_tools_modal.clone();
                                        move |_| show_tools_modal.set(Some(key.clone()))
                                    };
                                    let on_edit = {
                                        let show_edit_modal = show_edit_modal.clone();
                                        let config_clone = config_clone.clone();
                                        move |_| show_edit_modal.set(Some(config_clone.clone()))
                                    };
                                    let on_connect = {
                                        let key = key.clone();
                                        let connecting_key = connecting_key.clone();
                                        let load_servers = load_servers.clone();
                                        let service = service.clone();
                                        move |_| {
                                            connecting_key.set(Some(key.clone()));
                                            let key2 = key.clone();
                                            let connecting_key2 = connecting_key.clone();
                                            let load_servers = load_servers.clone();
                                            let service = service.clone();
                                            leptos::task::spawn_local(async move {
                                                match service.get_value().connect(&key2).await {
                                                    Ok(_) => {
                                                        load_servers.with_value(|f| f());
                                                    }
                                                    Err(e) => {
                                                        use_app_state().notify(
                                                            crate::state::notification::NotificationType::Error,
                                                            "Connection Failed",
                                                            e.to_string(),
                                                        );
                                                    }
                                                }
                                                connecting_key2.set(None);
                                            });
                                        }
                                    };
                                    let on_disconnect = {
                                        let key = key.clone();
                                        let disconnecting_key = disconnecting_key.clone();
                                        let load_servers = load_servers.clone();
                                        let service = service.clone();
                                        move |_| {
                                            disconnecting_key.set(Some(key.clone()));
                                            let key2 = key.clone();
                                            let disconnecting_key2 = disconnecting_key.clone();
                                            let load_servers = load_servers.clone();
                                            let service = service.clone();
                                            leptos::task::spawn_local(async move {
                                                match service.get_value().disconnect(&key2).await {
                                                    Ok(_) => {
                                                        load_servers.with_value(|f| f());
                                                    }
                                                    Err(e) => {
                                                        use_app_state().notify(
                                                            crate::state::notification::NotificationType::Error,
                                                            "Disconnect Failed",
                                                            e.to_string(),
                                                        );
                                                    }
                                                }
                                                disconnecting_key2.set(None);
                                            });
                                        }
                                    };
                                    let on_delete = {
                                        let key = key.clone();
                                        let deleting_key = deleting_key.clone();
                                        let load_servers = load_servers.clone();
                                        let service = service.clone();
                                        move |_| {
                                            deleting_key.set(Some(key.clone()));
                                            let key2 = key.clone();
                                            let deleting_key2 = deleting_key.clone();
                                            let load_servers = load_servers.clone();
                                            let service = service.clone();
                                            leptos::task::spawn_local(async move {
                                                match service.get_value().delete(&key2) {
                                                    Ok(()) => {
                                                        use_app_state().notify(
                                                            crate::state::notification::NotificationType::Success,
                                                            "Deleted",
                                                            format!("MCP server '{}' deleted", key2),
                                                        );
                                                        load_servers.with_value(|f| f());
                                                    }
                                                    Err(e) => {
                                                        use_app_state().notify(
                                                            crate::state::notification::NotificationType::Error,
                                                            "Delete Failed",
                                                            e.to_string(),
                                                        );
                                                    }
                                                }
                                                deleting_key2.set(None);
                                            });
                                        }
                                    };

                                    view! {
                                        <McpServerCard
                                            server=server.clone()
                                            on_tools=on_tools
                                            on_edit=on_edit
                                            on_connect=on_connect
                                            on_disconnect=on_disconnect
                                            on_delete=on_delete
                                            is_connecting=is_connecting
                                            is_disconnecting=is_disconnecting
                                            is_deleting=is_deleting
                                        />
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }
                }}
            </div>

            // Import Config Modal
            {move || if show_import_modal.get() {
                view! {
                    <ImportConfigModal
                        service=service.get_value()
                        on_close={
                            let show_import_modal = show_import_modal.clone();
                            move || show_import_modal.set(false)
                        }
                        on_success={
                            let show_import_modal = show_import_modal.clone();
                            let load_servers = load_servers.clone();
                            move || {
                                show_import_modal.set(false);
                                load_servers.with_value(|f| f());
                            }
                        }
                    />
                }.into_any()
            } else {
                view! { <></> }.into_any()
            }}

            // Tools Modal
            {move || if let Some(key) = show_tools_modal.get() {
                view! {
                    <ToolsModal
                        service=service.get_value()
                        key=key.clone()
                        on_close={
                            let show_tools_modal = show_tools_modal.clone();
                            move || show_tools_modal.set(None)
                        }
                    />
                }.into_any()
            } else {
                view! { <></> }.into_any()
            }}

            // Edit Modal
            {move || if let Some(config) = show_edit_modal.get() {
                view! {
                    <EditModal
                        service=service.get_value()
                        config=config.clone()
                        on_close={
                            let show_edit_modal = show_edit_modal.clone();
                            move || show_edit_modal.set(None)
                        }
                        on_save={
                            let show_edit_modal = show_edit_modal.clone();
                            let load_servers = load_servers.clone();
                            move || {
                                show_edit_modal.set(None);
                                load_servers.with_value(|f| f());
                            }
                        }
                    />
                }.into_any()
            } else {
                view! { <></> }.into_any()
            }}
        </div>
    }
}

#[component]
fn McpServerCard(
    server: McpServer,
    #[prop(into)] on_tools: Callback<()>,
    #[prop(into)] on_edit: Callback<()>,
    #[prop(into)] on_connect: Callback<()>,
    #[prop(into)] on_disconnect: Callback<()>,
    #[prop(into)] on_delete: Callback<()>,
    #[prop(into)] is_connecting: Signal<bool>,
    #[prop(into)] is_disconnecting: Signal<bool>,
    #[prop(into)] is_deleting: Signal<bool>,
) -> impl IntoView {
    let status_class = match server.status {
        McpServerStatus::Connected => "mcp-status-bar connected",
        McpServerStatus::Error => "mcp-status-bar error",
        _ => "mcp-status-bar disconnected",
    };

    let status_text = match server.status {
        McpServerStatus::Connected => "已连接",
        McpServerStatus::Error => "错误",
        McpServerStatus::Connecting => "连接中...",
        McpServerStatus::Disconnected => "未连接",
    };

    let status_dot_class = match server.status {
        McpServerStatus::Connected => "status-dot connected",
        McpServerStatus::Error => "status-dot error",
        _ => "status-dot disconnected",
    };

    let enabled_text = if server.config.enabled { "启用" } else { "禁用" };
    let transport_text = transport_display(&server.config.transport);
    let args_text = transport_args_display(&server.config.transport);

    let is_connected = server.status == McpServerStatus::Connected;
    let has_error = server.status == McpServerStatus::Error;

    view! {
        <div class="mcp-server-card">
            // Status bar
            <div class=status_class>
                <span>{server.config.name.clone()}</span>
                <span class="status-text">{status_text}</span>
            </div>

            // Card body
            <div class="mcp-card-body">
                <div class="mcp-card-header">
                    <div class="mcp-card-icon">
                        {server.config.name.chars().next().unwrap_or('M').to_uppercase().to_string()}
                    </div>
                    <div class="mcp-card-info">
                        <div class="mcp-card-title">{server.config.name.clone()}</div>
                        <div class="mcp-card-subtitle">{server.config.key.clone()}</div>
                    </div>
                    <div class=status_dot_class>
                        <span>{status_text}</span>
                    </div>
                </div>

                <div class="mcp-card-details">
                    <div class="mcp-detail-row">
                        <span class="mcp-detail-label">"Transport"</span>
                        <span class="mcp-detail-value">{transport_text}</span>
                    </div>
                    <div class="mcp-detail-row">
                        <span class="mcp-detail-label">"Enabled"</span>
                        <span class="mcp-detail-value">{enabled_text}</span>
                    </div>
                </div>

                // Command args block
                <div class="mcp-command-block">
                    <pre>{args_text}</pre>
                </div>

                // Error message
                {if has_error {
                    if let Some(ref err) = server.error_message {
                        view! {
                            <div class="mcp-error-message">{err.clone()}</div>
                        }.into_any()
                    } else {
                        view! { <></> }.into_any()
                    }
                } else {
                    view! { <></> }.into_any()
                }}

                // Actions
                <div class="mcp-card-actions">
                    <button class="btn btn-secondary btn-sm" on:click=move |_| on_tools.run(())>
                        "工具"
                    </button>
                    <button class="btn btn-secondary btn-sm" on:click=move |_| on_edit.run(())>
                        "编辑"
                    </button>
                    {if is_connected {
                        view! {
                            <button
                                class="btn btn-secondary btn-sm"
                                disabled=is_disconnecting
                                on:click=move |_| on_disconnect.run(())
                            >
                                {move || if is_disconnecting.get() { "断开中..." } else { "断开" }}
                            </button>
                        }.into_any()
                    } else {
                        view! {
                            <button
                                class="btn btn-primary btn-sm"
                                disabled=is_connecting
                                on:click=move |_| on_connect.run(())
                            >
                                {move || if is_connecting.get() { "连接中..." } else { "连接" }}
                            </button>
                        }.into_any()
                    }}
                    <button
                        class="btn btn-danger btn-sm"
                        disabled=is_deleting
                        on:click=move |_| on_delete.run(())
                    >
                        {move || if is_deleting.get() { "删除中..." } else { "删除" }}
                    </button>
                </div>
            </div>
        </div>
    }
}

#[component]
fn McpServerEmpty() -> impl IntoView {
    view! {
        <div class="empty-state">
            <div class="empty-icon">"🔌"</div>
            <h3>"暂无 MCP 客户端"</h3>
            <p>"点击右上角导入配置按钮添加 MCP 客户端"</p>
        </div>
    }
}

#[component]
fn ImportConfigModal(
    service: McpServerService,
    #[prop(into)] on_close: Callback<()>,
    #[prop(into)] on_success: Callback<()>,
) -> impl IntoView {
    let import_text = RwSignal::new(String::new());
    let is_saving = RwSignal::new(false);
    let service = StoredValue::new(service);

    let handle_save = {
        move || {
            let text = import_text.get();
            if text.trim().is_empty() {
                return;
            }
            is_saving.set(true);
            let service = service.clone();
            leptos::task::spawn_local(async move {
                match service.with_value(|s| s.import_config(&text)) {
                    Ok(servers) => {
                        use_app_state().notify(
                            crate::state::notification::NotificationType::Success,
                            "Import Successful",
                            format!("Imported {} MCP server(s)", servers.len()),
                        );
                        on_success.run(());
                    }
                    Err(e) => {
                        use_app_state().notify(
                            crate::state::notification::NotificationType::Error,
                            "Import Failed",
                            e.to_string(),
                        );
                    }
                }
                is_saving.set(false);
            });
        }
    };

    view! {
        <Modal title="导入 MCP 配置".to_string() on_close=move || on_close.run(())>
            <div class="mcp-modal-body">
                <div class="mcp-import-hint">
                    "支持 { \"mcpServers\": { \"key\": {...} } }、{ \"key\": {...} } 或包含 key 字段的单个配置。"
                </div>
                <textarea
                    class="mcp-json-editor"
                    placeholder="在此粘贴 MCP 配置 JSON..."
                    prop:value=import_text
                    on:input=move |e| import_text.set(event_target_value(&e))
                    rows=20
                />
            </div>
            <div class="mcp-modal-footer">
                <button class="btn btn-secondary" on:click=move |_| on_close.run(())>
                    "取消"
                </button>
                <button
                    class="btn btn-primary"
                    disabled=move || is_saving.get() || import_text.get().trim().is_empty()
                    on:click=move |_| handle_save()
                >
                    {move || if is_saving.get() { "保存中..." } else { "保存" }}
                </button>
            </div>
        </Modal>
    }
}

#[component]
fn ToolsModal(
    service: McpServerService,
    #[prop(into)] key: String,
    #[prop(into)] on_close: Callback<()>,
) -> impl IntoView {
    let tools = RwSignal::new(Vec::<McpTool>::new());
    let is_loading = RwSignal::new(true);
    let error_msg = RwSignal::new(None::<String>);
    let service = StoredValue::new(service);

    {
        let key = key.clone();
        leptos::task::spawn_local(async move {
            match service.get_value().list_tools(&key).await {
                Ok(data) => {
                    tools.set(data);
                    error_msg.set(None);
                }
                Err(e) => {
                    error_msg.set(Some(e.to_string()));
                }
            }
            is_loading.set(false);
        });
    }

    let server_name = key.clone();

    view! {
        <Modal title=format!("{} 工具", server_name) on_close=move || on_close.run(())>
            <div class="mcp-modal-body">
                {move || if is_loading.get() {
                    view! { <div class="mcp-loading">"加载中..."</div> }.into_any()
                } else if let Some(ref err) = error_msg.get() {
                    view! {
                        <div class="mcp-tools-error">{err.clone()}</div>
                    }.into_any()
                } else {
                    let tools_list = tools.get();
                    if tools_list.is_empty() {
                        view! {
                            <div class="mcp-tools-empty">"该 MCP 客户端没有可用工具"</div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="mcp-tools-list">
                                {tools_list.into_iter().map(|tool| {
                                    view! {
                                        <div class="mcp-tool-item">
                                            <div class="mcp-tool-name">{tool.name}</div>
                                            {tool.description.map(|d| view! {
                                                <div class="mcp-tool-desc">{d}</div>
                                            })}
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }
                }}
            </div>
            <div class="mcp-modal-footer">
                <button class="btn btn-primary" on:click=move |_| on_close.run(())>
                    "关闭"
                </button>
            </div>
        </Modal>
    }
}

#[component]
fn EditModal(
    service: McpServerService,
    config: McpServerConfig,
    #[prop(into)] on_close: Callback<()>,
    #[prop(into)] on_save: Callback<()>,
) -> impl IntoView {
    let edit_text = RwSignal::new(String::new());
    let is_saving = RwSignal::new(false);
    let service = StoredValue::new(service);

    // Initialize with current config JSON
    {
        let json = serde_json::to_string_pretty(&config).unwrap_or_default();
        edit_text.set(json);
    }

    let handle_save = {
        move || {
            let text = edit_text.get();
            match serde_json::from_str::<McpServerConfig>(&text) {
                Ok(new_config) => {
                    is_saving.set(true);
                    let service = service.clone();
                    leptos::task::spawn_local(async move {
                        match service.with_value(|s| s.save(new_config)) {
                            Ok(_) => {
                                use_app_state().notify(
                                    crate::state::notification::NotificationType::Success,
                                    "Saved",
                                    "MCP server configuration saved",
                                );
                                on_save.run(());
                            }
                            Err(e) => {
                                use_app_state().notify(
                                    crate::state::notification::NotificationType::Error,
                                    "Save Failed",
                                    e.to_string(),
                                );
                            }
                        }
                        is_saving.set(false);
                    });
                }
                Err(e) => {
                    use_app_state().notify(
                        crate::state::notification::NotificationType::Error,
                        "Invalid JSON",
                        e.to_string(),
                    );
                }
            }
        }
    };

    let title = format!("编辑 {}", config.name);

    view! {
        <Modal title=title on_close=move || on_close.run(())>
            <div class="mcp-modal-body">
                <textarea
                    class="mcp-json-editor"
                    prop:value=edit_text
                    on:input=move |e| edit_text.set(event_target_value(&e))
                    rows=20
                />
            </div>
            <div class="mcp-modal-footer">
                <button class="btn btn-secondary" on:click=move |_| on_close.run(())>
                    "取消"
                </button>
                <button
                    class="btn btn-primary"
                    disabled=move || is_saving.get()
                    on:click=move |_| handle_save()
                >
                    {move || if is_saving.get() { "保存中..." } else { "保存" }}
                </button>
            </div>
        </Modal>
    }
}

fn transport_display(transport: &McpTransport) -> String {
    match transport {
        McpTransport::Stdio { .. } => "stdio".to_string(),
        McpTransport::Sse { .. } => "sse".to_string(),
        McpTransport::Websocket { .. } => "websocket".to_string(),
    }
}

fn transport_args_display(transport: &McpTransport) -> String {
    match transport {
        McpTransport::Stdio { command, args, .. } => {
            let mut s = command.clone();
            for arg in args {
                s.push(' ');
                s.push_str(arg);
            }
            s
        }
        McpTransport::Sse { url, .. } => url.clone(),
        McpTransport::Websocket { url, .. } => url.clone(),
    }
}
