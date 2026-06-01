//! Internationalization (i18n) module for BeeBotOS Web
//!
//! Provides multi-language support with Chinese (zh-CN) as default

use std::collections::HashMap;

use leptos::prelude::*;

/// Supported locales
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Locale {
    ZhCN,
    En,
}

impl Default for Locale {
    fn default() -> Self {
        Locale::ZhCN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_store_manager_has_navigation_labels() {
        let i18n = init_i18n();

        i18n.set_locale(Locale::ZhCN);
        assert_eq!(i18n.t("nav-ai-store-manager"), "AI 店长");

        i18n.set_locale(Locale::En);
        assert_eq!(i18n.t("nav-ai-store-manager"), "AI Store Manager");
    }
}

impl Locale {
    pub fn as_str(&self) -> &'static str {
        match self {
            Locale::ZhCN => "zh-CN",
            Locale::En => "en",
        }
    }
}

/// I18n context holding current locale and translations
#[derive(Clone)]
pub struct I18nContext {
    locale: RwSignal<Locale>,
    translations: HashMap<&'static str, HashMap<&'static str, &'static str>>,
}

impl I18nContext {
    /// Get current locale
    pub fn get_locale(&self) -> Locale {
        self.locale.get()
    }

    /// Set locale
    pub fn set_locale(&self, locale: Locale) {
        self.locale.set(locale);
    }

    /// Translate a key
    pub fn t(&self, key: &str) -> String {
        let locale_str = self.get_locale().as_str();
        self.translations
            .get(locale_str)
            .and_then(|t| t.get(key))
            .copied()
            .unwrap_or(key)
            .to_string()
    }
}

