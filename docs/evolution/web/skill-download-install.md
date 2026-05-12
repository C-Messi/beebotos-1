
## BeeBotOS apps/WEB 技能市场：ClawHub Skill 下载与安装 API 方法

### 一、ClawHub REST API 调用（WEB 前端直接调用或 Gateway 代理）

| 功能 | HTTP 方法 | 端点 | 说明 |
|------|-----------|------|------|
| 搜索技能 | GET | `https://api.clawhub.ai/api/v1/skills/search?q={keyword}&limit=20` | 关键词查找，支持分页 |
| 获取详情 | GET | `https://api.clawhub.ai/api/v1/skills/{slug}` | 获取元数据、版本、依赖 |
| 下载技能包 | GET | `https://api.clawhub.ai/api/v1/skills/{id}/download` | 返回 ZIP 文件，内含 `SKILL.md` 与 `_meta.json` |

### 二、本地安装方法

下载完成后，客户端将 ZIP 包解压至 Agent 指定工作目录：

- **BeeBotOS 标准安装路径**：`data/skills/installed/{skill_id}/`
- **默认数据目录**：
  - 生产环境：`data/beebotos/skills/installed/{skill_id}/`
  - 开发环境：`./data/skills/installed/{skill_id}/`

### 三、WEB 模块调用流程

```javascript
// 1. 搜索技能
const list = await fetch('https://clawhub.ai/api/v1/skills/search?q=crypto&limit=20');

// 2. 获取指定技能详情
const meta = await fetch(`https://clawhub.ai/api/v1/skills/${slug}`);

// 3. 下载 ZIP 包 问题：这个下载链接有错！
const zip = await fetch(`https://clawhub.ai/api/v1/skills/${id}/download`).then(r => r.blob());

// 4. 提交至本地 Gateway 安装接口，由后端解压到 installed 目录
await fetch('/api/v1/skills/install', {
  method: 'POST',
  headers: { 'Content-Type': 'application/zip' },
  body: zip
});
```

### 四、Gateway 后端处理

Gateway 接收 ZIP 后执行：
1. 校验 `_meta.json` 版本与依赖
2. 解压到 `data/skills/installed/{skill_id}/`
3. 更新本地 `registry/skills.json`
4. 向 Agent Runtime 注册该技能




