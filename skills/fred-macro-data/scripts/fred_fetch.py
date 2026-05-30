#!/usr/bin/env python3
"""
FRED 宏观数据获取脚本
通过美联储经济数据 (Federal Reserve Economic Data) API 获取：
- 真实美元指数 (DTWEXO / DTWEXBGS)
- 10年期国债收益率 (DGS10) — 利率预期代理
- 联邦基金有效利率 (FEDFUNDS)
- 10年期盈亏平衡通胀率 (T10YIE)
- 核心 PCE 物价指数 (PCEPILFE) — 美联储首选通胀指标

无需网页抓取，直接调用官方 REST API。
注册免费 API Key: https://fred.stlouisfed.org/docs/api/api_key.html
"""

import argparse
import json
import sys
from datetime import datetime

try:
    import requests
except ImportError:
    print(
        json.dumps(
            {"error": "requests not installed. Run: python3 -m pip install requests"},
            ensure_ascii=False,
        )
    )
    sys.exit(1)


FRED_API_BASE = "https://api.stlouisfed.org/fred/series/observations"

SERIES_META = {
    "DTWEXO": {
        "name": "Trade Weighted U.S. Dollar Index: Major Currencies",
        "category": "dxy_proxy",
        "unit": "Index",
        "note": "接近 ICE DXY，但涵盖更多货币",
    },
    "DTWEXBGS": {
        "name": "Trade Weighted U.S. Dollar Index: Broad",
        "category": "dxy_proxy",
        "unit": "Index",
        "note": "广义美元指数",
    },
    "DGS10": {
        "name": "10-Year Treasury Constant Maturity Rate",
        "category": "interest_rate",
        "unit": "%",
        "note": "10年期美债收益率，作为利率预期和实际利率代理",
    },
    "DGS5": {
        "name": "5-Year Treasury Constant Maturity Rate",
        "category": "interest_rate",
        "unit": "%",
        "note": "5年期美债收益率",
    },
    "FEDFUNDS": {
        "name": "Federal Funds Effective Rate",
        "category": "interest_rate",
        "unit": "%",
        "note": "联邦基金有效利率，反映当前货币政策水平",
    },
    "T10YIE": {
        "name": "10-Year Breakeven Inflation Rate",
        "category": "inflation",
        "unit": "%",
        "note": "10年期盈亏平衡通胀率 = 名义收益率 - TIPS 收益率",
    },
    "PCEPILFE": {
        "name": "Personal Consumption Expenditures: Chain-type Price Index Less Food and Energy",
        "category": "inflation",
        "unit": "Index 2017=100",
        "note": "核心 PCE 物价指数，美联储首选通胀指标。脚本会自动计算同比年率",
    },
}


def fetch_series(api_key: str, series_id: str, limit: int = 5):
    """从 FRED API 获取单个序列的最新观测值。"""
    params = {
        "series_id": series_id,
        "api_key": api_key,
        "file_type": "json",
        "sort_order": "desc",
        "limit": limit,
    }
    try:
        resp = requests.get(FRED_API_BASE, params=params, timeout=20)
        resp.raise_for_status()
        data = resp.json()
        observations = data.get("observations", [])
        # FRED 用 "." 表示缺失值
        valid = [
            obs
            for obs in observations
            if obs.get("value") not in (".", None, "", "NaN")
        ]
        return valid
    except requests.exceptions.Timeout:
        return {"error": "FRED API request timed out after 20s"}
    except requests.exceptions.HTTPError as e:
        return {"error": f"HTTP error: {e.response.status_code} - {e.response.text[:200]}"}
    except Exception as e:
        return {"error": str(e)}


