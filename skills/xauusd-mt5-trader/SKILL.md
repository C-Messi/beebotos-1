---
name: xauusd-mt5-trader
description: >
  XAUUSD 现货黄金量化交易 Skill。连接四层数据源：
  1) Yahoo Finance 宏观数据 (VIX/GLD/DXY/股指)
  2) MT5 直连数据 (XAUUSD 实时价格与24h涨跌、账户、交易执行)
  3) FRED 美联储官方数据 (真实美元指数/国债收益率/联邦基金利率/核心PCE)
  4) WGC 央行购金数据 (IMF API 尽力获取)
  执行数据搜集、分析决策、交易执行和状态汇报。
  严禁使用任何网页抓取工具。
license: MIT
metadata:
  author: BeeBotOS
  version: "1.3.0"
  agent:
    requires:
      bins: []
---

# XAUUSD MT5 量化交易员

> **合规声明**: 本 Skill 仅用于 MT5 模拟盘自动化测试，不构成投资建议。

## 绝对约束（违反将导致任务失败）

1. **严禁调用 `web_search` 和 `web_fetch`**。如果某项数据无法通过本文件列出的工具或外部传入数据获取，直接标注为 **"N/A"**，不得尝试其他来源，不得中断任务。
2. 交易标的统一为 **XAUUSD**。
3. 所有订单使用 **市价单 (market order)**。
4. 当前环境为 **MT5 模拟盘**。

## 可用工具与数据来源

### A. 外部传入数据（Workflow 上游步骤提供，优先使用）

当本 Skill 通过 Workflow 调用时，`macro_data` 会作为参数传入。**优先读取这些传入数据**，不要再调用重复工具。

**macro_data**（来自 `macro-data-aggregator` Skill，统一聚合格式）：
```json
{
  "yfinance": {
    "price_data": {
      "^VIX": {"price": 13.45, "prev_close": 14.20, "change_pct": -5.28},
      "GLD":   {"price": 234.50, "prev_close": 233.00, "change_pct": 0.64},
      "DX-Y.NYB": {"price": 104.20, "prev_close": 103.80, "change_pct": 0.38},
      "GC=F":  {"price": 2345.60, "prev_close": 2330.00, "change_pct": 0.67},
      "^GSPC": {"price": 5300.00, "prev_close": 5280.00, "change_pct": 0.38},
      "^DJI":  {"price": 39000.00, "prev_close": 38850.00, "change_pct": 0.39},
      "^IXIC": {"price": 16800.00, "prev_close": 16700.00, "change_pct": 0.60}
    },
    "etf_info": {
      "GLD": {"total_assets": 62000000000, "nav_price": 234.20}
    },
    "gold_futures_detail": {"open_interest": 450000, "volume": 180000}
  },
  "fred": {
    "series": {
      "DTWEXO": {"latest_value": 104.12, "prev_value": 103.88, "change_pct": 0.23},
      "DGS10":  {"latest_value": 4.32, "prev_value": 4.28, "change_pct": 0.93},
      "FEDFUNDS":{"latest_value": 5.33, "prev_value": 5.33, "change_pct": 0.0},
      "T10YIE":  {"latest_value": 2.35, "prev_value": 2.30, "change_pct": 2.17},
      "PCEPILFE":{"latest_index": 125.34, "yoy_pct": 2.8, "mom_pct": 0.3}
    }
  },
  "wgc": {
    "status": "degraded",
    "quarterly_change_tonnes": "N/A",
    "background_knowledge": {
      "latest_quarter_net_purchases_tonnes": 244,
      "key_trend": "Central bank net purchases remained elevated in Q1 2026 at ~244t..."
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
    "data_source": "Google News RSS"
  }
}
```

**mt5_data**（来自 MT5 MCP 直接查询）：
- `mcp:metatrader/get_symbol_price` (symbol: "XAUUSD") — 实时 bid/ask
- `mcp:metatrader/get_symbol_info` (symbol: "XAUUSD") — 24h 涨跌、点差、合约规格等（如 MCP 支持）

### B. 市场数据（MT5 MCP）

