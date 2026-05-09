
POAGENT这套 skill 的“意图识别”其实分三条路径，不是一套统一分类器。

1. 普通对话：让 LLM 自己匹配 description

启动时 registry 会把可用 skill 压成一个 <available_skills> prompt，只暴露：

name
description
location
最多 50 个，按 usage_count 排序：skill_prompt.rs (line 28)。

然后 Agent system prompt 里明确告诉模型：

先扫描 <description>
如果一个明显适用，调用 read_skill
多个适用，选最具体的
没有明显适用，不读 skill
代码在 agent_impl.rs (line 960)。

也就是说，主路径不是 embedding、不是规则引擎，而是 LLM 读 description 后自主判断意图。read_skill 被调用后才读取完整 SKILL.md，这就是 progressive disclosure：read_skill.rs (line 48)。

2. 工具检索：关键词搜索辅助

list_skills 支持 query/category，其中 query 走 registry search：list_skills.rs (line 48)。

SkillRegistry::search 逻辑很朴素：

name/description 直接 substring 命中，给高分
否则把 query、name、description、capabilities 拆词
用关键词 overlap 排序
在 registry.rs (line 198)。

这个搜索不是主对话自动选择的核心，只是给模型或 API 查 skill 用。

3. Planning/SkillExecution：规则 + 显式参数

如果任务类型就是 SkillExecution，那不是识别意图，而是参数里必须有 skill，直接 registry.get：agent_impl.rs (line 1070)。

如果是 plan step，代码有一套硬编码 domain keyword 映射，比如 code、travel、finance、security 映射到特定 skill id，再 fallback 到 registry search/tag：agent_impl.rs (line 1808)。

所以 POAGENT 当前机制可以总结为：

普通对话靠 LLM 对 description 做语义判断；计划任务额外加一层硬编码关键词路由；显式 skill 任务直接指定 skill。

对比 Claude Code

Claude Code / Claude Skills 的官方机制和 BeeBotOS 主路径非常像：skill 是目录，包含 SKILL.md，通过 description 判断何时使用，并采用 progressive disclosure。Claude 官方文档说 metadata 会先加载，任务匹配 description 时才加载完整 SKILL.md，额外 scripts/resources 只在需要时加载。来源：Claude Skills overview 与 Claude Code Agent Skills 文档。
https://claude.com/docs/skills/overview
https://docs.claude.com/en/docs/claude-code/skills

差异在于：

Claude 的 description 是一等公民，官方明确强调“写清楚何时使用”；POAGENT 也依赖 description，但内置 loader 会从正文/标题兜底生成，质量更不稳定。
Claude Code 支持 personal/project/plugin skills 路径，比如 ~/.claude/skills、.claude/skills、plugin skills；POAGENT 是内置 skills/ 加安装目录 data/skills。
Claude Code 有 allowed-tools 这类 skill 级权限约束；POAGENT 当前 skill 只是提示包，没有 per-skill tool permission。
Claude 的资源加载模型更完整：SKILL.md 可指向 scripts、templates、references；POAGENT 当前 read_skill 只读主 SKILL.md，没有专门的资源分层加载协议。
Claude 的选择机制更“纯模型驱动”；POAGENT 在 planning 里混入了硬编码关键词表，短期可控，长期维护成本高。
对比 Codex / OpenAI Skills

OpenAI 官方 Help 说 Skills 是可复用、可分享 workflow，可包含 instructions、examples、code；安装后 ChatGPT 可在有帮助时自动使用一个或多个 skill，并且 skills 支持 Codex 和 API，遵循开放 Agent Skills 标准。来源：
https://help.openai.com/en/articles/20001066

Codex/Codex 类方案和POAGENT 的相似点：

都是 SKILL.md 风格的可复用工作流。
都靠简短 metadata/description 先做发现，再按需读取完整说明。
都把 skill 看成“让 agent 更稳定地执行流程”的指令包，而不是必须是可执行插件。
差异更关键：

Codex 的上下文通常是代码仓库和任务执行环境，skill 会和文件编辑、命令执行、测试、补丁生成深度结合；POAGENT 的 skill 目前更多是 LLM persona/prompt template。
OpenAI/Codex skills 可以包含代码和资源，但是否运行取决于 agent 工具/沙箱策略；POAGENT 当前源码明确说 WASM execution removed，执行 skill 是再调用 LLM：agent_impl.rs (line 990)。
Codex 主流实现更重视权限、沙箱、审批、工作区隔离；POAGENT 的 skill 层本身没有这些边界，边界主要在 Agent 工具层。
OpenAI 文档提到可自动使用一个或多个 skill；POAGENT prompt 明确要求“不要预先读取多个技能，只选一个”，组合能力更弱。
我对 POAGENT 这套的判断

它已经抓住了主流方向：description 驱动发现 + 按需读取 SKILL.md + 避免上下文爆炸。这和 Claude/Codex 的核心思路一致。

但它现在更像“轻量 skill prompt registry”，还没到成熟 agent skill runtime：

意图识别过度依赖 description 文案质量。
没有 embedding/reranker/负例训练，误触发和漏触发都靠 prompt 缓解。
没有多 skill 编排策略。
没有 skill 级权限、资源加载、依赖声明、脚本执行边界。
Web/API 里还有旧 instance/WASM 残留，和当前实现不一致。
如果要对齐 Claude Code / Codex，我会优先补四件事：统一 SKILL.md 标准、加 skill activation debug trace、引入可选 embedding/rerank、给 skill 增加 allowed_tools/resources/scripts 分层加载


