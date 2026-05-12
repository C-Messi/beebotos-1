//! Markdown 渲染组件
//!
//! 使用 pulldown-cmark 将 Markdown 文本解析为 HTML，并通过 inner_html 渲染。
//! 支持代码块、表格、列表、标题等标准 Markdown 语法。
//! 渲染后通过 DOM 后处理为代码块添加复制按钮。

use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement};

/// Markdown 渲染视图
#[component]
pub fn MarkdownView(
    /// Markdown 原始文本
    raw: String,
) -> impl IntoView {
    let container_ref = NodeRef::<leptos::html::Div>::new();

    // 将 Markdown 解析为 HTML
    let html_content = Signal::derive(move || {
        let parser = pulldown_cmark::Parser::new(&raw);
        let mut html_output = String::new();
        pulldown_cmark::html::push_html(&mut html_output, parser);
        html_output
    });

    // DOM 后处理：为代码块添加复制按钮和语言标签
    Effect::new(move |_| {
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

/// 增强代码块：添加语言标签和复制按钮
fn enhance_code_blocks(container: &HtmlElement) {
    let pre_elements = container.get_elements_by_tag_name("pre");

    for i in 0..pre_elements.length() {
        if let Some(pre) = pre_elements.item(i) {
            let pre_elem: HtmlElement = match pre.dyn_into() {
                Ok(el) => el,
                Err(_) => continue,
            };

            // 跳过已处理的代码块
            if pre_elem
                .parent_element()
                .map(|p| p.class_list().contains("code-block-wrapper"))
                .unwrap_or(false)
            {
                continue;
            }

            // 提取语言标签（从 <code class="... language-xxx ...">）
            let lang = pre_elem
                .query_selector("code[class*=\"language-\"]")
                .ok()
                .flatten()
                .and_then(|code| code.get_attribute("class"))
                .and_then(|class| {
                    class
                        .split_whitespace()
                        .find(|c| c.starts_with("language-"))
                        .and_then(|c| c.split_once("language-").map(|(_, l)| l.to_string()))
                })
                .unwrap_or_else(|| "text".to_string());

            // 创建工具栏
            let window = match web_sys::window() {
                Some(w) => w,
                None => continue,
            };
            let document = match window.document() {
                Some(d) => d,
                None => continue,
            };

            let toolbar = match document.create_element("div") {
                Ok(el) => el,
                Err(_) => continue,
            };
            toolbar.set_class_name("code-toolbar");

            // 语言标签
            let lang_badge = match document.create_element("span") {
                Ok(el) => el,
                Err(_) => continue,
            };
            lang_badge.set_class_name("code-lang-badge");
            lang_badge.set_text_content(Some(&lang));
            let _ = toolbar.append_child(&lang_badge);

            // 复制按钮
            let copy_btn = create_copy_button(&pre_elem, &document);
            let _ = toolbar.append_child(&copy_btn);

            // 包装 pre 元素
            let wrapper = match document.create_element("div") {
                Ok(el) => el,
                Err(_) => continue,
            };
            wrapper.set_class_name("code-block-wrapper");

            if let Some(parent) = pre_elem.parent_node() {
                let _ = parent.replace_child(&wrapper, &pre_elem);
                let _ = wrapper.append_child(&toolbar);
                let _ = wrapper.append_child(&pre_elem);
            }
        }
    }
}

/// 创建复制按钮元素
fn create_copy_button(pre: &HtmlElement, document: &web_sys::Document) -> Element {
    let btn = document.create_element("button").unwrap();
    btn.set_class_name("copy-btn");
    btn.set_text_content(Some("📋 复制"));
    btn.set_attribute("title", "复制代码").unwrap();

    let pre_clone = pre.clone();
    let btn_clone = btn.clone();

    let onclick = Closure::wrap(Box::new(move |_e: web_sys::MouseEvent| {
        let code_text = pre_clone.text_content().unwrap_or_default();

        if let Some(window) = web_sys::window() {
            let clipboard = window.navigator().clipboard();
            let _ = clipboard.write_text(&code_text);
        }

        // 视觉反馈
        btn_clone.set_text_content(Some("✅ 已复制"));
        let btn_for_reset = btn_clone.clone();
        let _ = gloo_timers::callback::Timeout::new(2000, move || {
            btn_for_reset.set_text_content(Some("📋 复制"));
        });
    }) as Box<dyn FnMut(_)>);

    btn.add_event_listener_with_callback("click", onclick.as_ref().unchecked_ref())
        .unwrap();
    onclick.forget();

    btn
}