**与模拟盘直连，价格最准确**：
- `mcp:metatrader/get_symbol_price` — `{"symbol": "XAUUSD"}`
- `mcp:metatrader/get_symbol_info` — `{"symbol": "XAUUSD"}`（如果返回中包含 24h 涨跌则直接用）

### C. 账户与交易（MT5 MCP 工具）

| 工具 | 用途 | 关键参数 |
|------|------|----------|
| `mcp:metatrader/get_account_info` | 查询账户权益、余额、杠杆 | 无 |
| `mcp:metatrader/get_symbol_price` | 查询 XAUUSD 实时报价 | `symbol: "XAUUSD"` |
| `mcp:metatrader/get_symbol_info` | 查询 XAUUSD 完整信息（含24h涨跌、点差等） | `symbol: "XAUUSD"` |
| `mcp:metatrader/get_all_positions` | 查询当前全部持仓 | 无 |
| `mcp:metatrader/place_order` | 下市价单 | `symbol`, `action: "buy"\|"sell"`, `volume` |
| `mcp:metatrader/close_position` | 平仓指定持仓 | 视 MCP server 实现（ticket / symbol） |

> 如不确定 `close_position` 的参数，可先通过 `mcp_tool_search` 搜索 `metatrader` 关键词确认可用工具列表，**仅允许这一次搜索**。

---

## 执行流程

### Step 1 — 数据搜集（必须逐项完成）

按以下维度搜集数据。**优先使用传入数据，缺失则通过工具获取，再缺失标 N/A：**

| # | 维度 | 数据来源 | 说明 |
|---|------|----------|------|
| 1 | 地缘政治风险 | `macro_data.geopolitical` | Google News RSS 情感分析，0-10 风险分 |
| 2 | VIX 恐慌指数 | `macro_data.yfinance.price_data["^VIX"]` | 数值 + 24h 变化 |
| 3 | 美股三大指数 | `macro_data.yfinance.price_data["^GSPC"]/["^DJI"]/["^IXIC"]` | 近24h涨跌幅 |
| 4 | 美联储预期 | `macro_data.fred.series["DGS10"]` + `macro_data.fred.series["FEDFUNDS"]` | DGS10 作为利率预期代理 |
| 5 | 实际利率 | `macro_data.fred.series["DGS10"]` - `macro_data.fred.series["T10YIE"]` | 精确实际利率近似值 |
| 6 | 美元指数 DXY | `macro_data.fred.series["DTWEXO"]` | 美联储官方广义美元指数 |
| 7 | XAUUSD 当前价格 | `mcp:metatrader/get_symbol_price` (symbol: "XAUUSD") | 必拿，MT5 直连最准确 |
| 8 | XAUUSD 近24h涨跌幅 | `mcp:metatrader/get_symbol_info` (symbol: "XAUUSD") 如果返回24h变化；否则通过 `get_symbol_price` 对比24h前价格计算 | 必拿 |
| 9 | 账户总权益 | `mcp:metatrader/get_account_info` | 必拿 |
| 10 | 当前持仓方向与盈亏 | `mcp:metatrader/get_all_positions` | 必拿 |
| 11 | 黄金 ETF 持仓 (SPDR) | `macro_data.yfinance.etf_info["GLD"].total_assets` | 以美元计的总资产 |
| 12 | COMEX 持仓 | `macro_data.yfinance.gold_futures_detail.open_interest` | 期货持仓量代理 |
| 13 | 央行购金 | `macro_data.wgc` | 季度低频数据，作为结构性背景 |
| 14 | 核心 PCE 通胀 | `macro_data.fred.series["PCEPILFE"].yoy_pct` | 美联储首选通胀指标，同比年率 |

**输出格式**：将以上数据整理为结构化 JSON/Text，进入下一步。

### Step 2 — 波动环境判断

综合以下信号：

- **高波动**（满足任一）：
  - XAUUSD 近24h涨跌幅绝对值 **> 2%**
  - VIX **> 20** 且较昨日上升
  - 美股三大指数同时跌幅 **> 1%**
- **正常波动**：
  - 数据分化，有明确主线但非极端
  - XAUUSD 涨跌幅 **0.5% ~ 2%**
