//! JSON 树形视图组件
//!
//! 递归渲染 JSON 数据，支持折叠/展开、类型着色、路径追踪、复制和下载。

use leptos::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::HtmlElement;

/// JSON 树形视图
#[component]
pub fn JsonTreeView(
    /// JSON 原始文本（用于复制/下载）
    raw: String,
    /// 已解析的 JSON Value
    value: serde_json::Value,
) -> impl IntoView {
    let (collapsed, set_collapsed) = signal(false);
    let (hover_path, set_hover_path) = signal(String::new());

    view! {
        <div class="json-tree-view">
            <div class="json-toolbar">
                <button on:click=move |_| set_collapsed.update(|v| *v = !*v)>
                    {move || if collapsed.get() { "▼ 展开全部" } else { "▶ 折叠全部" }}
                </button>
                <button on:click={
                    let raw = raw.clone();
                    move |_| copy_to_clipboard(&raw)
                }>
                    "📋 复制"
                </button>
                <button on:click={
                    let raw = raw.clone();
                    move |_| download_json(&raw)
                }>
                    "⬇️ 下载"
                </button>
                <span class="json-path">{move || hover_path.get()}</span>
            </div>
            <div class="json-tree-body">
                <JsonValueView
                    value=value.clone()
                    path="$".to_string()
                    depth=0
                    force_collapsed=collapsed
                    on_hover=set_hover_path
                />
            </div>
        </div>
    }
}

/// 递归 JSON 值渲染
#[component]
fn JsonValueView(
    value: serde_json::Value,
    path: String,
    depth: usize,
    #[prop(into)] force_collapsed: Signal<bool>,
    on_hover: WriteSignal<String>,
) -> impl IntoView {
    let (local_collapsed, set_local_collapsed) = signal(false);
    let is_collapsed = move || force_collapsed.get() || local_collapsed.get();
    let indent = "  ".repeat(depth);

    match value {
        serde_json::Value::Null => view! {
            <span class="json-null" on:mouseenter=move |_| on_hover.set(path.clone())>
                {format!("{}null", indent)}
            </span>
        }.into_any(),

        serde_json::Value::Bool(v) => view! {
            <span class={if v { "json-true" } else { "json-false" }}
                  on:mouseenter=move |_| on_hover.set(path.clone())>
                {format!("{}{}", indent, v)}
            </span>
        }.into_any(),

        serde_json::Value::Number(n) => view! {
            <span class="json-number" on:mouseenter=move |_| on_hover.set(path.clone())>
                {format!("{}{}", indent, n)}
            </span>
        }.into_any(),

        serde_json::Value::String(s) => view! {
            <span class="json-string" on:mouseenter=move |_| on_hover.set(path.clone())>
                {format!("{}{}", indent, serde_json::to_string(&s).unwrap_or_else(|_| "\"\"".to_string()))}
            </span>
        }.into_any(),

        serde_json::Value::Array(arr) => {
            let len = arr.len();
            if len == 0 {
                view! {
                    <span class="json-array" on:mouseenter=move |_| on_hover.set(path.clone())>
                        {format!("{}[ /* 0 items */ ]", indent)}
                    </span>
                }.into_any()
            } else {
                let arr = std::sync::Arc::new(arr);
                view! {
                    <div class="json-array">
                        <span class="json-toggle" on:click=move |_| set_local_collapsed.update(|v| *v = !*v)>
                            {move || if is_collapsed() { "▶" } else { "▼" }}
                            {format!("{}[ /* {} items */", indent, len)}
                        </span>
                        <Show when=move || !is_collapsed() fallback=move || view! { <span>" ]"</span> }>
                            <div class="json-children">
                                {arr.iter().enumerate().map(|(i, child)| {
                                    let child_path = format!("{}[{}]", path, i);
                                    let child = child.clone();
                                    view! {
                                        <div class="json-array-item">
                                            <JsonValueView
                                                value=child
                                                path=child_path
                                                depth=depth + 1
                                                force_collapsed=force_collapsed
                                                on_hover=on_hover
                                            />
                                            {if i < len - 1 { ", " } else { "" }}
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                            <span>{format!("{}]", indent)}</span>
                        </Show>
                    </div>
                }.into_any()
            }
        }

        serde_json::Value::Object(map) => {
            let len = map.len();
            if len == 0 {
                view! {
                    <span class="json-object" on:mouseenter=move |_| on_hover.set(path.clone())>
                        {format!("{} {{ /* 0 keys */ }}", indent)}
                    </span>
                }.into_any()
            } else {
                let entries: std::sync::Arc<Vec<_>> = std::sync::Arc::new(map.into_iter().collect());
                view! {
                    <div class="json-object">
                        <span class="json-toggle" on:click=move |_| set_local_collapsed.update(|v| *v = !*v)>
                            {move || if is_collapsed() { "▶" } else { "▼" }}
                            {format!("{} {{ /* {} keys */", indent, len)}
                        </span>
                        <Show when=move || !is_collapsed() fallback=move || view! { <span>" }"</span> }>
                            <div class="json-children">
                                {entries.iter().enumerate().map(|(i, (k, child))| {
                                    let child_path = format!("{}.{}", path, k);
                                    let child = child.clone();
                                    let k = k.clone();
                                    let child_path_for_hover = child_path.clone();
                                    view! {
                                        <div class="json-object-field">
                                            <span class="json-key" on:mouseenter=move |_| on_hover.set(child_path_for_hover.clone())>
                                                {format!("{}  \"{}\": ", indent, k)}
                                            </span>
                                            <JsonValueView
                                                value=child
                                                path=child_path
                                                depth=depth + 1
                                                force_collapsed=force_collapsed
                                                on_hover=on_hover
                                            />
                                            {if i < len - 1 { ", " } else { "" }}
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                            <span>{format!("{}}}", indent)}</span>
                        </Show>
                    </div>
                }.into_any()
            }
        }
    }
}

/// 复制到剪贴板
fn copy_to_clipboard(text: &str) {
    if let Some(window) = web_sys::window() {
        let clipboard = window.navigator().clipboard();
        let _ = clipboard.write_text(text);
    }
}

/// 触发浏览器下载 JSON 文件
fn download_json(text: &str) {
    if let Some(window) = web_sys::window() {
        let document = window.document().unwrap();
        let arr = js_sys::Array::of1(&JsValue::from_str(text));
        let blob = web_sys::Blob::new_with_str_sequence(&arr).unwrap();

        let url = web_sys::Url::create_object_url_with_blob(&blob).unwrap();
        let a = document.create_element("a").unwrap();
        a.set_attribute("href", &url).unwrap();
        a.set_attribute("download", "data.json").unwrap();
        document.body().unwrap().append_child(&a).unwrap();
        a.clone().dyn_into::<HtmlElement>().unwrap().click();
        document.body().unwrap().remove_child(&a).unwrap();
        web_sys::Url::revoke_object_url(&url).unwrap();
    }
}
