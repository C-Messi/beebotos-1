//! 内容分发器
//!
//! 根据消息内容自动识别类型并选择最佳渲染方式：
//! - 纯文本：直接渲染
//! - Markdown：使用 pulldown-cmark 解析并渲染为 HTML
//! - JSON：使用 JsonTreeView 递归树形渲染
//! - 代码块：作为 Markdown 的一部分渲染，附带复制按钮

use leptos::prelude::*;

use crate::components::webchat::{JsonTreeView, MarkdownView};
use crate::webchat::Attachment;

/// 内容类型自动识别结果
#[derive(Clone, Debug, PartialEq)]
enum ContentType {
    /// 纯文本
    PlainText,
    /// Markdown 文本（包含 Markdown 语法标记）
    Markdown,
    /// 完整 JSON 对象或数组
    Json,
}

/// 智能内容分发器：自动识别内容类型并选择渲染器
#[component]
pub fn ContentRenderer(
    /// 消息内容文本
    content: String,
    /// 附件列表
    #[prop(optional)]
    attachments: Vec<Attachment>,
) -> impl IntoView {
    let content_type = detect_content_type(&content);

    view! {
        <div class="content-renderer">
            {match content_type {
                ContentType::Json => {
                    match serde_json::from_str::<serde_json::Value>(&content) {
                        Ok(value) => view! {
                            <JsonTreeView raw=content.clone() value=value />
                        }.into_any(),
                        Err(_) => view! {
                            <PlainTextView text=content.clone() />
                        }.into_any(),
                    }
                }
                ContentType::Markdown => view! {
                    <MarkdownView raw=content.clone() />
                }.into_any(),
                ContentType::PlainText => view! {
                    <PlainTextView text=content.clone() />
                }.into_any(),
            }}

            // 渲染附件（图片优先）
            {if !attachments.is_empty() {
                view! {
                    <div class="message-attachments">
                        {attachments.into_iter().map(|att| {
                            view! { <AttachmentView attachment=att /> }
                        }).collect_view()}
                    </div>
                }.into_any()
            } else {
                view! { <div /> }.into_any()
            }}
        </div>
    }
}

/// 纯文本视图（保留换行和缩进，安全渲染）
#[component]
fn PlainTextView(text: String) -> impl IntoView {
    view! {
        <div class="plain-text-view">{text}</div>
    }
}

/// 附件视图
#[component]
fn AttachmentView(attachment: Attachment) -> impl IntoView {
    if attachment.is_image {
        let src = attachment
            .url
            .clone()
            .or(attachment.thumbnail_url.clone())
            .unwrap_or_default();

        view! {
            <div class="attachment image-attachment">
                <img src=src alt=attachment.file_name.clone() loading="lazy" />
                <span class="attachment-name">{attachment.file_name}</span>
            </div>
        }
        .into_any()
    } else {
        let href = attachment.url.clone().unwrap_or_default();
        view! {
            <a class="attachment file-attachment" href=href target="_blank">
                <span class="file-icon">"📎"</span>
                <span class="file-name">{attachment.file_name}</span>
                <span class="file-size">{format_file_size(attachment.file_size)}</span>
            </a>
        }
        .into_any()
    }
}

/// 检测内容类型
fn detect_content_type(content: &str) -> ContentType {
    let trimmed = content.trim();

    // 策略 1：检测完整 JSON（以 { 或 [ 开头且能成功解析）
    if (trimmed.starts_with('{') || trimmed.starts_with('['))
        && serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
    {
        return ContentType::Json;
    }

    // 策略 2：检测 Markdown 语法标记
    if has_markdown_syntax(content) {
        return ContentType::Markdown;
    }

    ContentType::PlainText
}

/// 判断文本是否包含 Markdown 语法
fn has_markdown_syntax(text: &str) -> bool {
    // 常见的 Markdown 标记（使用前缀匹配减少误报）
    let line_markers = [
        "```", // 代码块
        "# ",  // 标题
        "## ", "### ", "#### ", "##### ", "###### ", "- ", // 无序列表
        "* ", "> ",  // 引用
        "---", // 分隔线
        "***",
    ];

    let inline_markers = [
        "**", // 粗体
        "__", "`",  // 行内代码
        "![", // 图片
        "[",  // 链接（需后续跟随 ](）
    ];

    // 行级标记：任意一行以标记开头
    let has_line_marker = text.lines().any(|line| {
        let trimmed = line.trim_start();
        line_markers.iter().any(|m| trimmed.starts_with(m))
    });

    // 行内标记
    let has_inline_marker = inline_markers.iter().any(|m| text.contains(m));

    // 表格：至少包含一个 `|...|` 模式
    let has_table = text.lines().any(|line| {
        line.contains('|') && line.trim().starts_with('|') && line.trim().ends_with('|')
    });

    // 有序列表：行首匹配 "N. "
    let has_ordered_list = text.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.len() >= 3
            && trimmed.as_bytes()[0].is_ascii_digit()
            && trimmed[1..].starts_with(". ")
    });

    has_line_marker || has_inline_marker || has_table || has_ordered_list
}

/// 格式化文件大小
fn format_file_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = size as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.1} {}", size, UNITS[unit_idx])
}
