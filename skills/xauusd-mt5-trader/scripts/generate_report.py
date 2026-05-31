#!/usr/bin/env python3
"""Generate a full XAUUSD macro report from aggregator JSON."""

import argparse
import json
import os
from datetime import datetime


def load_json(path):
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def value_at(data, *path, default="N/A"):
    cur = data
    for key in path:
        if not isinstance(cur, dict) or key not in cur:
            return default
        cur = cur[key]
    if cur is None or cur == "":
        return default
    return cur


def fmt(value, suffix="", default="N/A"):
    if value in (None, "", "N/A"):
        return default
    if isinstance(value, float):
        return f"{value:.2f}{suffix}"
    return f"{value}{suffix}"


def pct(value):
    return fmt(value, "%")


def signal_from_change(value, up_label="上升", down_label="下降"):
    try:
        number = float(value)
    except (TypeError, ValueError):
        return "N/A"
    if number > 0:
        return up_label
    if number < 0:
        return down_label
    return "持平"


def safe_table_cell(value):
    text = str(value)
    return text.replace("\n", " ").replace("|", "\\|")


def market_row(label, data, symbol):
    item = value_at(data, "yfinance", "price_data", symbol, default={})
    if not isinstance(item, dict):
        item = {}
    price = item.get("price", "N/A")
    change = item.get("change_pct", "N/A")
    error = item.get("error")
    if error:
        return f"| {label} | {symbol} | N/A | N/A | {safe_table_cell(error)} |"
    return f"| {label} | {symbol} | {fmt(price)} | {pct(change)} | {signal_from_change(change)} |"


def fred_row(label, data, series_id, field="latest_value", unit=""):
    item = value_at(data, "fred", "series", series_id, default={})
    if not isinstance(item, dict):
        item = {}
    error = item.get("error")
    if error:
        return f"| {label} | {series_id} | N/A | N/A | {safe_table_cell(error)} |"
    value = item.get(field, "N/A")
    change = item.get("change_pct", item.get("mom_pct", "N/A"))
    return f"| {label} | {series_id} | {fmt(value, unit)} | {pct(change)} | {signal_from_change(change)} |"


def infer_bias(data):
    geo_level = value_at(data, "geopolitical", "risk_level")
    geo_score = value_at(data, "geopolitical", "risk_score")
    vix = value_at(data, "yfinance", "price_data", "^VIX", "price")
    vix_chg = value_at(data, "yfinance", "price_data", "^VIX", "change_pct")
    dxy_chg = value_at(data, "fred", "series", "DTWEXO", "change_pct")
    dgs10_chg = value_at(data, "fred", "series", "DGS10", "change_pct")
    pce_yoy = value_at(data, "fred", "series", "PCEPILFE", "yoy_pct")
    gc_chg = value_at(data, "yfinance", "price_data", "GC=F", "change_pct")

    score = 5
    reasons = []

    try:
        if float(geo_score) >= 7.5:
            score += 2
            reasons.append("地缘风险极高，避险需求显著")
        elif geo_level in ("high", "medium"):
            score += 1
            reasons.append(f"地缘风险为 {geo_level}")
    except (TypeError, ValueError):
        pass

    try:
        if float(vix) > 20 and float(vix_chg) > 0:
            score += 1
            reasons.append("VIX 上升且高于 20")
        elif float(vix) < 15:
            reasons.append("VIX 低位，恐慌情绪有限")
    except (TypeError, ValueError):
        pass

    try:
        if float(dxy_chg) > 0 and float(dgs10_chg) > 0:
            score -= 1
            reasons.append("美元指数与 10Y 收益率同步上行，压制黄金")
        elif float(dxy_chg) < 0 and float(dgs10_chg) < 0:
            score += 1
            reasons.append("美元指数与 10Y 收益率同步回落，支撑黄金")
    except (TypeError, ValueError):
        pass

    try:
        if float(pce_yoy) > 3:
            score += 1
            reasons.append("核心 PCE 高于 3%，通胀粘性支撑黄金配置")
    except (TypeError, ValueError):
        pass

    try:
        if abs(float(gc_chg)) < 0.5:
            volatility = "低"
        elif abs(float(gc_chg)) > 2:
            volatility = "高"
        else:
            volatility = "正常"
    except (TypeError, ValueError):
        volatility = "正常"

    score = max(1, min(10, score))
    if score >= 7:
        bias = "偏多"
    elif score <= 4:
        bias = "偏空"
    else:
        bias = "观望"

    return {
        "bias": bias,
        "volatility": volatility,
        "score": score,
        "reason": "；".join(reasons[:3]) if reasons else "多因子信号分化，等待更明确方向",
    }


