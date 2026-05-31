#!/usr/bin/env python3
"""Append trading execution output to the XAUUSD markdown report."""

import argparse
import os
from datetime import datetime


def read_text(path):
    if not path or not os.path.exists(path):
        return ""
    with open(path, "r", encoding="utf-8") as f:
        return f.read()


def write_text(path, content):
    os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write(content)


def build_final_report(base_report, quant_output, latest_path, timestamp_path):
    base = base_report.rstrip()
    quant = quant_output.strip() or "N/A"
    generated_at = datetime.now().strftime("%Y-%m-%d %H:%M:%S")

    if "## 10. 最终状态汇报" in base or "## 11. 报告文件" in base:
        base = base.split("## 10. 最终状态汇报", 1)[0].rstrip()

    sections = [
        base,
        "",
        "## 10. 最终状态汇报",
        "",
        quant,
        "",
        "## 11. 报告文件",
        "",
        f"- 更新时间：{generated_at}",
        f"- 最新报告：{latest_path}",
    ]
    if timestamp_path:
        sections.append(f"- 本次报告：{timestamp_path}")
    return "\n".join(sections).rstrip() + "\n"


def main():
    parser = argparse.ArgumentParser(description="Finalize XAUUSD markdown report")
    parser.add_argument("--report-file", required=True, help="Path to latest report markdown")
    parser.add_argument("--quant-output-file", default="", help="Optional file containing quant step output")
    parser.add_argument("--quant-output", default="", help="Quant step output text")
    parser.add_argument("--timestamp-report", default="", help="Optional timestamp report file to update")
    args = parser.parse_args()

    base_report = read_text(args.report_file)
    quant_output = read_text(args.quant_output_file) if args.quant_output_file else args.quant_output
    final_report = build_final_report(base_report, quant_output, args.report_file, args.timestamp_report)

    write_text(args.report_file, final_report)
    if args.timestamp_report:
        write_text(args.timestamp_report, final_report)

    print(f"FINAL_REPORT_PATH={os.path.abspath(args.report_file)}")
    if args.timestamp_report:
        print(f"TIMESTAMP_REPORT_PATH={os.path.abspath(args.timestamp_report)}")
    print("")
    print(final_report)


if __name__ == "__main__":
    main()
