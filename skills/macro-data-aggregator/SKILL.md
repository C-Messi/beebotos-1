---
name: macro-data-aggregator
description: >
  宏观数据统一聚合 Skill。一键并行获取四层数据源：
  Yahoo Finance (VIX/GLD/股指)、FRED (美元指数/利率/核心PCE)、
  WGC/IMF (央行购金)、Google News RSS (地缘政治风险)。
  通过 ThreadPoolExecutor 并行拉取，
  将原来 3 个 Skill + 1 个缺失维度的开销合并为 1 个，Workflow 步骤从 4 步减为 1 步。
version: "1.0.0"
metadata:
  author: BeeBotOS
  agent:
    requires:
      bins: ["python3"]
    install:
      - id: pip
        kind: python
        package: "requests>=2.28.0,yfinance>=0.2.54"
        bins: []
        label: "Install requests and yfinance"
permissions:
  - process:exec
---

# 宏观数据统一聚合器

> **设计目标**: 将原本分散在 3 个 Skill（yfinance-market-data、fred-macro-data、wgc-central-bank-gold）中的数据获取逻辑合并为单一脚本，减少 Skill 注册、加载和上下文切换开销。

## 前置依赖

```bash
pip install requests yfinance
```

## 使用方法

### 命令行

```bash
# 完整获取（推荐）
python3 {SKILL_DIR}/scripts/fetch_all_macro.py \
  --yfinance-symbols "^VIX,GLD,DX-Y.NYB,GC=F,^GSPC,^DJI,^IXIC" \
  --fred-api-key "{{env.FRED_API_KEY}}" \
  --fred-series "DTWEXO,DGS10,FEDFUNDS,T10YIE,PCEPILFE" \
  --wgc \
  --geopolitical

# 跳过 WGC 和地缘政治（仅市场数据+利率，最快）
python3 {SKILL_DIR}/scripts/fetch_all_macro.py \
  --fred-api-key "{{env.FRED_API_KEY}}" \
  --no-wgc \
  --no-geopolitical
```

### 输出格式

```json
{
  "timestamp": "2026-05-30T10:00:00",
  "yfinance": {
    "price_data": {
      "^VIX": {"price": 13.45, "prev_close": 14.20, "change_pct": -5.28},
      "GLD": {"price": 234.50, "prev_close": 233.00, "change_pct": 0.64}
    },
    "etf_info": {"GLD": {"total_assets": 62000000000}},
    "gold_futures_detail": {"open_interest": 450000}
  },
  "fred": {
    "series": {
      "DTWEXO": {"latest_value": 104.12, "change_pct": 0.23},
      "PCEPILFE": {"latest_index": 125.34, "yoy_pct": 2.8, "mom_pct": 0.3}
    }
  },
  "wgc": {
    "status": "degraded",
    "background_knowledge": {
      "latest_quarter_net_purchases_tonnes": 244,
      "key_trend": "Central bank net purchases remained elevated..."
    }
  },
  "geopolitical": {
    "status": "ok",
    "risk_score": 5.17,
    "risk_level": "medium",
    "article_count": 59,
    "keywords_found": {"war": 8, "tensions": 6, "deal": 4, "attack": 3},
    "top_headlines": [
      {"title": "Iran threatens to extend conflict beyond the region...", "source": "Al Jazeera", "score": 4.5}
    ],
    "queries_used": ["iran israel conflict war", "hormuz strait oil shipping", "middle east tensions escalation"],
    "data_source": "Google News RSS (no API key required)"
  }
}
```

## 在 Workflow 中使用

```yaml
steps:
  - id: fetch_macro
    skill: macro-data-aggregator
    params:
      instruction: |
        获取全部宏观数据：
        python3 {SKILL_DIR}/scripts/fetch_all_macro.py \
          --fred-api-key "{{env.FRED_API_KEY}}" \
          --wgc
    timeout_sec: 60
    retries: 2
```

## 性能优势

| 指标 | 3 个独立 Skill + N/A | 统一聚合 Skill |
|------|----------------------|----------------|
| Skill 注册数 | 3 | **1** |
| Workflow 步骤数 | 3 + 地缘 N/A | **1** |
| 数据获取方式 | 串行或并行调度 | **内部 ThreadPoolExecutor 并行** |
| 典型总耗时 | 15-30s | **5-15s** |
| 上下文切换 | 3 次 Skill 加载 | **1 次 Skill 加载** |
| 地缘风险 | **N/A（无工具）** | **Google News RSS 实时情感分析** |

## 故障排查

| 现象 | 原因 | 解决 |
|------|------|------|
| `yfinance not installed` | Python 依赖缺失 | `pip install yfinance requests` |
| `FRED API Key Bad Request` | Key 未设置或错误 | 检查 `FRED_API_KEY` 环境变量 |
| 某模块返回 error | 对应数据源网络/API 问题 | 其他模块仍正常返回，整体任务继续 |
| 地缘风险 `status: degraded` | Google News RSS 被墙/超时 | 降级为静态背景知识，不影响交易决策 |

## 地缘政治风险模块说明

**数据源**: Google News RSS（无需 API Key，完全免费）
**查询主题**:
- `iran israel conflict war`
- `hormuz strait oil shipping`
- `middle east tensions escalation`

**评分算法**:
- 对每篇文章标题进行关键词匹配
- 高风险词（war, attack, missile, strike...）+2 分
- 中风险词（deadline, warning, alert...）+1 分
- 缓和词（peace, talks, ceasefire, diplomacy...）-1.5 分
- 文章数量作为"关注度"因子（越多 = 风险越高）
- 最终归一化到 **0-10 分**，分级: low / medium / high / extreme

**输出字段**:
- `risk_score`: 0-10 数值
- `risk_level`: low / medium / high / extreme
- `article_count`: 去重后文章总数
- `keywords_found`: 命中关键词频次统计
- `top_headlines`: 得分最高的 8 条标题
