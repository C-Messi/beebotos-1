---
name: yfinance-market-data
description: >
  通过 Yahoo Finance (yfinance) 免费 API 获取全球宏观市场数据：
  VIX恐慌指数、美元指数DXY、黄金ETF(GLD)持仓、黄金期货(GC=F)、
  美股三大指数(标普500/道指/纳指)、10年期美债收益率等。
  无需API Key，不依赖网页抓取，直接调用Yahoo Finance接口。
version: "1.0.0"
metadata:
  author: BeeBotOS
  agent:
    requires:
      bins: ["python3"]
    install:
      - id: pip
        kind: python
        package: "yfinance>=0.2.54"
        bins: []
        label: "Install yfinance"
permissions:
  - process:exec
---

# Yahoo Finance 宏观数据获取

> **数据合规**: 本 Skill 仅通过 Yahoo Finance 公共 API 获取市场数据，不构成投资建议。

## 前置依赖

```bash
pip install yfinance
```

如果环境中无 pip，请先安装 pip：
```bash
apt-get install -y python3-pip   # Debian/Ubuntu
# 或
yum install -y python3-pip       # CentOS/RHEL
```

## 功能范围

| 数据维度 | Yahoo 代码 | 说明 |
|----------|-----------|------|
| VIX 恐慌指数 | `^VIX` | 实时数值与近24h变化 |
| 黄金 ETF (SPDR) | `GLD` | 价格、持仓量(totalAssets) |
| 美元指数 | `DX-Y.NYB` | DXY 近似指数 |
| 黄金期货 (COMEX) | `GC=F` | 近月期货价格、持仓量 |
| 标普500 | `^GSPC` | 美股大盘 |
| 道琼斯 | `^DJI` | 道指 |
| 纳斯达克 | `^IXIC` | 纳指 |
| 10年期美债收益率 | `^TNX` | 作为实际利率粗略代理 |
| 白银 ETF | `SLV` | 可选参考 |

## 使用方法

### 命令行

```bash
# 获取全部默认指标（推荐）
python3 {SKILL_DIR}/scripts/fetch_macro.py --type all

# 仅获取价格与涨跌幅
python3 {SKILL_DIR}/scripts/fetch_macro.py --type price

# 自定义代码列表
python3 {SKILL_DIR}/scripts/fetch_macro.py \
  --symbols "^VIX,GLD,DX-Y.NYB,GC=F,^GSPC,^DJI,^IXIC" \
  --type all

# 使用别名输出（更易读）
python3 {SKILL_DIR}/scripts/fetch_macro.py \
  --symbols "VIX,GLD,DXY,GC,SPX,DJI,IXIC" \
  --aliases --type all
```

### 输出格式

```json
{
  "timestamp": "2026-05-29T14:30:00",
  "data_source": "Yahoo Finance via yfinance",
  "price_data": {
    "^VIX": {
      "price": 13.45,
      "prev_close": 14.20,
      "change_pct": -5.28
    },
    "GLD": {
      "price": 234.50,
      "prev_close": 233.00,
      "change_pct": 0.64
    },
    "DX-Y.NYB": {
      "price": 104.20,
      "prev_close": 103.80,
      "change_pct": 0.38
    },
    "GC=F": {
      "price": 2345.60,
      "prev_close": 2330.00,
      "change_pct": 0.67
    }
  },
  "etf_info": {
    "GLD": {
      "total_assets": 62000000000,
      "nav_price": 234.20,
      "currency": "USD"
    }
  },
  "gold_futures_detail": {
    "open_interest": 450000,
    "volume": 180000
  }
}
```

## 在 Workflow 中使用

作为 `steps` 中的数据获取节点，输出可通过 `{{steps.<id>.output}}` 传递给下游分析 Skill。

```yaml
steps:
  - id: fetch_macro
    skill: yfinance-market-data
    params:
      command: |
        python3 {SKILL_DIR}/scripts/fetch_macro.py --type all
    timeout_sec: 30
    retries: 2
```

## 局限性

1. **数据延迟**: Yahoo Finance 数据有 15-30 分钟延迟，不适合高频交易，但足够用于小时级量化决策。
2. **网络依赖**: 需要能访问 Yahoo Finance 服务器（`query1.finance.yahoo.com`）。
3. **非官方 API**: yfinance 是社区维护的逆向工程库，接口可能变动。
4. **DXY 精度**: `DX-Y.NYB` 是美元指数 ETN，与 ICE DXY 期货有细微差异。
5. **ETF 持仓量**: `totalAssets` 为基金总资产，非精确吨数；精确吨数需参考 SPDR 官方每日披露。

## 故障排查

| 现象 | 解决 |
|------|------|
| `yfinance not installed` | 执行 `pip install yfinance` |
| `No data returned` | Yahoo 临时限制，等待 30 秒后重试 |
| 数据全为 `null` | 非交易日或 symbol 错误，检查代码拼写 |
