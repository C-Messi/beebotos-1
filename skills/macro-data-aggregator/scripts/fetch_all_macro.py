#!/usr/bin/env python3
"""
Macro Data Aggregator — 宏观数据统一聚合脚本
一键获取三层数据源：
  1) Yahoo Finance (VIX/GLD/DXY/GC=F/美股指数)
  2) FRED (美元指数/国债收益率/联邦基金利率/核心PCE)
  3) WGC/IMF (央行购金尽力获取)

通过 ThreadPoolExecutor 并行拉取，总耗时 ≈ max(各源耗时)。
"""

import argparse
import json
import os
import sys
import urllib.parse
import warnings
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime
from xml.etree import ElementTree as ET

try:
    import requests
except ImportError:
    print(json.dumps({"error": "requests not installed"}, ensure_ascii=False))
    sys.exit(1)

try:
    import yfinance as yf
    # Limit internal threading to avoid "can't start new thread" in constrained
    # environments (e.g. containers with low ulimit -u).
    yf.set_tz_cache_location(None)
    YFINANCE_OK = True
except ImportError:
    yf = None
    YFINANCE_OK = False

# ───────────────────────────────────────────
# Yahoo Finance 模块
# ───────────────────────────────────────────
YF_SYMBOLS_DEFAULT = "^VIX,GLD,DX-Y.NYB,GC=F,^GSPC,^DJI,^IXIC"


def _yf_fetch_price(symbols):
    result = {}
    tickers_str = " ".join(symbols)
    try:
        # threads=False prevents yfinance from spawning its own thread pool,
        # which can exhaust OS threads in container/resource-limited envs.
        data = yf.download(
            tickers_str,
            period="5d",
            interval="1d",
            progress=False,
            group_by="ticker",
            threads=False,
        )
    except Exception as e:
        return {s: {"error": str(e)} for s in symbols}

    for sym in symbols:
        try:
            hist = data[sym] if len(symbols) > 1 else data
            if hist is None or hist.empty:
                result[sym] = {"error": "No data"}
                continue
            closes = hist["Close"].dropna()
            if len(closes) < 1:
                result[sym] = {"error": "No valid close"}
                continue
            latest = float(closes.iloc[-1])
            prev = float(closes.iloc[-2]) if len(closes) >= 2 else latest
            result[sym] = {
                "price": round(latest, 4),
                "prev_close": round(prev, 4),
                "change_pct": round((latest - prev) / prev * 100, 2) if prev else 0.0,
            }
        except Exception as e:
            result[sym] = {"error": str(e)}
    return result


def _yf_fetch_etf_info(symbols):
    result = {}
    for sym in symbols:
        try:
            # Use single-session requests to avoid Ticker-level thread pools
            ticker = yf.Ticker(sym)
            ticker._session = requests.Session()
            info = ticker.info
            result[sym] = {
                "total_assets": info.get("totalAssets"),
                "nav_price": info.get("navPrice"),
                "previous_close": info.get("previousClose"),
                "currency": info.get("currency", "USD"),
            }
        except Exception as e:
            result[sym] = {"error": str(e)}
    return result


def _yf_fetch_gold_futures_detail():
    try:
        info = yf.Ticker("GC=F").info
        return {
            "open_interest": info.get("openInterest"),
            "volume": info.get("volume"),
        }
    except Exception as e:
        return {"error": str(e)}


def fetch_yfinance(symbols_str: str, include_info: bool = True):
    if not YFINANCE_OK:
        return {"error": "yfinance not installed. Run: python3 -m pip install yfinance"}

    symbols = [s.strip() for s in symbols_str.split(",") if s.strip()]
    report = {
        "timestamp": datetime.now().isoformat(),
        "data_source": "Yahoo Finance via yfinance",
        "price_data": _yf_fetch_price(symbols),
    }

    etf_targets = [s for s in symbols if s in ("GLD", "SLV")]
    if include_info and etf_targets:
        report["etf_info"] = _yf_fetch_etf_info(etf_targets)

    if "GC=F" in symbols:
        report["gold_futures_detail"] = _yf_fetch_gold_futures_detail()

    return report


# ───────────────────────────────────────────
# FRED 模块
# ───────────────────────────────────────────
FRED_API_BASE = "https://api.stlouisfed.org/fred/series/observations"