- **低波动**：
  - XAUUSD 涨跌幅绝对值 **< 0.5%**
  - VIX **< 15** 且平稳
  - 美股波动极小

### Step 3 — 决策输出

严格按照以下格式输出：

```
决策倾向：[偏多/偏空/观望]
波动环境：[高/正常/低]
综合评分：[1-10分]（1=最悲观，10=最乐观）
核心依据：[用一句话概括最核心的一到两条数据]
仓位比例：[高波动5% / 正常波动10% / 低波动10% / 观望0%]
```

**多因子决策规则**（按优先级排序）：

| 优先级 | 条件 | 决策 |
|--------|------|------|
| P-1 | 地缘风险 `risk_level` = "extreme" + 金价上涨/持平 | **偏多**（地缘避险主导，可覆盖其他信号） |
| P0 | VIX > 20 且上升 + 美股大跌 + 金价上涨 | **偏多**（避险驱动） |
| P0b | 地缘风险 `risk_level` = "high" + VIX > 18 + 金价上涨 | **偏多**（双重避险） |
| P1 | FRED DGS10 大幅下降 + DTWEXO 下降 | **偏多**（降息预期 + 美元弱） |
| P2 | FRED DGS10 大幅上升 + DTWEXO 上升 | **偏空**（加息预期 + 美元强） |
| P3 | 核心 PCE (yoy_pct) > 3% 且上升 + 金价涨 | **偏多**（通胀超预期，黄金抗通胀） |
| P4 | 核心 PCE (yoy_pct) < 2% 且下降 + DGS10 降 | **观望/偏空**（通缩风险或软着陆） |
| P5 | T10YIE 上升 + 金价同步上升 | **偏多**（通胀驱动） |
| P6 | DXY(DTWEXO) 大涨 + 金价下跌 | **偏空**（美元压制） |
| P7 | DXY(DTWEXO) 大跌 + 金价上涨 | **偏多**（美元走弱） |
| P8 | 央行购金背景知识显示 "elevated" / "accelerating" | **偏多**（结构性支撑，但权重低） |
| P9 | 数据矛盾或波动极低 | **观望** |

**地缘政治风险权重说明**：
- 地缘风险是**高权重事件驱动因子**，在 `extreme` 级别时可覆盖部分宏观信号
- `risk_score >= 7.5` (extreme) + 金价未大跌 → **强制偏多**（战争/重大冲突避险）
- `risk_score 5.5-7.5` (high) + VIX 上升 → **偏多**（避险情绪共振）
- `risk_score 3.0-5.5` (medium) → 作为辅助参考，权重约 10-15%
- `risk_score < 3.0` (low) → 忽略地缘因子，专注宏观
- 当 `geopolitical.status == "degraded"` 时，回退到旧策略（不纳入决策）

**FedWatch 代理解读**（当无 CME FedWatch 时）：
- `DGS10` 上升 + `DTWEXO` 上升 → 市场预期利率上行 → **偏鹰** → 黄金承压
- `DGS10` 下降 + `DTWEXO` 下降 → 市场预期利率下行 → **偏鸽** → 黄金受益
- `FEDFUNDS` 处于高位且不变 + `DGS10` 下降 → 预期未来降息 → **偏多**
- `T10YIE`（通胀预期）单独上升 → 抗通胀需求 → **偏多**
- `PCEPILFE.yoy_pct` > 3% 且未回落 → 美联储可能维持高利率更久 → 需结合 DGS10 方向判断
- `PCEPILFE.yoy_pct` 持续下降 toward 2% → 降息预期升温 → **偏多**（若 DGS10 同步下降）

**央行购金权重说明**：
- 央行购金是**季度低频结构性因子**，在小时级决策中权重应较低（约 5-10%）
- 当 `macro_data.wgc.status == "degraded"` 时，使用 `macro_data.wgc.background_knowledge.key_trend` 作为定性参考
- 当 `quarterly_change_tonnes > 200` 且趋势为 accelerating → 轻微利多
- 当 `quarterly_change_tonnes < 100` 或趋势为 slowing → 轻微利空或无影响

