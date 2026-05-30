#!/usr/bin/env python3
"""
yfinance 宏观市场数据获取脚本
通过 Yahoo Finance 免费 API 获取 VIX、DXY、黄金 ETF/期货、美股指数等数据。
无需 API Key，不依赖网页抓取。
"""

import argparse
import json
import sys
from datetime import datetime, timedelta

try:
    import yfinance as yf
except ImportError:
    print(json.dumps(
        {"error": "yfinance not installed. Run: pip install yfinance"},
        ensure_ascii=False
    ))
    sys.exit(1)


SYMBOL_MAP = {
    "VIX": "^VIX",
    "GLD": "GLD",
    "DXY": "DX-Y.NYB",
    "GC": "GC=F",
    "SPX": "^GSPC",
    "DJI": "^DJI",
    "IXIC": "^IXIC",
    "TIPS10Y": "^TNX",   # 10Y Treasury Yield 作为实际利率粗略代理
    "SLV": "SLV",
}


def fetch_price_and_change(symbols: list[str]) -> dict:
    """
    获取最新价格、前收盘价、24h(实际为最近一个交易日)涨跌幅。
    yfinance 的 1d 数据为日级别，对宏观指标足够。
    """
    result = {}
    # 批量下载提高效率
    tickers_str = " ".join(symbols)
    data = yf.download(tickers_str, period="5d", interval="1d", progress=False, group_by="ticker")

    for sym in symbols:
        try:
            if len(symbols) == 1:
                hist = data
            else:
                hist = data[sym]

            if hist is None or hist.empty:
                result[sym] = {"error": "No data returned"}
                continue

            # 取最近两有效行
            closes = hist["Close"].dropna()
            if len(closes) < 1:
                result[sym] = {"error": "No valid close price"}
                continue

            latest = float(closes.iloc[-1])
            prev = float(closes.iloc[-2]) if len(closes) >= 2 else latest
            change_pct = round((latest - prev) / prev * 100, 2) if prev else 0.0

            result[sym] = {
                "price": round(latest, 4),
                "prev_close": round(prev, 4),
                "change_pct": change_pct,
            }
        except Exception as e:
            result[sym] = {"error": str(e)}

    return result


def fetch_etf_info(symbols: list[str]) -> dict:
    """获取 ETF 扩展信息，主要用于 GLD 持仓量等。"""
    result = {}
    for sym in symbols:
        try:
            t = yf.Ticker(sym)
            info = t.info
            result[sym] = {
                "total_assets": info.get("totalAssets"),
                "nav_price": info.get("navPrice"),
                "previous_close": info.get("previousClose"),
                "fifty_two_week_high": info.get("fiftyTwoWeekHigh"),
                "fifty_two_week_low": info.get("fiftyTwoWeekLow"),
                "currency": info.get("currency", "USD"),
            }
        except Exception as e:
            result[sym] = {"error": str(e)}
    return result


def fetch_gold_futures_detail() -> dict:
    """获取 COMEX 黄金期货 (GC=F) 的额外信息，如持仓量等。"""
    try:
        t = yf.Ticker("GC=F")
        info = t.info
        return {
            "open_interest": info.get("openInterest"),
            "volume": info.get("volume"),
            "fifty_two_week_high": info.get("fiftyTwoWeekHigh"),
            "fifty_two_week_low": info.get("fiftyTwoWeekLow"),
        }
    except Exception as e:
        return {"error": str(e)}


def build_report(symbols: list[str], include_info: bool = False) -> dict:
    """构建标准输出报告。"""
    report = {
        "timestamp": datetime.now().isoformat(),
        "data_source": "Yahoo Finance via yfinance",
        "price_data": fetch_price_and_change(symbols),
    }

    etf_targets = [s for s in symbols if s in ("GLD", "SLV")]
    if include_info and etf_targets:
        report["etf_info"] = fetch_etf_info(etf_targets)

    if "GC=F" in symbols:
        report["gold_futures_detail"] = fetch_gold_futures_detail()

    return report


def main():
    parser = argparse.ArgumentParser(
        description="Fetch macro market data from Yahoo Finance"
    )
    parser.add_argument(
        "--symbols",
        default="^VIX,GLD,DX-Y.NYB,GC=F,^GSPC,^DJI,^IXIC",
        help="Comma-separated Yahoo Finance ticker symbols",
    )
    parser.add_argument(
        "--type",
        choices=["price", "info", "all"],
        default="all",
        help="Data type: price=price+change only, info=ETF info, all=both",
    )
    parser.add_argument(
        "--aliases",
        action="store_true",
        help="Use short aliases (VIX, GLD, DXY...) in output keys instead of raw symbols",
    )

    args = parser.parse_args()
    raw_symbols = [s.strip() for s in args.symbols.split(",") if s.strip()]

    # 若用户传入别名，转换为 Yahoo 代码
    if args.aliases:
        mapped = []
        for s in raw_symbols:
            mapped.append(SYMBOL_MAP.get(s.upper(), s))
        raw_symbols = mapped

    include_info = args.type in ("info", "all")
    report = build_report(raw_symbols, include_info)

    # 可选：将键名映射回别名以便阅读
    if args.aliases:
        reverse_map = {v: k for k, v in SYMBOL_MAP.items()}
        if "price_data" in report:
            report["price_data"] = {
                reverse_map.get(k, k): v for k, v in report["price_data"].items()
            }
        if "etf_info" in report:
            report["etf_info"] = {
                reverse_map.get(k, k): v for k, v in report["etf_info"].items()
            }

    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
