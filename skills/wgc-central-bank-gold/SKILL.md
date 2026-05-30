---
name: wgc-central-bank-gold
description: >
  尽力获取全球央行黄金净购金数据。首选通过 IMF IFS API 获取主要央行黄金储备季度变化；
  如 API 不可用，返回 WGC 最新已知背景数据作为决策参考。
  本 Skill 为"尽力而为"实现，因 WGC 季度数据无公开稳定 API。
version: "1.0.0"
metadata:
  author: BeeBotOS
  agent:
    requires:
      bins: ["python3"]
    install:
      - id: pip
        kind: python
        package: "requests>=2.28.0"
        bins: []
        label: "Install requests"
permissions:
  - process:exec
---

# WGC 央行购金数据

> **数据性质**: 全球央行黄金净购金数据由世界黄金协会 (WGC) 基于 IMF IFS 统计编制，**季度发布，滞后约 2 个月**。对小时级交易策略而言，这属于结构性背景因子，而非高频战术信号。

## 前置依赖

```bash
pip install requests
```

## 数据来源说明

| 层级 | 来源 | 可靠性 | 延迟 |
|------|------|--------|------|
| P0 | IMF IFS API (SDMX) | 官方，但 indicator code 可能变动 | 季度 |
| P1 | WGC Gold Demand Trends 报告 | 权威，但无实时 API | 季度+2月 |
| P2 | 内置背景知识 | 静态参考，非实时 | N/A |

## 使用方法

### 命令行

```bash
# 获取完整 JSON 报告
python3 {SKILL_DIR}/scripts/fetch_wgc.py --output json

# 获取纯文本摘要
python3 {SKILL_DIR}/scripts/fetch_wgc.py --output summary
```

### 输出格式

**成功时（IMF API 可用）**:
```json
{
  "status": "partial",
  "quarterly_change_tonnes": 244.0,
  "trend_direction": "accelerating",
  "countries": {
    "CN": {"name": "China", "latest_period": "2026-Q1", "latest_value": 2313.0, ...},
    "RU": {"name": "Russia", "latest_period": "2026-Q1", "latest_value": 2350.0, ...}
  },
  "note": "Based on 5/8 major central banks. IMF data may lag WGC estimates."
}
```

**降级时（IMF API 不可用）**:
```json
{
  "status": "degraded",
  "quarterly_change_tonnes": "N/A",
  "trend_direction": "N/A",
  "background_knowledge": {
    "latest_wgc_report": "Gold Demand Trends Q1 2026",
    "latest_quarter_net_purchases_tonnes": 244,
    "key_trend": "Central bank net purchases remained elevated in Q1 2026 at ~244t...",
    "data_lag_warning": "WGC data is quarterly and lags by ~2 months."
  }
}
```

## 在 Workflow 中使用

```yaml
steps:
  - id: fetch_wgc
    skill: wgc-central-bank-gold
    params:
      instruction: |
        获取央行购金数据：
        python3 {SKILL_DIR}/scripts/fetch_wgc.py --output json
    timeout_sec: 45
    retries: 1
```

## 注意事项

1. **低频数据**: 央行购金是季度数据，每小时重复获取不会有新信息。Workflow 可改为每季度触发一次，或读取缓存。
2. **战术权重低**: 在小时级 XAUUSD 交易中，央行购金的决策权重应低于 DXY、利率预期、VIX 等高频因子。
3. **背景知识有效性**: 当 API 降级时，脚本内置的 background_knowledge 基于 WGC 最新公开报告，会随时间老化。建议每季度手动更新脚本中的参考数据。