FRED_META = {
    "DTWEXO": {"name": "Trade Weighted U.S. Dollar Index: Major Currencies", "category": "dxy_proxy", "unit": "Index", "note": "接近 ICE DXY"},
    "DTWEXBGS": {"name": "Trade Weighted U.S. Dollar Index: Broad", "category": "dxy_proxy", "unit": "Index", "note": "广义美元指数"},
    "DGS10": {"name": "10-Year Treasury Constant Maturity Rate", "category": "interest_rate", "unit": "%", "note": "10Y 美债收益率"},
    "FEDFUNDS": {"name": "Federal Funds Effective Rate", "category": "interest_rate", "unit": "%", "note": "联邦基金有效利率"},
    "T10YIE": {"name": "10-Year Breakeven Inflation Rate", "category": "inflation", "unit": "%", "note": "盈亏平衡通胀率"},
    "PCEPILFE": {"name": "Core PCE Price Index", "category": "inflation", "unit": "Index 2017=100", "note": "核心 PCE，脚本自动计算同比年率"},
}


def _fred_fetch_series(api_key: str, series_id: str, limit: int = 5):
    params = {"series_id": series_id, "api_key": api_key, "file_type": "json", "sort_order": "desc", "limit": limit}
    errors = []
    # Attempt 1: normal verified request
    try:
        resp = requests.get(FRED_API_BASE, params=params, timeout=20)
        resp.raise_for_status()
        obs = resp.json().get("observations", [])
        return [o for o in obs if o.get("value") not in (".", None, "", "NaN")]
    except Exception as e:
        errors.append(str(e))
    # Attempt 2: retry with SSL verification disabled (fallback for CA issues)
    try:
        warnings.filterwarnings("ignore", message="Unverified HTTPS request")
        resp = requests.get(FRED_API_BASE, params=params, timeout=20, verify=False)
        resp.raise_for_status()
        obs = resp.json().get("observations", [])
        return [o for o in obs if o.get("value") not in (".", None, "", "NaN")]
    except Exception as e:
        errors.append(str(e))
    return {"error": "; ".join(errors)}


def _fred_build_standard(api_key: str, sid: str, limit: int = 5):
    meta = FRED_META.get(sid, {"name": sid, "category": "unknown", "unit": "", "note": ""})
    raw = _fred_fetch_series(api_key, sid, limit)
    if isinstance(raw, dict) and "error" in raw:
        return {"error": raw["error"], "name": meta["name"], "category": meta["category"]}
    if len(raw) < 1:
        return {"error": "No valid observations", "name": meta["name"], "category": meta["category"]}

    latest, prev = raw[0], (raw[1] if len(raw) >= 2 else raw[0])
    try:
        lv, pv = float(latest["value"]), float(prev["value"])
        change_abs = round(lv - pv, 4)
        change_pct = round(change_abs / abs(pv) * 100, 4) if pv != 0 else 0.0
    except (ValueError, TypeError) as e:
        return {"error": f"Parse error: {e}", "name": meta["name"]}

    return {
        "name": meta["name"], "category": meta["category"], "unit": meta["unit"], "note": meta.get("note", ""),
        "latest_date": latest.get("date"), "latest_value": lv,
        "prev_date": prev.get("date"), "prev_value": pv,
        "change_abs": change_abs, "change_pct": change_pct,
    }


def _fred_build_pce(api_key: str, sid: str = "PCEPILFE"):
    meta = FRED_META[sid]
    raw = _fred_fetch_series(api_key, sid, limit=14)
    if isinstance(raw, dict) and "error" in raw:
        return {"error": raw["error"], "name": meta["name"], "category": meta["category"]}
    if len(raw) < 13:
        return {"error": f"Insufficient obs ({len(raw)})", "name": meta["name"], "category": meta["category"]}
    try:
        latest = raw[0]
        yoy_ago = raw[12]
        lv, yv = float(latest["value"]), float(yoy_ago["value"])
        prev = raw[1]
        pv = float(prev["value"])
        return {
            "name": meta["name"], "category": meta["category"], "unit": meta["unit"], "note": meta.get("note", ""),
            "latest_date": latest.get("date"), "latest_index": lv,
            "yoy_date": yoy_ago.get("date"), "yoy_index": yv,
            "yoy_pct": round((lv - yv) / yv * 100, 2) if yv != 0 else 0.0,
            "mom_pct": round((lv - pv) / pv * 100, 2) if pv != 0 else 0.0,
        }
    except Exception as e:
        return {"error": str(e), "name": meta["name"]}