def build_report(data, generated_at):
    decision = infer_bias(data)
    report_date = generated_at.strftime("%Y-%m-%d %H:%M:%S")

    lines = [
        "# XAUUSD 量化交易完整报告",
        "",
        f"- 生成时间：{report_date}",
        f"- 宏观数据时间戳：{value_at(data, 'timestamp')}",
        "- 数据来源：Yahoo Finance / FRED / WGC-IMF / Google News RSS / MT5 MCP（交易步骤补充）",
        "",
        "## 1. 数据搜集状态",
        "",
        "| 维度 | 状态 | 关键说明 |",
        "|---|---:|---|",
    ]

    yfinance_source = value_at(data, "yfinance", "data_source")
    fred_source = value_at(data, "fred", "data_source")
    wgc_status = value_at(data, "wgc", "status")
    geo_status = value_at(data, "geopolitical", "status")
    lines.extend([
        f"| Yahoo Finance | {'可用' if yfinance_source != 'N/A' else 'N/A'} | {safe_table_cell(yfinance_source)} |",
        f"| FRED | {'可用' if fred_source != 'N/A' else 'N/A'} | {safe_table_cell(fred_source)} |",
        f"| WGC/IMF | {safe_table_cell(wgc_status)} | {safe_table_cell(value_at(data, 'wgc', 'note'))} |",
        f"| 地缘政治风险 | {safe_table_cell(geo_status)} | risk_score={fmt(value_at(data, 'geopolitical', 'risk_score'))}, risk_level={safe_table_cell(value_at(data, 'geopolitical', 'risk_level'))} |",
        "",
        "## 2. 市场价格与风险情绪",
        "",
        "| 指标 | 代码 | 最新值 | 变化 | 信号 |",
        "|---|---:|---:|---:|---|",
        market_row("VIX 恐慌指数", data, "^VIX"),
        market_row("SPDR 黄金 ETF", data, "GLD"),
        market_row("美元指数代理", data, "DX-Y.NYB"),
        market_row("COMEX 黄金期货", data, "GC=F"),
        market_row("标普500", data, "^GSPC"),
        market_row("道琼斯", data, "^DJI"),
        market_row("纳斯达克", data, "^IXIC"),
        "",
        "## 3. FRED 宏观因子",
        "",
        "| 指标 | 序列 | 最新值 | 变化 | 信号 |",
        "|---|---:|---:|---:|---|",
        fred_row("美联储官方美元指数", data, "DTWEXO"),
        fred_row("10Y 美债收益率", data, "DGS10", unit="%"),
        fred_row("联邦基金利率", data, "FEDFUNDS", unit="%"),
        fred_row("10Y 通胀预期", data, "T10YIE", unit="%"),
        fred_row("核心 PCE 同比", data, "PCEPILFE", field="yoy_pct", unit="%"),
        "",
    ])

    try:
        real_rate = float(value_at(data, "fred", "series", "DGS10", "latest_value")) - float(value_at(data, "fred", "series", "T10YIE", "latest_value"))
        lines.append(f"- 实际利率近似值：{real_rate:.2f}%（DGS10 - T10YIE）")
    except (TypeError, ValueError):
        lines.append("- 实际利率近似值：N/A")

    lines.extend([
        "",
        "## 4. 地缘政治风险",
        "",
        f"- 风险等级：{safe_table_cell(value_at(data, 'geopolitical', 'risk_level'))}",
        f"- 风险分数：{fmt(value_at(data, 'geopolitical', 'risk_score'))}/10",
        f"- 新闻样本数：{fmt(value_at(data, 'geopolitical', 'article_count'))}",
        f"- 命中关键词：`{safe_table_cell(json.dumps(value_at(data, 'geopolitical', 'keywords_found', default={}), ensure_ascii=False))}`",
        "",
        "| # | 标题 | 来源 | 分数 |",
        "|---:|---|---|---:|",
    ])

    headlines = value_at(data, "geopolitical", "top_headlines", default=[])
    if isinstance(headlines, list) and headlines:
        for idx, item in enumerate(headlines[:8], 1):
            lines.append(
                f"| {idx} | {safe_table_cell(item.get('title', 'N/A'))} | "
                f"{safe_table_cell(item.get('source', 'N/A'))} | {fmt(item.get('score'))} |"
            )
    else:
        lines.append("| 1 | N/A | N/A | N/A |")

    wgc_bg = value_at(data, "wgc", "background_knowledge", default={})
    lines.extend([
        "",
        "## 5. 央行购金与黄金结构性需求",
        "",
        f"- WGC 状态：{safe_table_cell(wgc_status)}",
        f"- 季度变化吨数：{fmt(value_at(data, 'wgc', 'quarterly_change_tonnes'))}",
        f"- 趋势方向：{safe_table_cell(value_at(data, 'wgc', 'trend_direction'))}",
    ])
    if isinstance(wgc_bg, dict) and wgc_bg:
        lines.extend([
            f"- 最新季度净购买：{fmt(wgc_bg.get('latest_quarter_net_purchases_tonnes'))} 吨",
            f"- 背景趋势：{safe_table_cell(wgc_bg.get('key_trend', 'N/A'))}",
        ])

    gld_info = value_at(data, "yfinance", "etf_info", "GLD", default={})
    futures = value_at(data, "yfinance", "gold_futures_detail", default={})
    lines.extend([
        "",
        "## 6. ETF 与 COMEX 持仓代理",
        "",
        "| 维度 | 数值 | 说明 |",
        "|---|---:|---|",
        f"| GLD total_assets | {fmt(gld_info.get('total_assets') if isinstance(gld_info, dict) else 'N/A')} | 黄金 ETF 资金规模代理 |",
        f"| GLD NAV | {fmt(gld_info.get('nav_price') if isinstance(gld_info, dict) else 'N/A')} | ETF 净值参考 |",
        f"| GC=F open_interest | {fmt(futures.get('open_interest') if isinstance(futures, dict) else 'N/A')} | COMEX 持仓代理 |",
        f"| GC=F volume | {fmt(futures.get('volume') if isinstance(futures, dict) else 'N/A')} | 期货成交活跃度 |",
        "",
        "## 7. 多因子决策矩阵",
        "",
        "| 因子 | 方向 | 对黄金影响 | 权重说明 |",
        "|---|---|---|---|",
        f"| 地缘政治 | {safe_table_cell(value_at(data, 'geopolitical', 'risk_level'))} | {'利多' if decision['score'] >= 6 else '中性'} | 高权重事件驱动 |",
        f"| VIX | {signal_from_change(value_at(data, 'yfinance', 'price_data', '^VIX', 'change_pct'))} | {'利多' if str(signal_from_change(value_at(data, 'yfinance', 'price_data', '^VIX', 'change_pct'))) == '上升' else '中性/利空'} | 避险情绪 |",
        f"| 美元指数 | {signal_from_change(value_at(data, 'fred', 'series', 'DTWEXO', 'change_pct'))} | {'利空' if str(signal_from_change(value_at(data, 'fred', 'series', 'DTWEXO', 'change_pct'))) == '上升' else '利多/中性'} | 美元计价压力 |",
        f"| 10Y 收益率 | {signal_from_change(value_at(data, 'fred', 'series', 'DGS10', 'change_pct'))} | {'利空' if str(signal_from_change(value_at(data, 'fred', 'series', 'DGS10', 'change_pct'))) == '上升' else '利多/中性'} | 实际利率代理 |",
        f"| 核心 PCE | {fmt(value_at(data, 'fred', 'series', 'PCEPILFE', 'yoy_pct'), '%')} | 通胀粘性参考 | 美联储政策约束 |",
        f"| 央行购金 | {safe_table_cell(wgc_status)} | 结构性支撑 | 小时级低权重 |",
        "",
        "## 8. 初始量化结论（交易执行前）",
        "",
        f"- 决策倾向：**{decision['bias']}**",
        f"- 波动环境：**{decision['volatility']}**",
        f"- 综合评分：**{decision['score']}/10**",
        f"- 核心依据：{decision['reason']}",
        "- 仓位比例建议：高波动 5%；正常波动 10%；低波动 10%；观望 0%。",
        "",
        "## 9. 交易执行记录",
        "",
        "> 本节由后续 `quant_run` 步骤使用 MT5 MCP 查询账户与持仓后追加。如果当前文档没有追加内容，说明交易执行步骤尚未完成或执行器未返回追加结果。",
        "",
    ])

    return "\n".join(lines).strip() + "\n"


def main():
    parser = argparse.ArgumentParser(description="Generate XAUUSD markdown report")
    parser.add_argument("--macro-json", required=True, help="Path to macro JSON file")
    parser.add_argument("--output-dir", default="data/reports/xauusd", help="Report output directory")
    parser.add_argument("--latest-name", default="latest_report.md", help="Latest report file name")
    args = parser.parse_args()

    data = load_json(args.macro_json)
    generated_at = datetime.now()
    report = build_report(data, generated_at)

    os.makedirs(args.output_dir, exist_ok=True)
    timestamp_name = f"xauusd_report_{generated_at.strftime('%Y%m%d_%H%M%S')}.md"
    timestamp_path = os.path.abspath(os.path.join(args.output_dir, timestamp_name))
    latest_path = os.path.abspath(os.path.join(args.output_dir, args.latest_name))

    for path in (timestamp_path, latest_path):
        with open(path, "w", encoding="utf-8") as f:
            f.write(report)

    print(f"REPORT_PATH={timestamp_path}")
    print(f"LATEST_REPORT_PATH={latest_path}")
    print("")
    print(report)


if __name__ == "__main__":
    main()
