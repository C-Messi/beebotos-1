use leptos::prelude::*;
use leptos::view;
use leptos_meta::Title;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct AiCommerceShop {
    pub name: &'static str,
    pub description: &'static str,
    pub icon: &'static str,
    pub tag: &'static str,
    pub url: Option<&'static str>,
}

const AI_COMMERCE_SHOPS: [AiCommerceShop; 3] = [
    AiCommerceShop {
        name: "抖音小店",
        description: "面向短视频与直播场景的小店入口",
        icon: "🎵",
        tag: "待接入",
        url: None,
    },
    AiCommerceShop {
        name: "微信小店",
        description: "面向微信生态私域经营的小店入口",
        icon: "💬",
        tag: "待接入",
        url: None,
    },
    AiCommerceShop {
        name: "云柑小店",
        description: "面向自有业务的小店入口",
        icon: "🍊",
        tag: "待接入",
        url: None,
    },
];

pub fn ai_commerce_shops() -> &'static [AiCommerceShop] {
    &AI_COMMERCE_SHOPS
}

#[component]
pub fn AiCommercePage() -> impl IntoView {
    view! {
        <Title text="ai电商 - BeeBotOS" />
        <div class="page ai-commerce-page">
            <div class="page-header">
                <div>
                    <h2>"ai电商"</h2>
                    <p>"集中管理外部小店入口，后续接入链接后一键进入。"</p>
                </div>
            </div>

            <div class="ai-commerce-grid">
                {ai_commerce_shops()
                    .iter()
                    .copied()
                    .map(|shop| view! { <ShopCard shop=shop /> })
                    .collect_view()}
            </div>
        </div>
    }
}

#[component]
fn ShopCard(shop: AiCommerceShop) -> impl IntoView {
    let action = match shop.url {
        Some(url) => view! {
            <a class="btn btn-primary btn-block" href=url target="_blank" rel="noopener noreferrer">
                "进入小店"
            </a>
        }
        .into_any(),
        None => view! {
            <button class="btn btn-secondary btn-block" disabled>
                "即将接入"
            </button>
        }
        .into_any(),
    };

    view! {
        <section class="ai-commerce-card">
            <div class="ai-commerce-card-head">
                <div class="ai-commerce-icon">{shop.icon}</div>
                <span class="status-badge status-pending">{shop.tag}</span>
            </div>
            <div class="ai-commerce-card-body">
                <h3>{shop.name}</h3>
                <p>{shop.description}</p>
            </div>
            <div class="ai-commerce-card-action">
                {action}
            </div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_commerce_shops_match_initial_scope() {
        let shops: Vec<_> = ai_commerce_shops().iter().map(|shop| shop.name).collect();

        assert_eq!(shops, vec!["抖音小店", "微信小店", "云柑小店"]);
    }
}