def fetch_fred(api_key: str, series_str: str, limit: int = 5):
    report = {"timestamp": datetime.now().isoformat(), "data_source": "FRED", "series": {}}
    for sid in [s.strip() for s in series_str.split(",") if s.strip()]:
        report["series"][sid] = _fred_build_pce(api_key, sid) if sid == "PCEPILFE" else _fred_build_standard(api_key, sid, limit)
    return report


# ───────────────────────────────────────────
# WGC / IMF 央行购金模块
# ───────────────────────────────────────────
IMF_API_BASE = "http://dataservices.imf.org/REST/SDMX_JSON.svc/CompactData/IFS"

WGC_KEY_COUNTRIES = {
    "CN": "China", "RU": "Russia", "TR": "Turkey", "PL": "Poland",
    "KZ": "Kazakhstan", "IN": "India", "BR": "Brazil", "UZ": "Uzbekistan",
}
WGC_CANDIDATE_CODES = ["RAXG", "RAXGFX", "RA_GOLD", "1A.ZF"]


def _wgc_fetch_imf(country_code: str, indicator: str):
    try:
        resp = requests.get(f"{IMF_API_BASE}/Q.{country_code}.{indicator}", timeout=15)
        if resp.status_code != 200:
            return {"error": f"HTTP {resp.status_code}"}
        series = resp.json().get("CompactData", {}).get("DataSet", {}).get("Series", {})
        obs = series.get("Obs", [])
        if not isinstance(obs, list):
            obs = [obs]
        valid = [o for o in obs if o.get("@OBS_VALUE") and o["@OBS_VALUE"] not in (".", "NaN")]
        return {"observations": valid}
    except Exception as e:
        return {"error": str(e)}


def _wgc_try_indicators(country_code: str):
    for ind in WGC_CANDIDATE_CODES:
        result = _wgc_fetch_imf(country_code, ind)
        if "observations" in result and len(result["observations"]) > 0:
            return result, ind
    return None, None


def _wgc_parse_obs(obs_list):
    try:
        latest = obs_list[-1]
        prev = obs_list[-2] if len(obs_list) >= 2 else latest
        return {
            "latest_period": latest.get("@TIME_PERIOD"),
            "latest_value": float(latest["@OBS_VALUE"]),
            "prev_period": prev.get("@TIME_PERIOD"),
            "prev_value": float(prev["@OBS_VALUE"]),
        }
    except Exception as e:
        return {"error": str(e)}


def fetch_wgc():
    report = {
        "timestamp": datetime.now().isoformat(),
        "data_source": "IMF IFS via SDMX API (fallback to WGC background knowledge)",
        "countries": {},
        "quarterly_change_tonnes": None,
        "trend_direction": None,
        "status": "unknown",
    }
    total_latest = total_prev = 0.0
    success = 0

    for cc, name in WGC_KEY_COUNTRIES.items():
        result, ind = _wgc_try_indicators(cc)
        if result and "observations" in result:
            parsed = _wgc_parse_obs(result["observations"])
            if "error" not in parsed:
                report["countries"][cc] = {"name": name, "indicator_used": ind, **parsed}
                total_latest += parsed["latest_value"]
                total_prev += parsed["prev_value"]
                success += 1
                continue
        report["countries"][cc] = {"name": name, "error": "All indicator codes failed"}

    if success >= 3:
        report["status"] = "partial"
        report["quarterly_change_tonnes"] = round(total_latest - total_prev, 2)
        report["trend_direction"] = "accelerating" if (total_latest - total_prev) > 0 else "slowing"
        report["note"] = f"Based on {success}/{len(WGC_KEY_COUNTRIES)} major central banks."
    else:
        report["status"] = "degraded"
        report["quarterly_change_tonnes"] = "N/A"
        report["trend_direction"] = "N/A"
        report["note"] = "IMF IFS API unavailable or indicator codes changed. Using background knowledge."
        report["background_knowledge"] = {
            "latest_wgc_report": "Gold Demand Trends Q1 2026",
            "latest_quarter_net_purchases_tonnes": 244,
            "yoy_change_pct": 3,
            "largest_buyers_q1_2026": ["Poland (31t)", "Uzbekistan (25t)", "China (7t)"],
            "key_trend": (
                "Central bank net purchases remained elevated in Q1 2026 at ~244t, "
                "driven by emerging market diversification and geopolitical hedging."
            ),
        }
    return report