def build_standard_series(api_key: str, sid: str, limit: int = 5):
    """处理标准序列（价格/收益率类）。"""
    meta = SERIES_META.get(
        sid, {"name": sid, "category": "unknown", "unit": "", "note": ""}
    )
    raw = fetch_series(api_key, sid, limit=limit)

    if isinstance(raw, dict) and "error" in raw:
        return {"error": raw["error"], "name": meta["name"], "category": meta["category"]}

    if len(raw) < 1:
        return {
            "error": "No valid observations returned",
            "name": meta["name"],
            "category": meta["category"],
        }

    latest = raw[0]
    prev = raw[1] if len(raw) >= 2 else latest

    try:
        latest_val = float(latest["value"])
        prev_val = float(prev["value"]) if prev != latest else latest_val
        change_abs = round(latest_val - prev_val, 4)
        change_pct = (
            round(change_abs / abs(prev_val) * 100, 4) if prev_val != 0 else 0.0
        )
    except (ValueError, TypeError) as e:
        return {
            "error": f"Value parse error: {e}",
            "raw_latest": latest.get("value"),
            "name": meta["name"],
            "category": meta["category"],
        }

    return {
        "name": meta["name"],
        "category": meta["category"],
        "unit": meta["unit"],
        "note": meta.get("note", ""),
        "latest_date": latest.get("date"),
        "latest_value": latest_val,
        "prev_date": prev.get("date"),
        "prev_value": prev_val,
        "change_abs": change_abs,
        "change_pct": change_pct,
    }


def build_pce_series(api_key: str, sid: str = "PCEPILFE"):
    """特殊处理核心 PCE：获取 14 个月数据计算同比年率。"""
    meta = SERIES_META[sid]
    raw = fetch_series(api_key, sid, limit=14)

    if isinstance(raw, dict) and "error" in raw:
        return {"error": raw["error"], "name": meta["name"], "category": meta["category"]}

    if len(raw) < 13:
        return {
            "error": f"Insufficient observations for YoY calc (got {len(raw)}, need 13+)",
            "name": meta["name"],
            "category": meta["category"],
        }

    try:
        latest = raw[0]
        yoy_ago = raw[12]  # 大约 12 个月前（月度数据）
        latest_val = float(latest["value"])
        yoy_val = float(yoy_ago["value"])
        yoy_pct = round((latest_val - yoy_val) / yoy_val * 100, 2) if yoy_val != 0 else 0.0

        # 最近一期环比
        prev = raw[1]
        prev_val = float(prev["value"])
        mom_pct = round((latest_val - prev_val) / prev_val * 100, 2) if prev_val != 0 else 0.0
    except (ValueError, TypeError, IndexError) as e:
        return {
            "error": f"YoY calc error: {e}",
            "name": meta["name"],
            "category": meta["category"],
        }

    return {
        "name": meta["name"],
        "category": meta["category"],
        "unit": meta["unit"],
        "note": meta.get("note", ""),
        "latest_date": latest.get("date"),
        "latest_index": latest_val,
        "yoy_date": yoy_ago.get("date"),
        "yoy_index": yoy_val,
        "yoy_pct": yoy_pct,
        "mom_pct": mom_pct,
    }


def build_report(api_key: str, series_list: list[str], limit: int = 5):
    """构建标准化报告。"""
    report = {
        "timestamp": datetime.now().isoformat(),
        "data_source": "FRED (Federal Reserve Economic Data)",
        "api_base": FRED_API_BASE,
        "series": {},
    }

    for sid in series_list:
        if sid == "PCEPILFE":
            report["series"][sid] = build_pce_series(api_key, sid)
        else:
            report["series"][sid] = build_standard_series(api_key, sid, limit=limit)

    return report


def main():
    parser = argparse.ArgumentParser(
        description="Fetch macro economic data from FRED API"
    )
    parser.add_argument(
        "--api-key",
        required=True,
        help="FRED API Key (get free key at https://fred.stlouisfed.org/docs/api/api_key.html)",
    )
    parser.add_argument(
        "--series",
        default="DTWEXO,DGS10,FEDFUNDS,T10YIE,PCEPILFE",
        help="Comma-separated FRED series IDs",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=5,
        help="Number of recent observations to fetch per series (default: 5). PCEPILFE always uses 14 for YoY calc.",
    )

    args = parser.parse_args()
    series_ids = [s.strip() for s in args.series.split(",") if s.strip()]

    report = build_report(args.api_key, series_ids, limit=args.limit)
    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
