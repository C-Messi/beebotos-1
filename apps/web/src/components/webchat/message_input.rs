//! 消息输入组件

use leptos::prelude::*;

use crate::utils::event_target_value;

/// 消息输入组件
#[component]
pub fn MessageInput(
    #[prop(optional)] placeholder: Option<String>,
    #[prop(default = Signal::derive(|| false), into)] disabled: Signal<bool>,
    #[prop(default = Signal::derive(|| false), into)] is_generating: Signal<bool>,
    #[prop(optional)] on_submit: Option<Box<dyn Fn(String)>>,
    #[prop(optional)] on_stop: Option<Box<dyn Fn()>>,
    #[prop(optional)] on_typing: Option<Box<dyn Fn(String)>>,
) -> impl IntoView {
    let placeholder = placeholder.unwrap_or_else(|| "Type a message...".to_string());
    let (value, set_value) = signal(String::new());

    let on_submit_callback = on_submit.map(|cb| {
        let cb = std::rc::Rc::new(cb);
        move |s: String| cb(s)
    });
    let on_typing_callback = on_typing.map(|cb| {
        let cb = std::rc::Rc::new(cb);
        move |s: String| cb(s)
    });
    let on_stop_callback = on_stop.map(|cb| {
        let cb = std::rc::Rc::new(cb);
        move || cb()
    });

    let on_keydown = {
        let on_submit = on_submit_callback.clone();
        move |ev: leptos::ev::KeyboardEvent| {
            if ev.key() == "Enter" && !ev.shift_key() {
                ev.prevent_default();
                if is_generating.get() {
                    return;
                }
                let content = value.get();
                if !content.trim().is_empty() {
                    if let Some(ref cb) = on_submit {
                        cb(content.clone());
                    }
                    set_value.set(String::new());
                }
            }
        }
    };

    let on_input = {
        let on_typing = on_typing_callback.clone();
        move |ev: leptos::ev::Event| {
            let content = event_target_value(&ev);
            set_value.set(content.clone());
            if let Some(ref cb) = on_typing {
                cb(content);
            }
        }
    };

    let on_click_submit = {
        let on_submit = on_submit_callback;
        let on_stop = on_stop_callback;
        move |_| {
            if is_generating.get() {
                if let Some(ref cb) = on_stop {
                    cb();
                }
                return;
            }

            let content = value.get();
            if !content.trim().is_empty() {
                if let Some(ref cb) = on_submit {
                    cb(content.clone());
                }
                set_value.set(String::new());
            }
        }
    };

    view! {
        <div class="message-input-container">
            <div class="message-input-wrapper">
                <button class="btn btn-icon attachment-btn" disabled=disabled>
                    "📎"
                </button>

                <textarea
                    class="message-textarea"
                    placeholder=placeholder
                    disabled=move || disabled.get()
                    prop:value=value
                    on:input=on_input
                    on:keydown=on_keydown
                    rows=1
                />

                <button
                    class="btn btn-primary send-btn"
                    disabled=move || disabled.get() || (!is_generating.get() && value.get().trim().is_empty())
                    on:click=on_click_submit
                >
                    {move || if is_generating.get() { "⏹" } else { "➤" }}
                </button>
            </div>

            <div class="input-hints">
                <span>"Press Enter to send, Shift+Enter for new line"</span>
                <span>"Use /btw for side question"</span>
            </div>
        </div>
    }
}