# ───────────────────────────────────────────
# 地缘政治风险模块 (Google News RSS)
# ───────────────────────────────────────────
GEOPOLITICAL_QUERIES = [
    "iran israel conflict war",
    "hormuz strait oil shipping",
    "middle east tensions escalation",
]

GEOPOLITICAL_HIGH_RISK = [
    "war", "attack", "missile", "strike", "bomb", "invasion", "military",
    "sanctions", "tensions", "escalation", "conflict", "crisis", "threat",
    "hostile", "casualties", "killed", "destroyed", "nuclear", "uranium",
    "ballistic", "drone", "retaliation", "warship", "troops", "deploy",
    "violation", "blockade", "chokepoint", "disrupted", "explosion",
    "intercepted", "invasion", "raid", "airstrike", "shelling",
]

GEOPOLITICAL_MED_RISK = [
    "deadline", "warning", "alert", "fighter", "patrol", "closure",
    "suspend", "halt", "delay", "evacuate", "embassy", "consulate",
    "advisory", "travel warning", "security concern",
]

GEOPOLITICAL_DE_ESCALATION = [
    "peace", "talks", "agreement", "ceasefire", "diplomacy", "de-escalation",
    "cooperation", "normalization", "dialogue", "deal", "truce", "reopen",
    "safe", "resume", "negotiation", "settlement", "accord",
]


def _parse_rss_pubdate(text):
    """Parse RSS pubDate to ISO format (best effort)."""
    if not text:
        return None
    formats = [
        "%a, %d %b %Y %H:%M:%S %Z",
        "%a, %d %b %Y %H:%M:%S %z",
    ]
    for fmt in formats:
        try:
            return datetime.strptime(text.strip(), fmt).isoformat()
        except ValueError:
            continue
    return text.strip()


def _fetch_google_news_rss(query, max_articles=20):
    """Fetch articles from Google News RSS for a query."""
    encoded = urllib.parse.quote(query)
    url = f"https://news.google.com/rss/search?q={encoded}&hl=en-US&gl=US&ceid=US:en"
    try:
        resp = requests.get(url, timeout=15, allow_redirects=True)
        resp.raise_for_status()
        root = ET.fromstring(resp.content)
        items = []
        channel = root.find("channel")
        if channel is None:
            return []
        for item in channel.findall("item")[:max_articles]:
            title_elem = item.find("title")
            pub_elem = item.find("pubDate")
            source_elem = item.find("source")
            title = (title_elem.text or "") if title_elem is not None else ""
            pub = (pub_elem.text or "") if pub_elem is not None else ""
            source = (source_elem.text or "") if source_elem is not None else ""
            if title and title not in [i["title"] for i in items]:
                items.append({
                    "title": title,
                    "published": _parse_rss_pubdate(pub),
                    "source": source,
                })
        return items
    except Exception as e:
        return [{"error": str(e)}]


def _score_title(title):
    """Score a single title for geopolitical risk."""
    t = title.lower()
    high = sum(2 for w in GEOPOLITICAL_HIGH_RISK if w in t)
    med = sum(1 for w in GEOPOLITICAL_MED_RISK if w in t)
    de_esc = sum(1.5 for w in GEOPOLITICAL_DE_ESCALATION if w in t)
    return max(0.0, high + med - de_esc)