/// Initialize i18n context
pub fn init_i18n() -> I18nContext {
    let mut translations: HashMap<&'static str, HashMap<&'static str, &'static str>> =
        HashMap::new();

    // Chinese translations
    let mut zh = HashMap::new();
    zh.insert("app-title", "BeeBotOS - Web4.0 自主智能体操作系统");
    zh.insert("app-description", "自主 AI 智能体的操作系统");
    zh.insert("nav-home", "首页");
    zh.insert("nav-agents", "智能体");
    zh.insert("nav-dao", "DAO 治理");
    zh.insert("nav-treasury", "金库");
    zh.insert("nav-skills", "技能市场");
    zh.insert("nav-mcp-server", "MCP 服务器");
    zh.insert("nav-ai-commerce", "ai电商");
    zh.insert("nav-ai-store-manager", "AI 店长");
    zh.insert("nav-skill-instances", "实例管理");
    zh.insert("nav-workflows", "工作流");
    zh.insert("nav-cron-jobs", "定时任务");
    zh.insert("nav-llm-settings", "大模型");
    zh.insert("nav-channels", "频道管理");
    zh.insert("nav-settings", "设置");
    zh.insert("nav-chat", "聊天");
    zh.insert("nav-browser", "浏览器");
    zh.insert("action-get-started", "开始使用");
    zh.insert("action-browse-skills", "浏览技能");
    zh.insert("action-create", "创建");
    zh.insert("action-view", "查看");
    zh.insert("action-browse", "浏览");
    zh.insert("action-save", "保存");
    zh.insert("action-cancel", "取消");
    zh.insert("action-delete", "删除");
    zh.insert("action-edit", "编辑");
    zh.insert("action-submit", "提交");
    zh.insert("action-refresh", "刷新");
    zh.insert("action-loading", "加载中...");
    zh.insert("action-back", "返回");
    zh.insert("action-close", "关闭");
    zh.insert("action-search", "搜索");
    zh.insert("action-filter", "筛选");
    zh.insert("action-install", "安装");
    zh.insert("action-uninstall", "卸载");
    zh.insert("action-enable", "启用");
    zh.insert("action-disable", "禁用");
    zh.insert("action-login", "登录");
    zh.insert("action-logout", "退出登录");
    zh.insert("action-register", "注册");
    // Login page
    zh.insert("login-title", "欢迎回来");
    zh.insert("login-subtitle", "登录到您的 BeeBotOS 账户");
    zh.insert("login-username", "用户名");
    zh.insert("login-username-placeholder", "请输入用户名");
    zh.insert("login-password", "密码");
    zh.insert("login-password-placeholder", "请输入密码");
    zh.insert("login-error-empty", "用户名和密码不能为空");
    zh.insert("login-error-failed", "登录失败");
    zh.insert("login-or", "或");
    zh.insert("login-demo-button", "演示登录");
    zh.insert("login-no-account", "还没有账户？");
    zh.insert("login-register-link", "立即注册");
    // Register page
    zh.insert("register-title", "创建账户");
    zh.insert("register-subtitle", "注册 BeeBotOS 账户开始使用");
    zh.insert("register-username", "用户名");
    zh.insert("register-username-placeholder", "请输入用户名");
    zh.insert("register-email", "邮箱");
    zh.insert("register-email-placeholder", "请输入邮箱（可选）");
    zh.insert("register-password", "密码");
    zh.insert("register-password-placeholder", "请输入密码（至少6位）");
    zh.insert("register-confirm-password", "确认密码");
    zh.insert("register-confirm-password-placeholder", "请再次输入密码");
    zh.insert("register-error-empty", "用户名和密码不能为空");
    zh.insert("register-error-password-mismatch", "两次输入的密码不一致");
    zh.insert("register-error-password-short", "密码长度至少6位");
    zh.insert("register-error-failed", "注册失败");
    zh.insert("register-or", "或");
    zh.insert("register-demo-button", "演示注册");
    zh.insert("register-have-account", "已有账户？");
    zh.insert("register-login-link", "立即登录");
    zh.insert("hero-title", "自主 AI 智能体的操作系统");
    zh.insert(
        "hero-subtitle",
        "构建、部署和管理具备内置治理功能的智能代理",
    );
    zh.insert("hero-cta-primary", "开始使用");
    zh.insert("hero-cta-secondary", "浏览技能");
    zh.insert("features-title", "核心功能");
    zh.insert("feature-agents-title", "自主智能体");
    zh.insert(
        "feature-agents-desc",
        "部署具备内置安全控制的独立运行 AI 智能体",
    );
    zh.insert("feature-dao-title", "DAO 治理");
    zh.insert("feature-dao-desc", "通过透明投票机制实现社区驱动决策");
    zh.insert("feature-treasury-title", "安全金库");
    zh.insert("feature-treasury-desc", "多签金库管理，链上透明可追溯");
    zh.insert("feature-skills-title", "技能市场");
    zh.insert("feature-skills-desc", "通过社区构建的技能扩展智能体能力");
    zh.insert("feature-wasm-title", "WebAssembly 运行时");
    zh.insert("feature-wasm-desc", "高性能、沙盒化执行环境");
    zh.insert("feature-analytics-title", "实时分析");
    zh.insert("feature-analytics-desc", "实时监控智能体性能和系统健康状况");
    zh.insert("quick-actions-title", "快速操作");
    zh.insert("quick-action-create-agent-title", "创建智能体");
    zh.insert("quick-action-create-agent-desc", "设置新的自主智能体");
    zh.insert("quick-action-view-proposals-title", "查看提案");
    zh.insert("quick-action-view-proposals-desc", "参与 DAO 治理投票");
    zh.insert("quick-action-install-skills-title", "安装技能");
    zh.insert("quick-action-install-skills-desc", "为智能体添加新能力");
    zh.insert("agents-title", "智能体管理");
    zh.insert("agents-subtitle", "管理您的自主 AI 智能体");
    zh.insert("agents-create-new", "创建新智能体");
    zh.insert("agents-no-agents", "暂无智能体");
    zh.insert("agents-loading", "加载中...");
    zh.insert("agents-error", "加载失败");
    zh.insert("status-active", "运行中");
    zh.insert("status-idle", "空闲");
    zh.insert("status-paused", "已暂停");
    zh.insert("status-error", "错误");
    zh.insert("status-offline", "离线");
    zh.insert("status-running", "运行中");
    zh.insert("status-completed", "已完成");
    zh.insert("status-pending", "待处理");
    // Channels
    zh.insert("channels-title", "频道管理");
    zh.insert("channels-subtitle", "配置和管理各消息频道的连接");
    zh.insert("channel-status", "频道状态");
    zh.insert("channel-config", "频道配置");
    zh.insert("status-enabled", "已启用");
    zh.insert("status-disabled", "未启用");
    zh.insert("wechat-login", "微信登录");
    zh.insert(
        "wechat-login-hint",
        "使用微信扫描二维码登录，获取 Bot Token",
    );
    zh.insert("qr-expires-in", "二维码过期时间");
    zh.insert("action-get-qr", "获取二维码");
    zh.insert("action-refresh-qr", "刷新二维码");
    zh.insert("action-test", "测试连接");
    zh.insert("config-base-url", "基础 URL");
    zh.insert("config-bot-token", "Bot 令牌");
    zh.insert("config-auto-reconnect", "自动重连");

    zh.insert("dao-title", "DAO 治理");
    zh.insert("dao-subtitle", "参与社区决策");
    zh.insert("dao-active-proposals", "活跃提案");
    zh.insert("dao-completed-proposals", "已完成提案");
    zh.insert("dao-create-proposal", "创建提案");
    zh.insert("dao-vote-for", "赞成");
    zh.insert("dao-vote-against", "反对");
    zh.insert("dao-votes-for", "赞成票");
    zh.insert("dao-votes-against", "反对票");
    zh.insert("dao-voting-ends", "投票截止");
    zh.insert("dao-executed", "已执行");
    zh.insert("treasury-title", "金库管理");
    zh.insert("treasury-subtitle", "管理 DAO 资产和交易");
    zh.insert("treasury-total-balance", "总资产");
    zh.insert("treasury-assets", "资产列表");
    zh.insert("treasury-transactions", "交易记录");
    zh.insert("treasury-deposit", "存入");
    zh.insert("treasury-withdraw", "提取");
    zh.insert("skills-title", "技能市场");
    zh.insert("skills-subtitle", "发现和安装智能体能力");
    zh.insert("skills-categories", "分类");
    zh.insert("skills-installed", "已安装");
    zh.insert("skills-available", "可用");
    zh.insert("skills-search-placeholder", "搜索技能...");
    zh.insert("skills-tab-all", "全部");
    zh.insert("skills-tab-local", "本地");
    zh.insert("skills-tab-clawhub", "ClawHub");
    zh.insert("skills-tab-beehub", "BeeHub");
    zh.insert("skills-category-all", "全部");
    zh.insert("skills-category-trading", "交易");
    zh.insert("skills-category-data", "数据");
    zh.insert("skills-category-social", "社交");
    zh.insert("skills-category-automation", "自动化");
    zh.insert("skills-category-analysis", "分析");
    zh.insert("skills-search", "搜索");
    zh.insert("skill-installed", "已安装");
    zh.insert("skill-details", "详情");
    zh.insert("skill-uninstall", "卸载");
    zh.insert("skill-install", "安装");
    zh.insert("skill-view-on-hub", "在Hub上查看");
    zh.insert("skill-version", "版本");
    zh.insert("skill-author", "作者");
    zh.insert("skill-license", "许可证");
    zh.insert("skill-downloads", "下载");
    zh.insert("skill-rating", "评分");
    zh.insert("skill-description", "描述");
    zh.insert("skill-capabilities", "能力");
    zh.insert("skill-tags", "标签");
    zh.insert("skill-none-listed", "未列出");
    zh.insert("skill-removing", "卸载中...");
    zh.insert("skill-installing", "安装中...");
    zh.insert("skills-no-results", "未找到技能");
    zh.insert("skills-search-hint", "输入关键词搜索技能");
    zh.insert("skills-try-different", "尝试不同的搜索词或切换到本地技能");
    zh.insert("skills-failed-load", "加载技能失败");
    zh.insert("skills-hub-unreachable", "技能中心当前无法访问");
    zh.insert("skills-check-network", "请切换到本地技能或检查网关网络配置");
    zh.insert("skills-retry", "重试");
    zh.insert("skills-no-skills-found", "暂无技能");
    zh.insert("skills-adjust-search", "尝试调整搜索或筛选条件");
    zh.insert("settings-title", "系统设置");
    zh.insert("settings-subtitle", "配置您的 BeeBotOS 实例");
    zh.insert("settings-general", "常规设置");
    zh.insert("settings-appearance", "外观设置");
    zh.insert("settings-language", "语言");
    zh.insert("settings-theme", "主题");
    zh.insert("theme-light", "浅色");
    zh.insert("theme-dark", "深色");
    zh.insert("theme-system", "跟随系统");
    zh.insert("settings-notifications", "通知设置");
    zh.insert("settings-security", "安全设置");
    zh.insert("settings-wallet", "钱包设置");
    zh.insert("settings-system", "系统信息");
    zh.insert("footer-copyright", "© 2026 BeeBotOS. 保留所有权利。");
    zh.insert("footer-version", "版本");
    zh.insert("error-404-title", "404");
    zh.insert("error-404-message", "页面未找到");
    zh.insert("error-404-description", "您访问的页面不存在或已被移动。");
    zh.insert("error-go-home", "返回首页");
    zh.insert("error-generic", "出错了");
    zh.insert("error-retry", "重试");
    zh.insert("notification-success", "成功");
    zh.insert("notification-error", "错误");
    zh.insert("notification-warning", "警告");
    zh.insert("notification-info", "信息");
    zh.insert("logout-success", "您已成功退出登录");
    zh.insert("stats-tasks", "完成任务");
    zh.insert("stats-uptime", "系统正常运行时间");
    zh.insert("stats-members", "社区成员");
    zh.insert("nav-home", "首页");
    zh.insert("quick-action-start-chat-title", "开始聊天");
    zh.insert("quick-action-start-chat-desc", "与AI智能体开始对话");
    zh.insert("nav-section-chat", "聊天");
    zh.insert("nav-section-control", "控制");
    zh.insert("nav-section-agents", "智能体");
    zh.insert("nav-section-settings", "设置");
    zh.insert("footer-product", "产品");
    zh.insert("footer-resources", "资源");
    zh.insert("footer-community", "社区");
    zh.insert("ai-commerce-title", "ai电商");
    zh.insert(
        "ai-commerce-subtitle",
        "集中管理外部小店入口，后续接入链接后一键进入。",
    );
    zh.insert("ai-commerce-enter-store", "进入小店");
    zh.insert("ai-commerce-coming-soon", "即将接入");
    zh.insert("ai-commerce-status-pending", "待接入");
    zh.insert("ai-store-manager-title", "AI 店长");
    zh.insert(
        "ai-store-manager-subtitle",
        "用 AI 批量生成视频、图文和电话营销任务。",
    );
    zh.insert("ai-store-manager-import-products", "导入商品");
    zh.insert("ai-store-manager-create-task", "创建营销任务");
    zh.insert("ai-store-manager-marketing-entries", "营销入口");
    zh.insert("ai-store-manager-todo", "营销待办");
    zh.insert("ai-store-manager-video-marketing", "AI 视频营销");
    zh.insert(
        "ai-store-manager-video-desc",
        "按商品、卖点和平台生成短视频脚本、分镜、标题、口播词和字幕。",
    );
    zh.insert("ai-store-manager-video-core", "核心能力");
    zh.insert("ai-store-manager-create-video", "创建视频任务");
    zh.insert("ai-store-manager-graphic-marketing", "AI 图文营销");
    zh.insert(
        "ai-store-manager-graphic-desc",
        "生成种草文案、朋友圈内容、海报文案和商品详情优化建议。",
    );
    zh.insert("ai-store-manager-graphic-core", "核心能力");
    zh.insert("ai-store-manager-create-graphic", "创建图文任务");
    zh.insert("ai-store-manager-phone-marketing", "AI 电话营销");
    zh.insert(
        "ai-store-manager-phone-desc",
        "面向老客复购、活动通知和高意向线索生成外呼话术与跟进任务。",
    );
    zh.insert("ai-store-manager-phone-core", "核心能力");
    zh.insert("ai-store-manager-create-phone", "创建外呼任务");
    zh.insert("skill-instances-title", "实例管理");
    zh.insert("skill-instances-subtitle", "管理绑定到智能体的技能实例");
    zh.insert("skill-instances-new", "新建实例");
    zh.insert("skill-instances-cancel", "取消");
    zh.insert("skill-instances-create", "创建实例");
    zh.insert("skill-instances-creating", "创建中...");
    zh.insert("skill-instances-skill-id", "技能 ID");
    zh.insert("skill-instances-skill-id-placeholder", "例如：echo-skill");
    zh.insert("skill-instances-agent-id", "智能体 ID");
    zh.insert("skill-instances-agent-id-placeholder", "例如：agent-001");
    zh.insert("skill-instances-table-id", "实例 ID");
    zh.insert("skill-instances-table-skill", "技能");
    zh.insert("skill-instances-table-agent", "智能体");
    zh.insert("skill-instances-table-status", "状态");
    zh.insert("skill-instances-table-usage", "使用量");
    zh.insert("skill-instances-table-actions", "操作");
    zh.insert("skill-instances-run", "运行");
    zh.insert("skill-instances-running", "运行中...");
    zh.insert("skill-instances-delete", "删除");
    zh.insert("skill-instances-empty-title", "暂无实例");
    zh.insert(
        "skill-instances-empty-desc",
        "创建一个新实例以绑定技能到智能体",
    );
    zh.insert("skill-instances-error-title", "加载实例失败");
    zh.insert("skill-instances-missing-fields", "缺少字段");
    zh.insert("skill-instances-fill-fields", "请填写技能 ID 和智能体 ID");
    zh.insert("skill-instances-created", "实例已创建");
    zh.insert("skill-instances-creation-failed", "创建失败");
    zh.insert("skill-instances-deleted", "实例已删除");
    zh.insert("skill-instances-delete-failed", "删除失败");
    zh.insert("skill-instances-execution-result", "执行结果");
    zh.insert("skill-instances-execution-failed", "执行失败");
    zh.insert("workflows-title", "工作流");
    zh.insert("workflows-subtitle", "监控工作流定义、执行和技能组合");
    zh.insert("workflows-refresh", "刷新");
    zh.insert("workflows-refreshing", "刷新中...");
    zh.insert("workflows-tab-orchestration", "📋 工作流编排");
    zh.insert("workflows-tab-composition", "🔗 技能组合");
    zh.insert("workflows-recent-instances", "最近实例");
    zh.insert("workflows-definitions", "工作流定义");
    zh.insert("workflows-add", "添加工作流");
    zh.insert("workflows-skill-compositions", "技能组合");
    zh.insert("workflows-stat-total", "工作流总数");
    zh.insert("workflows-stat-instances", "实例总数");
    zh.insert("workflows-stat-completed", "已完成");
    zh.insert("workflows-stat-failed", "失败");
    zh.insert("workflows-stat-running", "运行中");
    zh.insert("workflows-stat-pending", "待处理");
    zh.insert("workflows-no-instances", "暂无工作流实例");
    zh.insert("workflows-table-workflow", "工作流");
    zh.insert("workflows-table-status", "状态");
    zh.insert("workflows-table-progress", "进度");
    zh.insert("workflows-table-duration", "持续时间");
    zh.insert("workflows-table-started", "开始时间");
    zh.insert("workflows-no-definitions", "尚未定义工作流");
    zh.insert("workflows-no-triggers", "无触发器");
    zh.insert("workflows-manual-execute", "手动执行");
    zh.insert("workflows-start", "开始");
    zh.insert("workflows-starting", "启动中...");
    zh.insert("workflows-stop", "停止");
    zh.insert("workflows-stopping", "停止中...");
    zh.insert("workflows-stop-latest", "停止最新运行实例");
    zh.insert("workflows-no-running", "无运行实例");
    zh.insert("workflows-not-running", "此工作流当前未在运行");
    zh.insert("workflows-schedule", "调度");
    zh.insert("workflows-config", "配置");
    zh.insert("workflows-uninstall", "卸载");
    zh.insert("workflows-uninstalling", "卸载中...");
    zh.insert("workflows-uninstalled", "工作流已卸载");
    zh.insert("workflows-uninstall-failed", "卸载失败");
    zh.insert("workflows-dag", "DAG");
    zh.insert("workflows-reports", "报告");
    zh.insert(
        "workflows-no-reports",
        "暂无报告。执行完成后可在这里查看最新报告和历史报告。",
    );
    zh.insert("workflows-select-report", "请选择一个报告");
    zh.insert(
        "workflows-start-hint",
        "点击执行会使用默认空上下文启动工作流；只有需要覆盖参数时才展开高级 JSON。",
    );
    zh.insert("workflows-show-context", "高级参数 JSON");
    zh.insert("workflows-hide-context", "收起高级参数");
    zh.insert("workflows-trigger-context", "触发上下文 (可选 JSON)");
    zh.insert(
        "workflows-trigger-hint",
        "默认 {} 即可；可输入 JSON 对象作为触发上下文。",
    );
    zh.insert("workflows-execute", "执行");
    zh.insert("workflows-executing", "执行中...");
    zh.insert("workflows-started", "工作流已启动");
    zh.insert("workflows-execution-failed", "执行失败");
    zh.insert("workflows-invalid-json", "无效的 JSON");
    zh.insert("workflows-cancel", "取消");
    zh.insert("workflows-close", "关闭");
    zh.insert("workflows-cron-expression", "Cron 表达式");
    zh.insert("workflows-timezone", "时区");
    zh.insert(
        "workflows-cron-placeholder",
        "例如：0 9 * * *（每天上午 9 点）, 0 */6 * * *（每 6 小时）",
    );
    zh.insert(
        "workflows-timezone-placeholder",
        "例如：UTC, Asia/Shanghai, America/New_York",
    );
    zh.insert("workflows-parse-failed", "解析失败");
    zh.insert("workflows-schedule-updated", "调度已更新");
    zh.insert("workflows-schedule-update-failed", "更新失败");
    zh.insert("workflows-fetch-failed", "获取失败");
    zh.insert("workflows-save-schedule", "保存调度");
    zh.insert("workflows-saving", "保存中...");
    zh.insert("workflows-install-title", "安装工作流");
    zh.insert("workflows-file-path", "工作流文件路径");
    zh.insert(
        "workflows-file-hint",
        "YAML/JSON 工作流文件的绝对或相对路径",
    );
    zh.insert("workflows-install", "安装");
    zh.insert("workflows-installing", "安装中...");
    zh.insert("workflows-installed", "工作流已安装");
    zh.insert("workflows-install-failed", "安装失败");
    zh.insert("workflows-load-failed", "加载失败");
    zh.insert("workflows-definition-yaml", "工作流定义 (YAML)");
    zh.insert(
        "workflows-definition-hint",
        "直接编辑工作流定义。注意 YAML 语法。",
    );
    zh.insert("workflows-updated", "工作流已更新");
    zh.insert("workflows-saved", "配置保存成功");
    zh.insert("workflows-update-failed", "更新失败");
    zh.insert("workflows-save", "保存");
    zh.insert("workflows-no-compositions", "尚未定义技能组合");
    zh.insert(
        "workflows-composition-hint",
        "通过 API 或在 data/compositions/ 中创建 YAML 文件来创建组合",
    );
    zh.insert("workflows-composition-executed", "组合已执行");
    zh.insert("workflows-composition-execution-failed", "组合执行失败");
    zh.insert("workflows-composition-delete", "删除组合");
    zh.insert("workflows-composition-deleting", "删除中...");
    zh.insert("workflows-composition-deleted", "组合已删除");
    zh.insert("workflows-composition-delete-failed", "删除失败");
    zh.insert("workflows-execute-composition", "执行组合");
    zh.insert("workflows-composition-running", "运行中...");
    zh.insert("workflows-error-recent", "加载最近实例失败");
    zh.insert("workflows-error-definitions", "加载工作流定义失败");
    zh.insert("workflows-error-compositions", "加载技能组合失败");
    zh.insert("cron-jobs-title", "定时任务");
    zh.insert("cron-jobs-subtitle", "管理定时执行的自动化任务");
    zh.insert("cron-jobs-refresh", "刷新");
    zh.insert("cron-jobs-refreshing", "刷新中...");
    zh.insert("cron-jobs-new", "新建任务");
    zh.insert("cron-jobs-edit", "编辑定时任务");
    zh.insert("cron-jobs-create", "新建定时任务");
    zh.insert("cron-jobs-history", "执行历史");
    zh.insert("cron-jobs-empty-title", "暂无定时任务");
    zh.insert(
        "cron-jobs-empty-hint",
        "点击右上角「新建任务」创建第一个定时任务",
    );
    zh.insert("cron-jobs-table-name", "名称");
    zh.insert("cron-jobs-table-schedule", "调度方式");
    zh.insert("cron-jobs-table-expression", "表达式");
    zh.insert("cron-jobs-table-status", "状态");
    zh.insert("cron-jobs-table-runs", "运行次数");
    zh.insert("cron-jobs-table-next", "下次执行");
    zh.insert("cron-jobs-table-actions", "操作");
    zh.insert("cron-jobs-type-timed", "定时");
    zh.insert("cron-jobs-type-interval", "间隔");
    zh.insert("cron-jobs-enable", "启用");
    zh.insert("cron-jobs-disable", "禁用");
    zh.insert("cron-jobs-enabled", "已启用");
    zh.insert("cron-jobs-disabled", "已禁用");
    zh.insert("cron-jobs-run", "运行");
    zh.insert("cron-jobs-edit-action", "编辑");
    zh.insert("cron-jobs-history-action", "历史");
    zh.insert("cron-jobs-delete", "删除");
    zh.insert("cron-jobs-form-name", "任务名称");
    zh.insert("cron-jobs-form-name-placeholder", "如：每日晨报");
    zh.insert("cron-jobs-form-desc", "描述");
    zh.insert("cron-jobs-form-desc-placeholder", "简要描述任务用途");
    zh.insert("cron-jobs-form-schedule-type", "调度方式");
    zh.insert("cron-jobs-form-expression", "调度表达式");
    zh.insert("cron-jobs-form-cron-placeholder", "*/5 * * * *");
    zh.insert("cron-jobs-form-interval-placeholder", "30m");
    zh.insert("cron-jobs-form-cron-hint", "5 字段 cron：分 时 日 月 星期");
    zh.insert(
        "cron-jobs-form-interval-hint",
        "支持 s/m/h/d，如 30m, 1h, 4h, 1d",
    );
    zh.insert("cron-jobs-form-timezone", "时区");
    zh.insert("cron-jobs-form-timezone-placeholder", "Asia/Shanghai");
    zh.insert("cron-jobs-form-prompt", "执行提示词 (Prompt)");
    zh.insert(
        "cron-jobs-form-prompt-placeholder",
        "任务触发时发送给 Agent 的提示词",
    );
    zh.insert("cron-jobs-form-context-mode", "上下文模式");
    zh.insert("cron-jobs-form-context-standalone", "独立会话");
    zh.insert("cron-jobs-form-context-shared", "主会话共享");
    zh.insert("cron-jobs-form-max-runs", "最大运行次数");
    zh.insert("cron-jobs-form-max-runs-placeholder", "留空表示无限制");
    zh.insert("cron-jobs-form-channel", "通知投递频道");
    zh.insert("cron-jobs-form-channel-webchat", "网页聊天");
    zh.insert("cron-jobs-form-channel-webhook", "Webhook");
    zh.insert("cron-jobs-form-target", "投递目标");
    zh.insert(
        "cron-jobs-form-target-webchat-hint",
        "WebSocket 频道名（默认 webchat）",
    );
    zh.insert("cron-jobs-form-target-webhook-hint", "接收 POST 请求的 URL");
    zh.insert("cron-jobs-form-enabled", "启用此任务");
    zh.insert("cron-jobs-form-cancel", "取消");
    zh.insert("cron-jobs-form-saving", "保存中...");
    zh.insert("cron-jobs-form-save", "保存");
    zh.insert("cron-jobs-form-create", "创建");
    zh.insert("cron-jobs-history-empty", "暂无执行记录");
    zh.insert("cron-jobs-history-loading", "加载中...");
    zh.insert("cron-jobs-history-error", "加载失败");
    zh.insert("cron-jobs-error-load", "加载定时任务列表失败");
    zh.insert("llm-settings-title", "大模型设置");
    zh.insert(
        "llm-settings-subtitle",
        "选择并配置当前使用的大语言模型及其参数",
    );
    zh.insert("llm-settings-retry", "重试");
    zh.insert("llm-settings-provider", "模型提供商");
    zh.insert("llm-settings-select-provider", "选择提供商");
    zh.insert("llm-settings-model-version", "模型版本");
    zh.insert("llm-settings-select-kimi", "选择 Kimi 模型");
    zh.insert(
        "llm-settings-kimi-hint",
        "选择会同步写入 model、thinking 和 temperature。",
    );
    zh.insert("llm-settings-select-deepseek", "选择 DeepSeek 模型");
    zh.insert(
        "llm-settings-deepseek-hint",
        "DeepSeek 官方当前模型为 deepseek-v4-flash / deepseek-v4-pro，选择会同步写入 thinking 和 \
         reasoning_effort。",
    );
    zh.insert("llm-settings-model-name", "模型名称");
    zh.insert("llm-settings-model-placeholder", "例如: gpt-4o");
    zh.insert("llm-settings-temperature", "温度");
    zh.insert(
        "llm-settings-temperature-hint",
        "取值范围 0.0 ~ 2.0，越低越确定，越高越 creative",
    );
    zh.insert("llm-settings-select-provider-first", "请先选择模型提供商");
    zh.insert("llm-settings-current-params", "当前参数");
    zh.insert("llm-settings-provider-label", "提供商");
    zh.insert("llm-settings-model-label", "模型");
    zh.insert("llm-settings-temperature-label", "温度");
    zh.insert("llm-settings-thinking-label", "思考");
    zh.insert("llm-settings-reasoning-label", "推理力度");
    zh.insert("llm-settings-actions", "操作");
    zh.insert("llm-settings-save-success", "保存成功");
    zh.insert("llm-settings-select-provider-model", "请选择提供商和模型");
    zh.insert("llm-settings-saving", "保存中...");
    zh.insert("llm-settings-save-config", "保存配置");
    zh.insert("llm-settings-reload-success", "配置已重载");
    zh.insert("llm-settings-reloading", "重载中...");
    zh.insert("llm-settings-reload-restart", "重启生效 (Reload)");
    zh.insert(
        "llm-settings-reload-hint",
        "保存后会自动写入 config/beebotos.toml 并热重载。如需完全生效，请点击",
    );
    zh.insert("agents-page-title", "智能体");
    zh.insert("agents-page-subtitle", "管理您的自主 AI 智能体");
    zh.insert("agents-new", "新建智能体");
    zh.insert("agents-tasks", "个任务");
    zh.insert("agents-manage", "管理");
    zh.insert("agents-stop", "停止");
    zh.insert("agents-stopping", "停止中...");
    zh.insert("agents-stopped", "智能体已停止");
    zh.insert("agents-stop-failed", "停止失败");
    zh.insert("agents-start", "启动");
    zh.insert("agents-starting", "启动中...");
    zh.insert("agents-started", "智能体已启动");
    zh.insert("agents-start-failed", "启动失败");
    zh.insert("agents-empty-title", "暂无智能体");
    zh.insert("agents-empty-desc", "创建您的第一个自主智能体以开始使用");
    zh.insert("agents-empty-create", "创建智能体");
    zh.insert("agents-error-title", "加载智能体失败");
    zh.insert("agents-retry", "重试");
    zh.insert("agents-refresh", "刷新页面");
    zh.insert("agents-create-title", "创建新智能体");
    zh.insert("agents-create-name", "智能体名称 *");
    zh.insert("agents-create-name-placeholder", "输入智能体名称");
    zh.insert("agents-create-name-error", "名称不能为空");
    zh.insert("agents-create-desc", "描述");
    zh.insert("agents-create-desc-placeholder", "输入智能体描述");
    zh.insert("agents-create-provider", "模型提供商");
    zh.insert("agents-create-model", "模型名称");
    zh.insert(
        "agents-create-model-placeholder",
        "例如：gpt-4, claude-3-opus-20240229",
    );
    zh.insert("agents-create-cancel", "取消");
    zh.insert("agents-create-creating", "创建中...");
    zh.insert("agents-create-submit", "创建智能体");
    zh.insert("settings-page-title", "设置");
    zh.insert("settings-page-subtitle", "管理您的偏好和系统配置");
    zh.insert("settings-loading", "加载设置中...");
    zh.insert("settings-appearance", "外观");
    zh.insert("settings-theme", "主题");
    zh.insert("settings-dark", "深色");
    zh.insert("settings-light", "浅色");
    zh.insert("settings-system", "跟随系统");
    zh.insert("settings-language", "语言");
    zh.insert("settings-english", "English");
    zh.insert("settings-chinese", "中文");
    zh.insert("settings-japanese", "日本語");
    zh.insert("settings-korean", "한국어");
    zh.insert("settings-notifications", "通知");
    zh.insert("settings-enable-notifications", "启用通知");
    zh.insert(
        "settings-notifications-hint",
        "接收智能体状态和 DAO 治理的提醒",
    );
    zh.insert("settings-auto-update", "自动更新");
    zh.insert("settings-auto-update-hint", "自动更新到最新版本");
    zh.insert("settings-network", "网络");
    zh.insert("settings-api-endpoint", "API 端点");
    zh.insert(
        "settings-api-endpoint-hint",
        "自定义 API 端点（留空使用默认）",
    );
    zh.insert("settings-wallet", "钱包");
    zh.insert("settings-wallet-address", "钱包地址");
    zh.insert("settings-wallet-placeholder", "0x...");
    zh.insert("settings-wallet-hint", "用于参与 DAO 的钱包地址");
    zh.insert("settings-connect-wallet", "连接钱包");
    zh.insert("settings-disconnect", "断开连接");
    zh.insert("settings-ai-config", "AI 配置");
    zh.insert("settings-ai-config-hint", "查看全局 LLM 提供商设置和指标");
    zh.insert("settings-open-llm", "打开 LLM 配置 →");
    zh.insert("settings-gateway", "网关设置");
    zh.insert("settings-gateway-hint", "运行配置向导来设置或重新配置网关");
    zh.insert("settings-open-wizard", "配置向导 →");
    zh.insert("settings-system-info", "系统");
    zh.insert("settings-version", "版本");
    zh.insert("settings-build", "构建");
    zh.insert("settings-platform", "平台");
    zh.insert("settings-check-updates", "检查更新");
    zh.insert("settings-config-reloaded", "配置已重载");
    zh.insert("settings-reload-config", "重载配置");
    zh.insert("settings-reset-defaults", "恢复默认");
    zh.insert("settings-save-success", "设置保存成功");
    zh.insert("settings-save-error", "保存设置失败");
    zh.insert("settings-saving", "保存中...");
    zh.insert("settings-save-changes", "保存更改");
    zh.insert("ai-store-manager-metric-reach", "今日触达");
    zh.insert("ai-store-manager-metric-reach-trend", "覆盖 3 个渠道");
    zh.insert("ai-store-manager-metric-assets", "生成素材");
    zh.insert("ai-store-manager-metric-assets-trend", "12 条待审核");
    zh.insert("ai-store-manager-metric-leads", "转化线索");
    zh.insert("ai-store-manager-metric-leads-trend", "23 位高意向");
    zh.insert("ai-store-manager-metric-revenue", "预计成交");
    zh.insert("ai-store-manager-metric-revenue-trend", "ROI 3.4");
    zh.insert("priority-high", "高");
    zh.insert("priority-medium", "中");
    zh.insert("workflows-stop-success", "工作流已停止");
    zh.insert("workflows-error-recent", "加载最近实例失败");
    zh.insert("workflows-error-definitions", "加载工作流定义失败");
    zh.insert("workflows-error-compositions", "加载技能组合失败");

    // Browser page
    zh.insert("browser-title", "浏览器");
    zh.insert("browser-automation", "浏览器自动化");
    zh.insert("browser-subtitle", "Chrome DevTools MCP 控制");
    zh.insert("browser-profiles", "配置文件");
    zh.insert("browser-add-profile", "+ 添加配置");
    zh.insert("browser-sandboxes", "沙盒");
    zh.insert("browser-create-sandbox", "+ 创建沙盒");
    zh.insert("browser-toggle-profiles", "切换配置文件");
    zh.insert("browser-toggle-sandboxes", "切换沙盒");
    zh.insert("browser-url-placeholder", "输入网址...");
    zh.insert("browser-go", "前往");
    zh.insert("browser-toggle-debug", "切换调试面板");
    zh.insert("browser-screenshot", "截图");
    zh.insert("browser-preview", "浏览器预览");
    zh.insert("browser-connecting", "连接中...");
    zh.insert("browser-connection-failed", "连接失败");
    zh.insert("browser-no-browser", "未连接浏览器");
    zh.insert("browser-select-profile", "选择一个配置文件进行连接");
    zh.insert("browser-debug-console", "调试控制台");
    zh.insert("browser-clear", "清空");
    zh.insert("browser-debug-logs-hint", "调试日志将显示在这里...");
    zh.insert("browser-add-profile-modal", "添加浏览器配置");
    zh.insert("browser-profile-name", "配置名称");
    zh.insert("browser-profile-placeholder", "例如：工作配置");
    zh.insert("browser-cdp-port", "CDP 端口");
    zh.insert("browser-profile-created", "配置已创建");
    zh.insert("browser-create-failed", "创建失败");
    zh.insert("browser-create", "创建");
    zh.insert("browser-cancel", "取消");
    zh.insert("browser-create-sandbox-modal", "创建沙盒");
    zh.insert("browser-sandbox-name", "沙盒名称");
    zh.insert("browser-sandbox-placeholder", "例如：测试沙盒");
    zh.insert("browser-base-profile", "基础配置");
    zh.insert("browser-sandbox-created", "沙盒已创建");
    zh.insert("browser-delete-profile", "删除配置");
    zh.insert("browser-delete-sandbox", "删除沙盒");
    zh.insert("browser-debug-panel-hidden", "调试面板已隐藏");
    // DAO page
    zh.insert("dao-page-title", "DAO 治理");
    zh.insert("dao-governance", "DAO 治理");
    zh.insert("dao-subtitle", "参与社区驱动的决策");
    zh.insert("dao-view-treasury", "查看金库 →");
    zh.insert("dao-members", "DAO 成员");
    zh.insert("dao-active-proposals", "活跃提案");
    zh.insert("dao-voting-power", "您的投票权");
    zh.insert("dao-balance", "您的余额");
    zh.insert("dao-proposals-title", "治理提案");
    zh.insert("dao-new-proposal", "+ 新建提案");
    zh.insert("dao-create-proposal", "创建提案");
    zh.insert("dao-proposal-title-label", "标题");
    zh.insert("dao-proposal-title-placeholder", "提案标题");
    zh.insert("dao-proposal-desc-label", "描述");
    zh.insert("dao-proposal-desc-placeholder", "描述您的提案...");
    zh.insert("dao-proposal-type", "类型");
    zh.insert("dao-type-general", "普通");
    zh.insert("dao-type-funding", "资金");
    zh.insert("dao-type-upgrade", "升级");
    zh.insert("dao-type-parameter", "参数");
    zh.insert("dao-cancel", "取消");
    zh.insert("dao-creating", "创建中...");
    zh.insert("dao-create-proposal-btn", "创建提案");
    zh.insert("dao-failed-load-proposals", "加载提案失败");
    zh.insert("dao-active-group", "活跃提案");
    zh.insert("dao-past-group", "历史提案");
    zh.insert("dao-status-active", "进行中");
    zh.insert("dao-status-passed", "已通过");
    zh.insert("dao-status-rejected", "已拒绝");
    zh.insert("dao-status-executed", "已执行");
    zh.insert("dao-status-pending", "待处理");
    zh.insert("dao-by", "发起人：");
    zh.insert("dao-ends", "截止：");
    zh.insert("dao-vote-for", "赞成");
    zh.insert("dao-vote-against", "反对");
    zh.insert("dao-vote-submitted", "投票已提交");
    zh.insert("dao-vote-recorded", "您的投票已成功记录");
    zh.insert("dao-vote-failed", "投票失败");
    zh.insert("dao-vote-submit-failed", "提交投票失败");
    zh.insert("dao-voting", "投票中...");
    zh.insert("dao-voted-for", "✓ 您投了赞成票");
    zh.insert("dao-voted-against", "✓ 您投了反对票");
    zh.insert("dao-no-proposals", "暂无提案");
    zh.insert("dao-first-proposal", "成为第一个创建治理提案的人");
    // Treasury page
    zh.insert("treasury-page-title", "金库");
    zh.insert("treasury-breadcrumb-dao", "DAO");
    zh.insert("treasury-breadcrumb-treasury", "金库");
    zh.insert("treasury-title", "DAO 金库");
    zh.insert("treasury-subtitle", "通过透明的链上治理管理社区资金");
    zh.insert("treasury-transfer", "转账");
    zh.insert("treasury-to-address", "接收地址");
    zh.insert("treasury-address-placeholder", "0x...");
    zh.insert("treasury-amount", "金额 (wei)");
    zh.insert("treasury-cancel", "取消");
    zh.insert("treasury-submitting", "提交中...");
    zh.insert("treasury-submit-transfer", "提交转账");
    zh.insert("treasury-total-balance", "金库总余额");
    zh.insert("treasury-live", "● 实时");
    zh.insert("treasury-deposit", "存入");
    zh.insert("treasury-withdraw", "提取");
    zh.insert("treasury-assets", "资产");
    zh.insert("treasury-recent-transactions", "最近交易");
    zh.insert("treasury-view-all", "查看全部 →");
    zh.insert("treasury-about", "关于金库");
    zh.insert("treasury-multi-sig", "多重签名保护");
    zh.insert(
        "treasury-multi-sig-desc",
        "所有提款都需要 DAO 理事会成员的多重签名",
    );
    zh.insert("treasury-transparent", "透明");
    zh.insert(
        "treasury-transparent-desc",
        "所有交易都记录在链上，可公开验证",
    );
    zh.insert("treasury-governance", "治理控制");
    zh.insert(
        "treasury-governance-desc",
        "重大资金分配需要通过 DAO 提案进行社区投票",
    );
    zh.insert("treasury-no-assets", "金库中没有资产");
    zh.insert("treasury-first-deposit", "进行首笔存入");
    zh.insert("treasury-no-transactions", "暂无近期交易");
    zh.insert("treasury-failed-load", "加载金库失败");
    zh.insert("treasury-retry", "重试");
    zh.insert("treasury-transactions-title", "交易历史");
    zh.insert("treasury-transactions-subtitle", "所有金库交易都记录在链上");
    zh.insert("treasury-all-transactions", "全部交易");
    zh.insert("treasury-address-required", "地址和金额为必填项");
    zh.insert("treasury-transfer-submitted", "转账已提交");
    zh.insert("treasury-transfer-failed", "转账失败");
    zh.insert("treasury-breadcrumb-transactions", "交易");
    zh.insert("treasury-tokens", "个代币");
    zh.insert("treasury-total", "笔总计");
    zh.insert("dao-create-failed", "创建提案失败");
    zh.insert("treasury-tx-deposit", "存入");
    zh.insert("treasury-tx-withdrawal", "提取");
    zh.insert("treasury-tx-transfer", "转账");
    zh.insert("treasury-tx-swap", "兑换");
    zh.insert("treasury-status-pending", "待处理");
    zh.insert("treasury-status-completed", "已完成");
    zh.insert("treasury-status-failed", "失败");

    translations.insert("zh-CN", zh);

    // English translations
    let mut en = HashMap::new();
    en.insert(
        "app-title",
        "BeeBotOS - Web4.0 Autonomous Agent Operating System",
    );
    en.insert(
        "app-description",
        "The Operating System for Autonomous AI Agents",
    );
    en.insert("nav-home", "Home");
    en.insert("nav-agents", "Agents");
    en.insert("nav-dao", "DAO");
    en.insert("nav-treasury", "Treasury");
    en.insert("nav-skills", "Skills");
    en.insert("nav-mcp-server", "MCP Server");
    en.insert("nav-ai-commerce", "AI Commerce");
    en.insert("nav-ai-store-manager", "AI Store Manager");
    en.insert("nav-skill-instances", "Instances");
    en.insert("nav-workflows", "Workflows");
    en.insert("nav-cron-jobs", "Cron Jobs");
    en.insert("nav-llm-settings", "LLM Model");
    en.insert("nav-channels", "Channels");
    en.insert("nav-settings", "Settings");
    en.insert("nav-chat", "Chat");
    en.insert("nav-browser", "Browser");
    en.insert("action-get-started", "Get Started");
    en.insert("action-browse-skills", "Browse Skills");
    en.insert("action-create", "Create");
    en.insert("action-view", "View");
    en.insert("action-browse", "Browse");
    en.insert("action-save", "Save");
    en.insert("action-cancel", "Cancel");
    en.insert("action-delete", "Delete");
    en.insert("action-edit", "Edit");
    en.insert("action-submit", "Submit");
    en.insert("action-refresh", "Refresh");
    en.insert("action-loading", "Loading...");
    en.insert("action-back", "Back");
    en.insert("action-close", "Close");
    en.insert("action-search", "Search");
    en.insert("action-filter", "Filter");
    en.insert("action-install", "Install");
    en.insert("action-uninstall", "Uninstall");
    en.insert("action-enable", "Enable");
    en.insert("action-disable", "Disable");
    en.insert("action-login", "Login");
    en.insert("action-logout", "Logout");
    en.insert("action-register", "Register");
    // Login page
    en.insert("login-title", "Welcome Back");
    en.insert("login-subtitle", "Sign in to your BeeBotOS account");
    en.insert("login-username", "Username");
    en.insert("login-username-placeholder", "Enter your username");
    en.insert("login-password", "Password");
    en.insert("login-password-placeholder", "Enter your password");
    en.insert("login-error-empty", "Username and password cannot be empty");
    en.insert("login-error-failed", "Login failed");
    en.insert("login-or", "OR");
    en.insert("login-demo-button", "Demo Login");
    en.insert("login-no-account", "Don't have an account?");
    en.insert("login-register-link", "Register now");
    en.insert(
        "login-demo-hint",
        "Demo mode: Enter any username and password to login",
    );
    // Register page
    en.insert("register-title", "Create Account");
    en.insert(
        "register-subtitle",
        "Register a BeeBotOS account to get started",
    );
    en.insert("register-username", "Username");
    en.insert("register-username-placeholder", "Enter your username");
    en.insert("register-email", "Email");
    en.insert("register-email-placeholder", "Enter your email (optional)");
    en.insert("register-password", "Password");
    en.insert(
        "register-password-placeholder",
        "Enter password (at least 6 characters)",
    );
    en.insert("register-confirm-password", "Confirm Password");
    en.insert(
        "register-confirm-password-placeholder",
        "Enter password again",
    );
    en.insert(
        "register-error-empty",
        "Username and password cannot be empty",
    );
    en.insert("register-error-password-mismatch", "Passwords do not match");
    en.insert(
        "register-error-password-short",
        "Password must be at least 6 characters",
    );
    en.insert("register-error-failed", "Registration failed");
    en.insert("register-or", "OR");
    en.insert("register-demo-button", "Demo Register");
    en.insert("register-have-account", "Already have an account?");
    en.insert("register-login-link", "Login now");
    en.insert(
        "hero-title",
        "The Operating System for Autonomous AI Agents",
    );
    en.insert(
        "hero-subtitle",
        "Build, deploy, and manage intelligent agents with built-in governance",
    );
    en.insert("hero-cta-primary", "Get Started");
    en.insert("hero-cta-secondary", "Browse Skills");
    en.insert("features-title", "Core Features");
    en.insert("feature-agents-title", "Autonomous Agents");
    en.insert(
        "feature-agents-desc",
        "Deploy AI agents that operate independently with built-in safety controls",
    );
    en.insert("feature-dao-title", "DAO Governance");
    en.insert(
        "feature-dao-desc",
        "Community-driven decision making with transparent voting mechanisms",
    );
    en.insert("feature-treasury-title", "Secure Treasury");
    en.insert(
        "feature-treasury-desc",
        "Multi-sig treasury management with on-chain transparency",
    );
    en.insert("feature-skills-title", "Skill Marketplace");
    en.insert(
        "feature-skills-desc",
        "Extend agent capabilities with community-built skills",
    );
    en.insert("feature-wasm-title", "WebAssembly Runtime");
    en.insert(
        "feature-wasm-desc",
        "High-performance, sandboxed execution environment",
    );
    en.insert("feature-analytics-title", "Real-time Analytics");
    en.insert(
        "feature-analytics-desc",
        "Monitor agent performance and system health in real-time",
    );
    en.insert("quick-actions-title", "Quick Actions");
    en.insert("quick-action-create-agent-title", "Create Agent");
    en.insert(
        "quick-action-create-agent-desc",
        "Set up a new autonomous agent",
    );
    en.insert("quick-action-view-proposals-title", "View Proposals");
    en.insert(
        "quick-action-view-proposals-desc",
        "Participate in DAO governance",
    );
    en.insert("quick-action-install-skills-title", "Install Skills");
    en.insert(
        "quick-action-install-skills-desc",
        "Add capabilities to your agents",
    );
    en.insert("agents-title", "Agents");
    en.insert("agents-subtitle", "Manage your autonomous AI agents");
    en.insert("agents-create-new", "Create New Agent");
    en.insert("agents-no-agents", "No agents found");
    en.insert("agents-loading", "Loading agents...");
    en.insert("agents-error", "Failed to load agents");
    en.insert("status-active", "Active");
    en.insert("status-idle", "Idle");
    en.insert("status-paused", "Paused");
    en.insert("status-error", "Error");
    en.insert("status-offline", "Offline");
    en.insert("status-running", "Running");
    en.insert("status-completed", "Completed");
    en.insert("status-pending", "Pending");
    // Channels
    en.insert("channels-title", "Channel Management");
    en.insert(
        "channels-subtitle",
        "Configure and manage message channel connections",
    );
    en.insert("channel-status", "Channel Status");
    en.insert("channel-config", "Channel Configuration");
    en.insert("status-enabled", "Enabled");
    en.insert("status-disabled", "Disabled");
    en.insert("wechat-login", "WeChat Login");
    en.insert(
        "wechat-login-hint",
        "Scan QR code with WeChat to get Bot Token",
    );
    en.insert("qr-expires-in", "QR expires in");
    en.insert("action-get-qr", "Get QR Code");
    en.insert("action-refresh-qr", "Refresh QR Code");
    en.insert("action-test", "Test Connection");
    en.insert("config-base-url", "Base URL");
    en.insert("config-bot-token", "Bot Token");
    en.insert("config-auto-reconnect", "Auto Reconnect");

    en.insert("dao-title", "DAO Governance");
    en.insert("dao-subtitle", "Participate in community decision-making");
    en.insert("dao-active-proposals", "Active Proposals");
    en.insert("dao-completed-proposals", "Completed Proposals");
    en.insert("dao-create-proposal", "Create Proposal");
    en.insert("dao-vote-for", "Vote For");
    en.insert("dao-vote-against", "Vote Against");
    en.insert("dao-votes-for", "For");
    en.insert("dao-votes-against", "Against");
    en.insert("dao-voting-ends", "Voting ends");
    en.insert("dao-executed", "Executed");
    en.insert("treasury-title", "Treasury");
    en.insert("treasury-subtitle", "Manage DAO assets and transactions");
    en.insert("treasury-total-balance", "Total Balance");
    en.insert("treasury-assets", "Assets");
    en.insert("treasury-transactions", "Transactions");
    en.insert("treasury-deposit", "Deposit");
    en.insert("treasury-withdraw", "Withdraw");
    en.insert("skills-title", "Skill Marketplace");
    en.insert("skills-subtitle", "Discover and install agent capabilities");
    en.insert("skills-categories", "Categories");
    en.insert("skills-installed", "Installed");
    en.insert("skills-available", "Available");
    en.insert("skills-search-placeholder", "Search skills...");
    en.insert("skills-tab-all", "All");
    en.insert("skills-tab-local", "Local");
    en.insert("skills-tab-clawhub", "ClawHub");
    en.insert("skills-tab-beehub", "BeeHub");
    en.insert("skills-category-all", "All");
    en.insert("skills-category-trading", "Trading");
    en.insert("skills-category-data", "Data");
    en.insert("skills-category-social", "Social");
    en.insert("skills-category-automation", "Automation");
    en.insert("skills-category-analysis", "Analysis");
    en.insert("skills-search", "Search");
    en.insert("skill-installed", "Installed");
    en.insert("skill-details", "Details");
    en.insert("skill-uninstall", "Uninstall");
    en.insert("skill-install", "Install");
    en.insert("skill-view-on-hub", "View on Hub");
    en.insert("skill-version", "Version");
    en.insert("skill-author", "Author");
    en.insert("skill-license", "License");
    en.insert("skill-downloads", "Downloads");
    en.insert("skill-rating", "Rating");
    en.insert("skill-description", "Description");
    en.insert("skill-capabilities", "Capabilities");
    en.insert("skill-tags", "Tags");
    en.insert("skill-none-listed", "None listed");
    en.insert("skill-removing", "Removing...");
    en.insert("skill-installing", "Installing...");
    en.insert("skills-no-results", "No results");
    en.insert("skills-search-hint", "Enter a keyword to search for skills");
    en.insert(
        "skills-try-different",
        "Try a different search term or switch to Local skills",
    );
    en.insert("skills-failed-load", "Failed to load skills");
    en.insert(
        "skills-hub-unreachable",
        "The skill hub is currently unreachable",
    );
    en.insert(
        "skills-check-network",
        "Please switch to Local skills or check Gateway network configuration",
    );
    en.insert("skills-retry", "Retry");
    en.insert("skills-no-skills-found", "No skills found");
    en.insert(
        "skills-adjust-search",
        "Try adjusting your search or filters",
    );
    en.insert("settings-title", "Settings");
    en.insert("settings-subtitle", "Configure your BeeBotOS instance");
    en.insert("settings-general", "General");
    en.insert("settings-appearance", "Appearance");
    en.insert("settings-language", "Language");
    en.insert("settings-theme", "Theme");
    en.insert("theme-light", "Light");
    en.insert("theme-dark", "Dark");
    en.insert("theme-system", "System");
    en.insert("settings-notifications", "Notifications");
    en.insert("settings-security", "Security");
    en.insert("settings-wallet", "Wallet");
    en.insert("settings-system", "System Info");
    en.insert("footer-copyright", "© 2026 BeeBotOS. All rights reserved.");
    en.insert("footer-version", "Version");
    en.insert("error-404-title", "404");
    en.insert("error-404-message", "Page not found");
    en.insert(
        "error-404-description",
        "The page you're looking for doesn't exist or has been moved.",
    );
    en.insert("error-go-home", "Go Home");
    en.insert("error-generic", "Something went wrong");
    en.insert("error-retry", "Try Again");
    en.insert("notification-success", "Success");
    en.insert("notification-error", "Error");
    en.insert("notification-warning", "Warning");
    en.insert("notification-info", "Info");
    en.insert("logout-success", "You have been successfully logged out");
    en.insert("stats-tasks", "Tasks Completed");
    en.insert("stats-uptime", "System Uptime");
    en.insert("stats-members", "Community Members");
    en.insert("quick-action-start-chat-title", "Start Chat");
    en.insert(
        "quick-action-start-chat-desc",
        "Start a conversation with AI agents",
    );
    en.insert("nav-section-chat", "Chat");
    en.insert("nav-section-control", "Control");
    en.insert("nav-section-agents", "Agents");
    en.insert("nav-section-settings", "Settings");
    en.insert("footer-product", "Product");
    en.insert("footer-resources", "Resources");
    en.insert("footer-community", "Community");
    en.insert("ai-commerce-title", "AI Commerce");
    en.insert(
        "ai-commerce-subtitle",
        "Manage external store entries, one-click access after linking.",
    );
    en.insert("ai-commerce-enter-store", "Enter Store");
    en.insert("ai-commerce-coming-soon", "Coming Soon");
    en.insert("ai-commerce-status-pending", "Pending");
    en.insert("ai-store-manager-title", "AI Store Manager");
    en.insert(
        "ai-store-manager-subtitle",
        "Use AI to batch-generate video, graphic, and phone marketing tasks.",
    );
    en.insert("ai-store-manager-import-products", "Import Products");
    en.insert("ai-store-manager-create-task", "Create Marketing Task");
    en.insert("ai-store-manager-marketing-entries", "Marketing Entries");
    en.insert("ai-store-manager-todo", "Marketing To-Do");
    en.insert("ai-store-manager-video-marketing", "AI Video Marketing");
    en.insert(
        "ai-store-manager-video-desc",
        "Generate short video scripts, storyboards, titles, voiceover scripts, and subtitles by \
         product, selling point, and platform.",
    );
    en.insert("ai-store-manager-video-core", "Core Capabilities");
    en.insert("ai-store-manager-create-video", "Create Video Task");
    en.insert("ai-store-manager-graphic-marketing", "AI Graphic Marketing");
    en.insert(
        "ai-store-manager-graphic-desc",
        "Generate seeding copy, Moments content, poster copy, and product detail optimization \
         suggestions.",
    );
    en.insert("ai-store-manager-graphic-core", "Core Capabilities");
    en.insert("ai-store-manager-create-graphic", "Create Graphic Task");
    en.insert("ai-store-manager-phone-marketing", "AI Phone Marketing");
    en.insert(
        "ai-store-manager-phone-desc",
        "Generate outbound call scripts and follow-up tasks for existing customer repurchase, \
         event notifications, and high-intent leads.",
    );
    en.insert("ai-store-manager-phone-core", "Core Capabilities");
    en.insert("ai-store-manager-create-phone", "Create Call Task");
    en.insert("skill-instances-title", "Instance Management");
    en.insert(
        "skill-instances-subtitle",
        "Manage skill instances bound to your agents",
    );
    en.insert("skill-instances-new", "New Instance");
    en.insert("skill-instances-cancel", "Cancel");
    en.insert("skill-instances-create", "Create Instance");
    en.insert("skill-instances-creating", "Creating...");
    en.insert("skill-instances-skill-id", "Skill ID");
    en.insert("skill-instances-skill-id-placeholder", "e.g. echo-skill");
    en.insert("skill-instances-agent-id", "Agent ID");
    en.insert("skill-instances-agent-id-placeholder", "e.g. agent-001");
    en.insert("skill-instances-table-id", "Instance ID");
    en.insert("skill-instances-table-skill", "Skill");
    en.insert("skill-instances-table-agent", "Agent");
    en.insert("skill-instances-table-status", "Status");
    en.insert("skill-instances-table-usage", "Usage");
    en.insert("skill-instances-table-actions", "Actions");
    en.insert("skill-instances-run", "Run");
    en.insert("skill-instances-running", "Running...");
    en.insert("skill-instances-delete", "Delete");
    en.insert("skill-instances-empty-title", "No instances yet");
    en.insert(
        "skill-instances-empty-desc",
        "Create a new instance to bind a skill to an agent",
    );
    en.insert("skill-instances-error-title", "Failed to load instances");
    en.insert("skill-instances-missing-fields", "Missing Fields");
    en.insert(
        "skill-instances-fill-fields",
        "Please fill in both Skill ID and Agent ID",
    );
    en.insert("skill-instances-created", "Instance Created");
    en.insert("skill-instances-creation-failed", "Creation Failed");
    en.insert("skill-instances-deleted", "Instance Deleted");
    en.insert("skill-instances-delete-failed", "Delete Failed");
    en.insert("skill-instances-execution-result", "Execution Result");
    en.insert("skill-instances-execution-failed", "Execution Failed");
    en.insert("workflows-title", "Workflows");
    en.insert(
        "workflows-subtitle",
        "Monitor workflow definitions, executions, and skill compositions",
    );
    en.insert("workflows-refresh", "Refresh");
    en.insert("workflows-refreshing", "Refreshing...");
    en.insert("workflows-tab-orchestration", "📋 Workflow Orchestration");
    en.insert("workflows-tab-composition", "🔗 Skill Composition");
    en.insert("workflows-recent-instances", "Recent Instances");
    en.insert("workflows-definitions", "Workflow Definitions");
    en.insert("workflows-add", "Add Workflow");
    en.insert("workflows-skill-compositions", "Skill Compositions");
    en.insert("workflows-stat-total", "Total Workflows");
    en.insert("workflows-stat-instances", "Total Instances");
    en.insert("workflows-stat-completed", "Completed");
    en.insert("workflows-stat-failed", "Failed");
    en.insert("workflows-stat-running", "Running");
    en.insert("workflows-stat-pending", "Pending");
    en.insert("workflows-no-instances", "No workflow instances yet");
    en.insert("workflows-table-workflow", "Workflow");
    en.insert("workflows-table-status", "Status");
    en.insert("workflows-table-progress", "Progress");
    en.insert("workflows-table-duration", "Duration");
    en.insert("workflows-table-started", "Started");
    en.insert("workflows-no-definitions", "No workflows defined yet");
    en.insert("workflows-no-triggers", "No triggers");
    en.insert("workflows-manual-execute", "Manual execute");
    en.insert("workflows-start", "Start");
    en.insert("workflows-starting", "Starting...");
    en.insert("workflows-stop", "Stop");
    en.insert("workflows-stopping", "Stopping...");
    en.insert("workflows-stop-latest", "Stop latest running instance");
    en.insert("workflows-no-running", "No Running Instance");
    en.insert(
        "workflows-not-running",
        "This workflow is not currently running",
    );
    en.insert("workflows-schedule", "Schedule");
    en.insert("workflows-config", "Config");
    en.insert("workflows-uninstall", "Uninstall");
    en.insert("workflows-uninstalling", "Uninstalling...");
    en.insert("workflows-uninstalled", "Workflow Uninstalled");
    en.insert("workflows-uninstall-failed", "Uninstall Failed");
    en.insert("workflows-dag", "DAG");
    en.insert("workflows-reports", "Reports");
    en.insert(
        "workflows-no-reports",
        "No reports yet. Run the workflow to view the latest and historical reports here.",
    );
    en.insert("workflows-select-report", "Select a report");
    en.insert(
        "workflows-start-hint",
        "Execute starts the workflow with an empty default context. Expand advanced JSON only \
         when you need to override parameters.",
    );
    en.insert("workflows-show-context", "Advanced JSON");
    en.insert("workflows-hide-context", "Hide advanced parameters");
    en.insert(
        "workflows-trigger-context",
        "Trigger Context (optional JSON)",
    );
    en.insert(
        "workflows-trigger-hint",
        "Keep the default {} unless you need to pass a JSON object as trigger context.",
    );
    en.insert("workflows-execute", "Execute");
    en.insert("workflows-executing", "Executing...");
    en.insert("workflows-started", "Workflow Started");
    en.insert("workflows-execution-failed", "Execution Failed");
    en.insert("workflows-invalid-json", "Invalid JSON");
    en.insert("workflows-cancel", "Cancel");
    en.insert("workflows-close", "Close");
    en.insert("workflows-cron-expression", "Cron Expression");
    en.insert("workflows-timezone", "Timezone");
    en.insert(
        "workflows-cron-placeholder",
        "e.g. 0 9 * * * (daily at 9am), 0 */6 * * * (every 6 hours)",
    );
    en.insert(
        "workflows-timezone-placeholder",
        "e.g. UTC, Asia/Shanghai, America/New_York",
    );
    en.insert("workflows-parse-failed", "Parse Failed");
    en.insert("workflows-schedule-updated", "Schedule Updated");
    en.insert("workflows-schedule-update-failed", "Update Failed");
    en.insert("workflows-fetch-failed", "Fetch Failed");
    en.insert("workflows-save-schedule", "Save Schedule");
    en.insert("workflows-saving", "Saving...");
    en.insert("workflows-install-title", "Install Workflow");
    en.insert("workflows-file-path", "Workflow File Path");
    en.insert(
        "workflows-file-hint",
        "Absolute or relative path to a YAML/JSON workflow file",
    );
    en.insert("workflows-install", "Install");
    en.insert("workflows-installing", "Installing...");
    en.insert("workflows-installed", "Workflow Installed");
    en.insert("workflows-install-failed", "Install Failed");
    en.insert("workflows-load-failed", "Load Failed");
    en.insert("workflows-definition-yaml", "Workflow Definition (YAML)");
    en.insert(
        "workflows-definition-hint",
        "Edit the workflow definition directly. Be careful with YAML syntax.",
    );
    en.insert("workflows-updated", "Workflow Updated");
    en.insert("workflows-saved", "Configuration saved successfully");
    en.insert("workflows-update-failed", "Update Failed");
    en.insert("workflows-save", "Save");
    en.insert(
        "workflows-no-compositions",
        "No skill compositions defined yet",
    );
    en.insert(
        "workflows-composition-hint",
        "Create compositions via API or YAML files in data/compositions/",
    );
    en.insert("workflows-composition-executed", "Composition Executed");
    en.insert("workflows-composition-execution-failed", "Execution Failed");
    en.insert("workflows-composition-delete", "Delete composition");
    en.insert("workflows-composition-deleting", "Deleting...");
    en.insert("workflows-composition-deleted", "Composition Deleted");
    en.insert("workflows-composition-delete-failed", "Delete Failed");
    en.insert("workflows-execute-composition", "Execute composition");
    en.insert("workflows-composition-running", "Running...");
    en.insert("workflows-error-recent", "Failed to load recent instances");
    en.insert(
        "workflows-error-definitions",
        "Failed to load workflow definitions",
    );
    en.insert(
        "workflows-error-compositions",
        "Failed to load skill compositions",
    );
    en.insert("cron-jobs-title", "Cron Jobs");
    en.insert("cron-jobs-subtitle", "Manage scheduled automation tasks");
    en.insert("cron-jobs-refresh", "Refresh");
    en.insert("cron-jobs-refreshing", "Refreshing...");
    en.insert("cron-jobs-new", "New Task");
    en.insert("cron-jobs-edit", "Edit Cron Job");
    en.insert("cron-jobs-create", "Create Cron Job");
    en.insert("cron-jobs-history", "Execution History");
    en.insert("cron-jobs-empty-title", "No cron jobs yet");
    en.insert(
        "cron-jobs-empty-hint",
        "Click \"New Task\" in the top right to create your first cron job",
    );
    en.insert("cron-jobs-table-name", "Name");
    en.insert("cron-jobs-table-schedule", "Schedule Type");
    en.insert("cron-jobs-table-expression", "Expression");
    en.insert("cron-jobs-table-status", "Status");
    en.insert("cron-jobs-table-runs", "Runs");
    en.insert("cron-jobs-table-next", "Next Run");
    en.insert("cron-jobs-table-actions", "Actions");
    en.insert("cron-jobs-type-timed", "Timed");
    en.insert("cron-jobs-type-interval", "Interval");
    en.insert("cron-jobs-enable", "Enable");
    en.insert("cron-jobs-disable", "Disable");
    en.insert("cron-jobs-enabled", "Enabled");
    en.insert("cron-jobs-disabled", "Disabled");
    en.insert("cron-jobs-run", "Run");
    en.insert("cron-jobs-edit-action", "Edit");
    en.insert("cron-jobs-history-action", "History");
    en.insert("cron-jobs-delete", "Delete");
    en.insert("cron-jobs-form-name", "Job Name");
    en.insert("cron-jobs-form-name-placeholder", "e.g. Daily Report");
    en.insert("cron-jobs-form-desc", "Description");
    en.insert(
        "cron-jobs-form-desc-placeholder",
        "Briefly describe the task purpose",
    );
    en.insert("cron-jobs-form-schedule-type", "Schedule Type");
    en.insert("cron-jobs-form-expression", "Schedule Expression");
    en.insert("cron-jobs-form-cron-placeholder", "*/5 * * * *");
    en.insert("cron-jobs-form-interval-placeholder", "30m");
    en.insert(
        "cron-jobs-form-cron-hint",
        "5-field cron: minute hour day month weekday",
    );
    en.insert(
        "cron-jobs-form-interval-hint",
        "Supports s/m/h/d, e.g. 30m, 1h, 4h, 1d",
    );
    en.insert("cron-jobs-form-timezone", "Timezone");
    en.insert("cron-jobs-form-timezone-placeholder", "Asia/Shanghai");
    en.insert("cron-jobs-form-prompt", "Execution Prompt");
    en.insert(
        "cron-jobs-form-prompt-placeholder",
        "Prompt sent to Agent when task triggers",
    );
    en.insert("cron-jobs-form-context-mode", "Context Mode");
    en.insert("cron-jobs-form-context-standalone", "Standalone Session");
    en.insert("cron-jobs-form-context-shared", "Shared Session");
    en.insert("cron-jobs-form-max-runs", "Max Runs");
    en.insert(
        "cron-jobs-form-max-runs-placeholder",
        "Leave empty for unlimited",
    );
    en.insert("cron-jobs-form-channel", "Notification Channel");
    en.insert("cron-jobs-form-channel-webchat", "WebChat");
    en.insert("cron-jobs-form-channel-webhook", "Webhook");
    en.insert("cron-jobs-form-target", "Delivery Target");
    en.insert(
        "cron-jobs-form-target-webchat-hint",
        "WebSocket channel name (default: webchat)",
    );
    en.insert(
        "cron-jobs-form-target-webhook-hint",
        "URL to receive POST requests",
    );
    en.insert("cron-jobs-form-enabled", "Enable this task");
    en.insert("cron-jobs-form-cancel", "Cancel");
    en.insert("cron-jobs-form-saving", "Saving...");
    en.insert("cron-jobs-form-save", "Save");
    en.insert("cron-jobs-form-create", "Create");
    en.insert("cron-jobs-history-empty", "No execution records");
    en.insert("cron-jobs-history-loading", "Loading...");
    en.insert("cron-jobs-history-error", "Load Failed");
    en.insert("cron-jobs-error-load", "Failed to load cron jobs");
    en.insert("llm-settings-title", "LLM Settings");
    en.insert(
        "llm-settings-subtitle",
        "Select and configure the current large language model and its parameters",
    );
    en.insert("llm-settings-retry", "Retry");
    en.insert("llm-settings-provider", "Model Provider");
    en.insert("llm-settings-select-provider", "Select Provider");
    en.insert("llm-settings-model-version", "Model Version");
    en.insert("llm-settings-select-kimi", "Select Kimi Model");
    en.insert(
        "llm-settings-kimi-hint",
        "Selection will sync model, thinking, and temperature.",
    );
    en.insert("llm-settings-select-deepseek", "Select DeepSeek Model");
    en.insert(
        "llm-settings-deepseek-hint",
        "DeepSeek official models are deepseek-v4-flash / deepseek-v4-pro, selection will sync \
         thinking and reasoning_effort.",
    );
    en.insert("llm-settings-model-name", "Model Name");
    en.insert("llm-settings-model-placeholder", "e.g. gpt-4o");
    en.insert("llm-settings-temperature", "Temperature");
    en.insert(
        "llm-settings-temperature-hint",
        "Range 0.0 ~ 2.0, lower is more deterministic, higher is more creative",
    );
    en.insert(
        "llm-settings-select-provider-first",
        "Please select a model provider first",
    );
    en.insert("llm-settings-current-params", "Current Parameters");
    en.insert("llm-settings-provider-label", "Provider");
    en.insert("llm-settings-model-label", "Model");
    en.insert("llm-settings-temperature-label", "Temperature");
    en.insert("llm-settings-thinking-label", "Thinking");
    en.insert("llm-settings-reasoning-label", "Reasoning Effort");
    en.insert("llm-settings-actions", "Actions");
    en.insert("llm-settings-save-success", "Saved successfully");
    en.insert(
        "llm-settings-select-provider-model",
        "Please select provider and model",
    );
    en.insert("llm-settings-saving", "Saving...");
    en.insert("llm-settings-save-config", "Save Configuration");
    en.insert("llm-settings-reload-success", "Config reloaded");
    en.insert("llm-settings-reloading", "Reloading...");
    en.insert("llm-settings-reload-restart", "Restart to apply (Reload)");
    en.insert(
        "llm-settings-reload-hint",
        "Saved config will be written to config/beebotos.toml and hot-reloaded. For full effect, \
         click",
    );
    en.insert("agents-page-title", "Agents");
    en.insert("agents-page-subtitle", "Manage your autonomous AI agents");
    en.insert("agents-new", "New Agent");
    en.insert("agents-tasks", " tasks");
    en.insert("agents-manage", "Manage");
    en.insert("agents-stop", "Stop");
    en.insert("agents-stopping", "Stopping...");
    en.insert("agents-stopped", "Agent Stopped");
    en.insert("agents-stop-failed", "Stop Failed");
    en.insert("agents-start", "Start");
    en.insert("agents-starting", "Starting...");
    en.insert("agents-started", "Agent Started");
    en.insert("agents-start-failed", "Start Failed");
    en.insert("agents-empty-title", "No agents yet");
    en.insert(
        "agents-empty-desc",
        "Create your first autonomous agent to get started",
    );
    en.insert("agents-empty-create", "Create Agent");
    en.insert("agents-error-title", "Failed to load agents");
    en.insert("agents-retry", "Retry");
    en.insert("agents-refresh", "Refresh Page");
    en.insert("agents-create-title", "Create New Agent");
    en.insert("agents-create-name", "Agent Name *");
    en.insert("agents-create-name-placeholder", "Enter agent name");
    en.insert("agents-create-name-error", "Name cannot be empty");
    en.insert("agents-create-desc", "Description");
    en.insert("agents-create-desc-placeholder", "Enter agent description");
    en.insert("agents-create-provider", "Model Provider");
    en.insert("agents-create-model", "Model Name");
    en.insert(
        "agents-create-model-placeholder",
        "e.g. gpt-4, claude-3-opus-20240229",
    );
    en.insert("agents-create-cancel", "Cancel");
    en.insert("agents-create-creating", "Creating...");
    en.insert("agents-create-submit", "Create Agent");
    en.insert("settings-page-title", "Settings");
    en.insert(
        "settings-page-subtitle",
        "Manage your preferences and system configuration",
    );
    en.insert("settings-loading", "Loading settings...");
    en.insert("settings-appearance", "Appearance");
    en.insert("settings-theme", "Theme");
    en.insert("settings-dark", "Dark");
    en.insert("settings-light", "Light");
    en.insert("settings-system", "System");
    en.insert("settings-language", "Language");
    en.insert("settings-english", "English");
    en.insert("settings-chinese", "中文");
    en.insert("settings-japanese", "日本語");
    en.insert("settings-korean", "한국어");
    en.insert("settings-notifications", "Notifications");
    en.insert("settings-enable-notifications", "Enable notifications");
    en.insert(
        "settings-notifications-hint",
        "Receive alerts about agent status and DAO governance",
    );
    en.insert("settings-auto-update", "Auto-update");
    en.insert(
        "settings-auto-update-hint",
        "Automatically update to the latest version",
    );
    en.insert("settings-network", "Network");
    en.insert("settings-api-endpoint", "API Endpoint");
    en.insert(
        "settings-api-endpoint-hint",
        "Custom API endpoint (leave empty for default)",
    );
    en.insert("settings-wallet", "Wallet");
    en.insert("settings-wallet-address", "Wallet Address");
    en.insert("settings-wallet-placeholder", "0x...");
    en.insert(
        "settings-wallet-hint",
        "Your wallet address for DAO participation",
    );
    en.insert("settings-connect-wallet", "Connect Wallet");
    en.insert("settings-disconnect", "Disconnect");
    en.insert("settings-ai-config", "AI Configuration");
    en.insert(
        "settings-ai-config-hint",
        "View global LLM provider settings and metrics",
    );
    en.insert("settings-open-llm", "Open LLM Configuration →");
    en.insert("settings-gateway", "Gateway Setup");
    en.insert(
        "settings-gateway-hint",
        "Run the configuration wizard to setup or reconfigure Gateway",
    );
    en.insert("settings-open-wizard", "Configuration Wizard →");
    en.insert("settings-system-info", "System");
    en.insert("settings-version", "Version");
    en.insert("settings-build", "Build");
    en.insert("settings-platform", "Platform");
    en.insert("settings-check-updates", "Check for Updates");
    en.insert("settings-config-reloaded", "Config reloaded");
    en.insert("settings-reload-config", "Reload Config");
    en.insert("settings-reset-defaults", "Reset to Defaults");
    en.insert("settings-save-success", "Settings saved successfully");
    en.insert("settings-save-error", "Failed to save settings");
    en.insert("settings-saving", "Saving...");
    en.insert("settings-save-changes", "Save Changes");
    en.insert("ai-store-manager-metric-reach", "Today's Reach");
    en.insert("ai-store-manager-metric-reach-trend", "Covering 3 channels");
    en.insert("ai-store-manager-metric-assets", "Generated Assets");
    en.insert("ai-store-manager-metric-assets-trend", "12 pending review");
    en.insert("ai-store-manager-metric-leads", "Converted Leads");
    en.insert("ai-store-manager-metric-leads-trend", "23 high-intent");
    en.insert("ai-store-manager-metric-revenue", "Estimated Revenue");
    en.insert("ai-store-manager-metric-revenue-trend", "ROI 3.4");
    en.insert("priority-high", "High");
    en.insert("priority-medium", "Medium");
    en.insert("workflows-stop-success", "Workflow Stopped");
    en.insert("workflows-error-recent", "Failed to load recent instances");
    en.insert(
        "workflows-error-definitions",
        "Failed to load workflow definitions",
    );
    en.insert(
        "workflows-error-compositions",
        "Failed to load skill compositions",
    );

    // Browser page
    en.insert("browser-title", "Browser");
    en.insert("browser-automation", "Browser Automation");
    en.insert("browser-subtitle", "Chrome DevTools MCP Control");
    en.insert("browser-profiles", "Profiles");
    en.insert("browser-add-profile", "+ Add Profile");
    en.insert("browser-sandboxes", "Sandboxes");
    en.insert("browser-create-sandbox", "+ Create Sandbox");
    en.insert("browser-toggle-profiles", "Toggle Profiles");
    en.insert("browser-toggle-sandboxes", "Toggle Sandboxes");
    en.insert("browser-url-placeholder", "Enter URL...");
    en.insert("browser-go", "Go");
    en.insert("browser-toggle-debug", "Toggle Debug Panel");
    en.insert("browser-screenshot", "Take Screenshot");
    en.insert("browser-preview", "Browser Preview");
    en.insert("browser-connecting", "Connecting...");
    en.insert("browser-connection-failed", "Connection failed");
    en.insert("browser-no-browser", "No browser connected");
    en.insert("browser-select-profile", "Select a profile to connect");
    en.insert("browser-debug-console", "Debug Console");
    en.insert("browser-clear", "Clear");
    en.insert("browser-debug-logs-hint", "Debug logs will appear here...");
    en.insert("browser-add-profile-modal", "Add Browser Profile");
    en.insert("browser-profile-name", "Profile Name");
    en.insert("browser-profile-placeholder", "e.g. Work Profile");
    en.insert("browser-cdp-port", "CDP Port");
    en.insert("browser-profile-created", "Profile created");
    en.insert("browser-create-failed", "Create failed");
    en.insert("browser-create", "Create");
    en.insert("browser-cancel", "Cancel");
    en.insert("browser-create-sandbox-modal", "Create Sandbox");
    en.insert("browser-sandbox-name", "Sandbox Name");
    en.insert("browser-sandbox-placeholder", "e.g. Test Sandbox");
    en.insert("browser-base-profile", "Base Profile");
    en.insert("browser-sandbox-created", "Sandbox created");
    en.insert("browser-delete-profile", "Delete profile");
    en.insert("browser-delete-sandbox", "Delete sandbox");
    en.insert("browser-debug-panel-hidden", "Debug panel hidden");
    // DAO page
    en.insert("dao-page-title", "DAO Governance");
    en.insert("dao-governance", "DAO Governance");
    en.insert(
        "dao-subtitle",
        "Participate in community-driven decision making",
    );
    en.insert("dao-view-treasury", "View Treasury →");
    en.insert("dao-members", "DAO Members");
    en.insert("dao-active-proposals", "Active Proposals");
    en.insert("dao-voting-power", "Your Voting Power");
    en.insert("dao-balance", "Your Balance");
    en.insert("dao-proposals-title", "Governance Proposals");
    en.insert("dao-new-proposal", "+ New Proposal");
    en.insert("dao-create-proposal", "Create Proposal");
    en.insert("dao-proposal-title-label", "Title");
    en.insert("dao-proposal-title-placeholder", "Proposal title");
    en.insert("dao-proposal-desc-label", "Description");
    en.insert("dao-proposal-desc-placeholder", "Describe your proposal...");
    en.insert("dao-proposal-type", "Type");
    en.insert("dao-type-general", "General");
    en.insert("dao-type-funding", "Funding");
    en.insert("dao-type-upgrade", "Upgrade");
    en.insert("dao-type-parameter", "Parameter");
    en.insert("dao-cancel", "Cancel");
    en.insert("dao-creating", "Creating...");
    en.insert("dao-create-proposal-btn", "Create Proposal");
    en.insert("dao-failed-load-proposals", "Failed to load proposals");
    en.insert("dao-active-group", "Active Proposals");
    en.insert("dao-past-group", "Past Proposals");
    en.insert("dao-status-active", "Active");
    en.insert("dao-status-passed", "Passed");
    en.insert("dao-status-rejected", "Rejected");
    en.insert("dao-status-executed", "Executed");
    en.insert("dao-status-pending", "Pending");
    en.insert("dao-by", "By ");
    en.insert("dao-ends", "Ends: ");
    en.insert("dao-vote-for", "Vote For");
    en.insert("dao-vote-against", "Vote Against");
    en.insert("dao-vote-submitted", "Vote Submitted");
    en.insert(
        "dao-vote-recorded",
        "Your vote has been recorded successfully",
    );
    en.insert("dao-vote-failed", "Vote Failed");
    en.insert("dao-vote-submit-failed", "Failed to submit vote");
    en.insert("dao-voting", "Voting...");
    en.insert("dao-voted-for", "✓ You voted For");
    en.insert("dao-voted-against", "✓ You voted Against");
    en.insert("dao-no-proposals", "No proposals yet");
    en.insert(
        "dao-first-proposal",
        "Be the first to create a governance proposal",
    );
    // Treasury page
    en.insert("treasury-page-title", "Treasury");
    en.insert("treasury-breadcrumb-dao", "DAO");
    en.insert("treasury-breadcrumb-treasury", "Treasury");
    en.insert("treasury-title", "DAO Treasury");
    en.insert(
        "treasury-subtitle",
        "Manage community funds with transparent, on-chain governance",
    );
    en.insert("treasury-transfer", "Transfer");
    en.insert("treasury-to-address", "To Address");
    en.insert("treasury-address-placeholder", "0x...");
    en.insert("treasury-amount", "Amount (wei)");
    en.insert("treasury-cancel", "Cancel");
    en.insert("treasury-submitting", "Submitting...");
    en.insert("treasury-submit-transfer", "Submit Transfer");
    en.insert("treasury-total-balance", "Total Treasury Balance");
    en.insert("treasury-live", "● Live");
    en.insert("treasury-deposit", "Deposit");
    en.insert("treasury-withdraw", "Withdraw");
    en.insert("treasury-assets", "Assets");
    en.insert("treasury-recent-transactions", "Recent Transactions");
    en.insert("treasury-view-all", "View All →");
    en.insert("treasury-about", "About the Treasury");
    en.insert("treasury-multi-sig", "Multi-Sig Protected");
    en.insert(
        "treasury-multi-sig-desc",
        "All withdrawals require multiple signatures from DAO council members",
    );
    en.insert("treasury-transparent", "Transparent");
    en.insert(
        "treasury-transparent-desc",
        "All transactions are recorded on-chain and publicly verifiable",
    );
    en.insert("treasury-governance", "Governance Controlled");
    en.insert(
        "treasury-governance-desc",
        "Major allocations require community vote through DAO proposals",
    );
    en.insert("treasury-no-assets", "No assets in treasury");
    en.insert("treasury-first-deposit", "Make First Deposit");
    en.insert("treasury-no-transactions", "No recent transactions");
    en.insert("treasury-failed-load", "Failed to load treasury");
    en.insert("treasury-retry", "Retry");
    en.insert("treasury-transactions-title", "Transaction History");
    en.insert(
        "treasury-transactions-subtitle",
        "All treasury transactions are recorded on-chain",
    );
    en.insert("treasury-all-transactions", "All Transactions");
    en.insert(
        "treasury-address-required",
        "Address and amount are required",
    );
    en.insert("treasury-transfer-submitted", "Transfer submitted");
    en.insert("treasury-transfer-failed", "Transfer failed");
    en.insert("treasury-breadcrumb-transactions", "Transactions");
    en.insert("treasury-tokens", "tokens");
    en.insert("treasury-total", "total");
    en.insert("dao-create-failed", "Failed to create proposal");
    en.insert("treasury-tx-deposit", "Deposit");
    en.insert("treasury-tx-withdrawal", "Withdrawal");
    en.insert("treasury-tx-transfer", "Transfer");
    en.insert("treasury-tx-swap", "Swap");
    en.insert("treasury-status-pending", "Pending");
    en.insert("treasury-status-completed", "Completed");
    en.insert("treasury-status-failed", "Failed");

    translations.insert("en", en);

    let i18n = I18nContext {
        locale: RwSignal::new(Locale::ZhCN),
        translations,
    };

    provide_context(i18n.clone());
    i18n
}

/// Get the current locale
pub fn current_locale(i18n: &I18nContext) -> Locale {
    i18n.get_locale()
}

/// Set the locale
pub fn set_locale(i18n: &I18nContext, locale: Locale) {
    i18n.set_locale(locale);
}

/// Toggle between Chinese and English
pub fn toggle_locale(i18n: &I18nContext) {
    let new_locale = match i18n.get_locale() {
        Locale::ZhCN => Locale::En,
        _ => Locale::ZhCN,
    };
    i18n.set_locale(new_locale);
}
