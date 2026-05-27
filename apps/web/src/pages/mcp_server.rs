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

    let servers = RwSignal::new(Vec::<McpServer>::new());

    let load_servers = {
        let servers = servers.clone();
        move || {
            let service = service.get_value();
            leptos::task::spawn_local(async move {
                match service.list().await {
                    Ok(data) => servers.set(data),
                    Err(e) => {
                        use_app_state().notify(
                            crate::state::notification::NotificationType::Error,
                            "Load Failed",
                            format!("Failed to load MCP servers: {}", e),
                        );
                    }
                }
            });
        }
    };
    let load_servers = StoredValue::new(load_servers);

    // Initial load
    load_servers.with_value(|f| f());

    let filtered_servers = Signal::derive(move || {
        let search = search_input.get().to_lowercase();
        servers
            .get()
            .into_iter()
            .filter(|s| {
                if search.is_empty() {
                    true
                } else {
                    s.config.name.to_lowercase().contains(&search)
                        || format!("{:?}", s.status()).to_lowercase().contains(&search)
                        || transport_display(&s.config.transport)
                            .to_lowercase()
                            .contains(&search)
                }
            })
            .collect::<Vec<_>>()
    });

    let summary = Signal::derive(move || server_summary(&servers.get()));

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
                    <h1>"MCP Server"</h1>
                    <p class="page-description">"管理当前可用的 Model Context Protocol 服务。"</p>
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

            <div class="mcp-summary-grid">
                <McpSummaryItem label="总数" value=move || summary.get().total />
                <McpSummaryItem label="已连接" value=move || summary.get().connected />
                <McpSummaryItem label="异常" value=move || summary.get().error />
                <McpSummaryItem label="未连接" value=move || summary.get().disconnected />
            </div>

            // Search Bar
            <div class="mcp-search-bar">
                <span class="search-icon">"🔍"</span>
                <input
                    type="text"
                    placeholder="搜索名称、状态或传输方式..."
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
                                    let key = server.config.name.clone();
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
                                            if !confirm_delete(&key) {
                                                return;
                                            }
                                            deleting_key.set(Some(key.clone()));
                                            let key2 = key.clone();
                                            let deleting_key2 = deleting_key.clone();
                                            let load_servers = load_servers.clone();
                                            let service = service.clone();
                                            leptos::task::spawn_local(async move {
                                                match service.get_value().delete(&key2).await {
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

#[derive(Clone, Copy, Default)]
struct McpServerSummary {
    total: usize,
    connected: usize,
    error: usize,
    disconnected: usize,
}

#[component]
fn McpSummaryItem<F>(label: &'static str, value: F) -> impl IntoView
where
    F: Fn() -> usize + Copy + Send + Sync + 'static,
{
    view! {
        <div class="mcp-summary-item">
            <span>{label}</span>
            <strong>{move || value().to_string()}</strong>
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
    let status = server.status();
    let status_text = match status {
        McpServerStatus::Connected => "已连接",
        McpServerStatus::Error => "错误",
        McpServerStatus::Connecting => "连接中...",
        McpServerStatus::Disconnected => "未连接",
    };

    let status_badge_class = match status {
        McpServerStatus::Connected => "mcp-status-badge connected",
        McpServerStatus::Error => "mcp-status-badge error",
        _ => "mcp-status-badge disconnected",
    };

    let transport_text = transport_display(&server.config.transport);
    let args_text = transport_args_display(&server.config.transport);
    let endpoint_label = transport_endpoint_display(&server.config.transport);

    let is_connected = status == McpServerStatus::Connected;
    let has_error = status == McpServerStatus::Error;

    view! {
        <div class="mcp-server-card">
            <div class="mcp-card-body">
                <div class="mcp-card-header">
                    <div class="mcp-card-icon">
                        {server.config.name.chars().next().unwrap_or('M').to_uppercase().to_string()}
                    </div>
                    <div class="mcp-card-info">
                        <div class="mcp-card-title">{server.config.name.clone()}</div>
                        <div class="mcp-card-subtitle">{endpoint_label}</div>
                    </div>
                    <div class=status_badge_class>
                        <span>{status_text}</span>
                    </div>
                </div>

                <div class="mcp-card-details">
                    <div class="mcp-detail-row">
                        <span class="mcp-detail-label">"Transport"</span>
                        <span class="mcp-detail-value">{transport_text}</span>
                    </div>
                    <div class="mcp-detail-row">
                        <span class="mcp-detail-label">"Runtime"</span>
                        <span class="mcp-detail-value">{status_text}</span>
                    </div>
                </div>

                <details class="mcp-command-details">
                    <summary>"配置摘要"</summary>
                    <pre>{args_text}</pre>
                </details>

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

                <div class="mcp-card-actions">
                    <button
                        class="btn btn-secondary btn-sm"
                        disabled=!is_connected
                        on:click=move |_| on_tools.run(())
                    >
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
            <h3>"暂无 MCP Server"</h3>
            <p>"点击右上角导入配置按钮添加 MCP Server"</p>
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
                match service.get_value().import_config(&text).await {
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
    let selected_tool = RwSignal::new(None::<McpTool>);
    let call_arguments = RwSignal::new("{}".to_string());
    let call_output = RwSignal::new(None::<String>);
    let is_calling = RwSignal::new(false);
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
    let run_tool = StoredValue::new({
        let key = key.clone();
        let service = service.clone();
        move || {
            let Some(tool) = selected_tool.get() else {
                return;
            };

            let args = match serde_json::from_str::<serde_json::Value>(&call_arguments.get()) {
                Ok(value) => match value.as_object() {
                    Some(map) => map.clone(),
                    None => {
                        use_app_state().notify(
                            crate::state::notification::NotificationType::Error,
                            "Invalid Arguments",
                            "工具参数必须是 JSON object",
                        );
                        return;
                    }
                },
                Err(e) => {
                    use_app_state().notify(
                        crate::state::notification::NotificationType::Error,
                        "Invalid JSON",
                        e.to_string(),
                    );
                    return;
                }
            };

            is_calling.set(true);
            call_output.set(None);
            let key = key.clone();
            let tool_name = tool.name.clone();
            let service = service.clone();
            leptos::task::spawn_local(async move {
                match service.get_value().call_tool(&key, &tool_name, args).await {
                    Ok(result) => {
                        let prefix = if result.is_error {
                            "工具返回错误"
                        } else {
                            "调用成功"
                        };
                        call_output.set(Some(format!("{}\n{}", prefix, result.output)));
                    }
                    Err(e) => {
                        call_output.set(Some(format!("调用失败\n{}", e)));
                    }
                }
                is_calling.set(false);
            });
        }
    });

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
                            <div class="mcp-tools-toolbar">
                                <span>{format!("{} 个工具", tools_list.len())}</span>
                            </div>
                            <div class="mcp-tools-list">
                                {tools_list.into_iter().map(|tool| {
                                    let tool_for_select = tool.clone();
                                    let selected_tool_name = tool.name.clone();
                                    view! {
                                        <div class="mcp-tool-item">
                                            <div class="mcp-tool-header">
                                                <div>
                                                    <div class="mcp-tool-name">{tool.name}</div>
                                                    <div class="mcp-tool-schema">{schema_summary(&tool.parameters)}</div>
                                                </div>
                                                <button
                                                    class="btn btn-secondary btn-sm"
                                                    on:click=move |_| {
                                                        selected_tool.set(Some(tool_for_select.clone()));
                                                        call_arguments.set(sample_arguments(&tool_for_select.parameters));
                                                        call_output.set(None);
                                                    }
                                                >
                                                    {move || if selected_tool.get().as_ref().map(|t| t.name.as_str()) == Some(selected_tool_name.as_str()) {
                                                        "已选择"
                                                    } else {
                                                        "测试"
                                                    }}
                                                </button>
                                            </div>
                                            {tool.description.map(|d| view! {
                                                <div class="mcp-tool-desc">{d}</div>
                                            })}
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                            {move || selected_tool.get().map(|tool| view! {
                                <div class="mcp-tool-runner">
                                    <div class="mcp-tool-runner-header">
                                        <strong>{format!("测试 {}", tool.name)}</strong>
                                        <span>"JSON 参数"</span>
                                    </div>
                                    <textarea
                                        class="mcp-json-editor mcp-tool-args"
                                        prop:value=call_arguments
                                        on:input=move |e| call_arguments.set(event_target_value(&e))
                                        rows=8
                                    />
                                    <div class="mcp-tool-runner-actions">
                                        <button
                                            class="btn btn-primary btn-sm"
                                            disabled=move || is_calling.get()
                                            on:click=move |_| run_tool.with_value(|f| f())
                                        >
                                            {move || if is_calling.get() { "调用中..." } else { "调用工具" }}
                                        </button>
                                    </div>
                                    {move || call_output.get().map(|output| view! {
                                        <pre class="mcp-tool-output">{output}</pre>
                                    })}
                                </div>
                            })}
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
    let app_state = use_app_state();

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
                    let service = service.get_value();
                    leptos::task::spawn_local(async move {
                        match service.save(new_config).await {
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
                                is_saving.set(false);
                            }
                        }
                    });
                }
                Err(e) => {
                    app_state.notify(
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
        McpTransport::Http { .. } => "http".to_string(),
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
        McpTransport::Http { url, .. } => url.clone(),
    }
}

fn transport_endpoint_display(transport: &McpTransport) -> String {
    match transport {
        McpTransport::Stdio { command, args, .. } => args
            .first()
            .map(|arg| format!("{} {}", command, arg))
            .unwrap_or_else(|| command.clone()),
        McpTransport::Http { url, .. } => url.clone(),
    }
}

fn server_summary(servers: &[McpServer]) -> McpServerSummary {
    let mut summary = McpServerSummary {
        total: servers.len(),
        ..Default::default()
    };

    for server in servers {
        match server.status() {
            McpServerStatus::Connected => summary.connected += 1,
            McpServerStatus::Error => summary.error += 1,
            McpServerStatus::Connecting | McpServerStatus::Disconnected => {
                summary.disconnected += 1
            }
        }
    }

    summary
}

fn schema_summary(schema: &serde_json::Value) -> String {
    let props = schema
        .get("properties")
        .and_then(|v| v.as_object())
        .map(|p| p.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    if props.is_empty() {
        "无参数".to_string()
    } else {
        format!("参数: {}", props.join(", "))
    }
}

fn sample_arguments(schema: &serde_json::Value) -> String {
    let mut sample = serde_json::Map::new();
    let required = schema
        .get("required")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if let Some(properties) = schema.get("properties").and_then(|v| v.as_object()) {
        for key in required.iter().filter_map(|v| v.as_str()) {
            let value = properties
                .get(key)
                .map(sample_value)
                .unwrap_or(serde_json::Value::Null);
            sample.insert(key.to_string(), value);
        }
    }

    serde_json::to_string_pretty(&sample).unwrap_or_else(|_| "{}".to_string())
}

fn sample_value(schema: &serde_json::Value) -> serde_json::Value {
    match schema.get("type").and_then(|v| v.as_str()) {
        Some("number") | Some("integer") => serde_json::json!(0),
        Some("boolean") => serde_json::json!(false),
        Some("array") => serde_json::json!([]),
        Some("object") => serde_json::json!({}),
        _ => serde_json::json!(""),
    }
}

fn confirm_delete(name: &str) -> bool {
    web_sys::window()
        .and_then(|window| {
            window
                .confirm_with_message(&format!("确认删除 MCP Server '{}'？", name))
                .ok()
        })
        .unwrap_or(false)
}