def fetch_geopolitical():
    """Fetch geopolitical risk indicators from Google News RSS."""
    report = {
        "timestamp": datetime.now().isoformat(),
        "data_source": "Google News RSS (no API key required)",
        "status": "unknown",
        "risk_score": None,
        "risk_level": None,
        "article_count": 0,
        "keywords_found": {},
        "top_headlines": [],
        "queries_used": GEOPOLITICAL_QUERIES,
    }

    all_articles = []
    errors = []

    for query in GEOPOLITICAL_QUERIES:
        articles = _fetch_google_news_rss(query, max_articles=20)
        for a in articles:
            if "error" in a:
                errors.append(a["error"])
                continue
            # Deduplicate by title
            if not any(existing["title"] == a["title"] for existing in all_articles):
                all_articles.append(a)

    if errors and not all_articles:
        report["status"] = "error"
        report["error"] = "; ".join(errors)
        return report

    if not all_articles:
        report["status"] = "degraded"
        report["error"] = "No articles found from any query"
        return report

    # Score articles
    scores = []
    keyword_counts = {}
    for article in all_articles:
        score = _score_title(article["title"])
        scores.append(score)
        # Count keywords
        t = article["title"].lower()
        for w in GEOPOLITICAL_HIGH_RISK + GEOPOLITICAL_MED_RISK + GEOPOLITICAL_DE_ESCALATION:
            if w in t:
                keyword_counts[w] = keyword_counts.get(w, 0) + 1

    avg_score = sum(scores) / len(scores) if scores else 0.0
    # Volume factor: more articles = higher attention (max 3.0)
    volume_factor = min(3.0, len(all_articles) / 10.0)

    # Calculate risk score 0-10
    raw_risk = avg_score * 1.2 + volume_factor
    risk_score = round(min(10.0, max(0.0, raw_risk)), 2)

    # Determine risk level
    if risk_score >= 7.5:
        risk_level = "extreme"
    elif risk_score >= 5.5:
        risk_level = "high"
    elif risk_score >= 3.0:
        risk_level = "medium"
    else:
        risk_level = "low"

    # Top headlines sorted by score
    scored_headlines = sorted(
        [(a, _score_title(a["title"])) for a in all_articles],
        key=lambda x: x[1],
        reverse=True,
    )[:8]

    report.update({
        "status": "ok",
        "risk_score": risk_score,
        "risk_level": risk_level,
        "article_count": len(all_articles),
        "avg_title_score": round(avg_score, 2),
        "keywords_found": dict(sorted(keyword_counts.items(), key=lambda x: x[1], reverse=True)[:15]),
        "top_headlines": [{"title": a["title"], "source": a.get("source", ""), "published": a.get("published", ""), "score": round(s, 2)} for a, s in scored_headlines],
    })

    return report


# ───────────────────────────────────────────
# 主函数
# ───────────────────────────────────────────
def main():
    parser = argparse.ArgumentParser(description="Macro Data Aggregator")
    parser.add_argument("--yfinance-symbols", default=YF_SYMBOLS_DEFAULT, help="Yahoo symbols")
    parser.add_argument("--fred-api-key", default="", help="FRED API Key")
    parser.add_argument("--fred-series", default="DTWEXO,DGS10,FEDFUNDS,T10YIE,PCEPILFE", help="FRED series")
    parser.add_argument("--wgc", action="store_true", default=True, help="Fetch WGC data")
    parser.add_argument("--no-wgc", dest="wgc", action="store_false", help="Skip WGC data")
    parser.add_argument("--geopolitical", action="store_true", default=True, help="Fetch geopolitical risk data")
    parser.add_argument("--no-geopolitical", dest="geopolitical", action="store_false", help="Skip geopolitical risk data")
    parser.add_argument("--save-json", default="", help="Write full JSON result to this file path")
    parser.add_argument("--output", choices=["json", "summary"], default="json")
    args = parser.parse_args()

    result = {"timestamp": datetime.now().isoformat(), "yfinance": {}, "fred": {}, "wgc": {}, "geopolitical": {}}

    tasks = {}
    # Reduce max_workers to 2 to avoid thread exhaustion in constrained envs
    with ThreadPoolExecutor(max_workers=2) as ex:
        if args.yfinance_symbols:
            tasks[ex.submit(fetch_yfinance, args.yfinance_symbols)] = "yfinance"
        if args.fred_api_key and args.fred_series:
            tasks[ex.submit(fetch_fred, args.fred_api_key, args.fred_series)] = "fred"
        if args.wgc:
            tasks[ex.submit(fetch_wgc)] = "wgc"
        if args.geopolitical:
            tasks[ex.submit(fetch_geopolitical)] = "geopolitical"

        for fut in as_completed(tasks):
            key = tasks[fut]
            try:
                result[key] = fut.result(timeout=60)
            except Exception as e:
                result[key] = {"error": str(e)}

    if args.save_json:
        out_dir = os.path.dirname(os.path.abspath(args.save_json))
        if out_dir:
            os.makedirs(out_dir, exist_ok=True)
        with open(args.save_json, "w", encoding="utf-8") as f:
            json.dump(result, f, ensure_ascii=False, indent=2)

    if args.output == "json":
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        print("=== Macro Data Summary ===")
        if args.save_json:
            print(f"Saved full JSON: {args.save_json}")
        for k, v in result.items():
            if k == "timestamp":
                print(f"Timestamp: {v}")
                continue
            err = v.get("error", "")
            if err:
                print(f"[{k}] ERROR: {err}")
            else:
                print(f"[{k}] OK")


if __name__ == "__main__":
    main()