**持仓状态修正**：
- 如已有多头且价格续涨 → 维持偏多，**不再加仓**
- 如已有空头且价格续跌 → 维持偏空，**不再加仓**

### Step 4 — 交易执行

根据决策倾向，严格执行对应分支：

**分支A：偏多**
1. 调用 `mcp:metatrader/get_all_positions` 查询当前持仓。
2. 如有空头仓位（position_type 包含 sell/short），立即全部平仓。
3. 平仓后再次查询持仓。如已有多头仓位，跳过开仓，保持持有。
4. 如无任何多头仓位：
   - 计算交易量：`volume = floor(equity * 仓位比例 / current_price / 100) / 100`
   - 注意 MT5 XAUUSD 的合约规格，如计算结果小于 0.01，则取 0.01。
   - 调用 `mcp:metatrader/place_order`，参数：`{"symbol": "XAUUSD", "action": "buy", "volume": <计算值>}`
5. 输出：`"执行偏多操作完成。"`

**分支B：偏空**
1. 查询当前持仓。
2. 如有多头仓位，立即全部平仓。
3. 平仓后再次查询持仓。如已有空头仓位，跳过开仓，保持持有。
4. 如无任何空头仓位：
   - 计算交易量同上。
   - 调用 `mcp:metatrader/place_order`，参数：`{"symbol": "XAUUSD", "action": "sell", "volume": <计算值>}`
5. 输出：`"执行偏空操作完成。"`

**分支C：观望**
1. 查询当前持仓。
2. 如有任何仓位（多或空），全部平仓。
3. 输出：`"执行观望操作完成，已清空所有持仓。"`

> **平仓说明**：如 `mcp:metatrader/close_position` 可用，直接传入 ticket 或 symbol 平仓。如不可用，通过 `place_order` 下反向等量的市价单对冲平仓。

### Step 5 — 最终状态汇报

交易操作完成后，必须再次查询并输出：

```
【最终状态汇报】
- 当前持仓方向：[无/多/空]
- 入场均价：[数值，无持仓写0]
- 当前浮动盈亏：[以账户货币计，无持仓写0]
- 账户总权益：[数值]
- 今日已平仓交易笔数：[如有记录则写，否则写N/A]
```

数据来源：`mcp:metatrader/get_all_positions` + `mcp:metatrader/get_account_info`。

---

## 错误处理

| 场景 | 行为 |
|------|------|
| `macro_data` 解析失败 | 回退到标 N/A，继续执行 |
| MT5 MCP 价格/涨跌获取失败 | 尝试通过 MCP 其他工具获取；如均失败则标 N/A，继续执行 |
| OKX CLI 返回错误 | 标注对应数据为 N/A，继续执行 |
| MT5 MCP 连接失败 | 输出错误信息，终止任务（交易不可执行） |
| place_order 返回错误 | 输出失败原因，不再重试，汇报当前状态 |
| 计算 volume <= 0 | 使用 MT5 最小允许交易量（通常 0.01） |

---

## 注意事项

- **MT5 XAUUSD 价格直连**：`mcp:metatrader/get_symbol_price` 返回的是模拟盘实时报价，与交易执行价格完全一致，无需经过外部交易所。
- **24h 涨跌获取策略**：先尝试 `get_symbol_info`，如 MCP 不支持则通过 `get_symbol_price` 的历史数据能力计算。
- **FRED DTWEXO** 是美联储官方编制的广义美元指数，比 Yahoo 的 `DX-Y.NYB` ETN 更权威，优先使用。
- **核心 PCE (PCEPILFE)** 是美联储首选通胀指标。脚本输出的 `yoy_pct` 是市场关注的核心数值（如 2.8%）。
  - PCE > 3% 且上升：通胀粘性，可能迫使美联储维持高利率，对黄金偏空（除非避险因素主导）
  - PCE 持续向 2% 靠拢：软着陆预期，降息空间打开，对黄金偏多
- **央行购金**是季度低频数据，在小时级交易中权重应低于 DXY、利率预期、VIX 等高频因子。
- 如遇外部传入数据为空/无效，则按文档中的降级逻辑处理，**不要试图通过 web 工具补充**。
