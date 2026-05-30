---
name: fred-macro-data
description: >
  通过美联储经济数据 (FRED) 官方 API 获取权威宏观数据：
  真实美元指数 (DTWEXO/DTWEXBGS)、10年期国债收益率 (DGS10)、
  联邦基金利率 (FEDFUNDS)、盈亏平衡通胀率 (T10YIE)、
  核心 PCE 物价指数 (PCEPILFE) 等。
  数据直接来自美联储圣路易斯分行，无需网页抓取。
version: "1.1.0"
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

# FRED 美联储宏观数据

> **数据权威性**: 所有数据来自美联储圣路易斯分行 (Federal Reserve Bank of St. Louis) 的官方数据库。

## 前置依赖

```bash
pip install requests
```

## API Key 获取

FRED API 完全免费，仅需注册获取 Key：

1. 访问 https://fred.stlouisfed.org/docs/api/api_key.html
2. 点击 "Request API Key"
3. 填入邮箱，1 分钟内收到 Key（32 位小写字母数字组合）
4. 将 Key 设置为环境变量：
   ```bash
   export FRED_API_KEY="your_32_char_key_here"
   ```

## 支持的数据序列

| Series ID | 名称 | 类别 | 对黄金的影响逻辑 |
|-----------|------|------|------------------|
| `DTWEXO` | Trade Weighted U.S. Dollar Index: Major Currencies | 美元指数代理 | **反向**。美元走强 → 黄金承压 |
| `DGS10` | 10-Year Treasury Constant Maturity Rate | 利率预期代理 | **反向**。收益率上升 → 实际利率上行 → 黄金承压 |
| `FEDFUNDS` | Federal Funds Effective Rate | 当前政策利率 | **反向**。利率越高，黄金机会成本越大 |
| `T10YIE` | 10-Year Breakeven Inflation Rate | 通胀预期 | **正向**。通胀上升 → 抗通胀需求 → 黄金受益 |
| `PCEPILFE` | Core PCE Price Index | 核心通胀 | **正向**。但需结合美联储政策预期综合判断 |

### 核心 PCE 特殊说明

`PCEPILFE` 是美联储**首选的通胀指标**。脚本会自动获取最近 14 个月数据并计算：
- **同比年率 (yoy_pct)**: 与 12 个月前对比的百分比变化（市场关注值）
- **环比变化 (mom_pct)**: 最近一个月的变化

## 使用方法

### 命令行

```bash
# 默认组合（含核心 PCE）
python3 {SKILL_DIR}/scripts/fred_fetch.py --api-key "{{env.FRED_API_KEY}}"

# 完整宏观组合
python3 {SKILL_DIR}/scripts/fred_fetch.py \
  --api-key "{{env.FRED_API_KEY}}" \
  --series "DTWEXO,DGS10,FEDFUNDS,T10YIE,PCEPILFE"
```

### 输出格式示例

```json
{
  "timestamp": "2026-05-29T14:30:00",
  "series": {
    "PCEPILFE": {
      "name": "Core PCE Price Index",
      "category": "inflation",
      "latest_date": "2026-04-01",
      "latest_index": 125.34,
      "yoy_pct": 2.8,
      "mom_pct": 0.3
    }
  }
}
```

## 故障排查

| 现象 | 原因 | 解决 |
|------|------|------|
| `Bad Request. api_key...` | API Key 格式错误或已失效 | 重新从 FRED 官网申请 |
| `Insufficient observations` | PCEPILFE 需要 13+ 个月数据，序列可能太新 | 罕见，通常 FRED 数据回溯到 1959 年 |
| `timed out after 20s` | 网络问题 | 检查外网连通性，Workflow 会自动重试 |
