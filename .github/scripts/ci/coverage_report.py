"""Publish measured Rust, Python and R line coverage as a docs page and badge."""

import argparse
import json
import shutil
import xml.etree.ElementTree as ET
from pathlib import Path

REPORTS = {"Rust": "rust.lcov", "Python": "python.xml", "R": "r.xml"}


def checked_counts(covered, total):
    if total <= 0 or not 0 <= covered <= total:
        raise ValueError(
            "coverage must contain a positive line total and valid hit count"
        )
    return {"covered": covered, "total": total}


def read_cobertura(path):
    root = ET.parse(path).getroot()
    return checked_counts(
        int(root.attrib["lines-covered"]), int(root.attrib["lines-valid"])
    )


def read_lcov(path):
    lines = {}
    source = None
    for entry in path.read_text(encoding="utf-8").splitlines():
        if entry.startswith("SF:"):
            source = entry[3:]
        elif entry.startswith("DA:"):
            if not source:
                raise ValueError("LCOV line has no source file")
            number, hits, *_ = entry[3:].split(",")
            number, hits = int(number), int(hits)
            if number <= 0 or hits < 0:
                raise ValueError("invalid LCOV line or hit count")
            key = (source, number)
            lines[key] = lines.get(key, False) or hits > 0
        elif entry == "end_of_record":
            source = None
    return checked_counts(sum(lines.values()), len(lines))


def collect(directory):
    suites = {}
    for language, filename in REPORTS.items():
        path = directory / filename
        suites[language] = (
            read_lcov(path) if path.suffix == ".lcov" else read_cobertura(path)
        )
    total = checked_counts(
        sum(suite["covered"] for suite in suites.values()),
        sum(suite["total"] for suite in suites.values()),
    )
    return {"languages": suites, **total}


def percent(counts):
    return 100 * counts["covered"] / counts["total"]


def badge(counts=None):
    value = percent(counts) if counts else None
    message = f"{value:.1f}%" if value is not None else "pending"
    color = "#4c1" if value is not None and value >= 80 else "#dfb317"
    if value is None:
        color = "#9f9f9f"
    elif value < 60:
        color = "#e05d44"
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="140" height="20" role="img" aria-label="coverage: {message}">
<title>coverage: {message}</title>
<linearGradient id="shine" x2="0" y2="100%"><stop offset="0" stop-color="#fff" stop-opacity=".1"/><stop offset="1" stop-opacity=".1"/></linearGradient>
<clipPath id="rounded"><rect width="140" height="20" rx="3"/></clipPath>
<g clip-path="url(#rounded)"><path fill="#555" d="M0 0h76v20H0z"/><path fill="{color}" d="M76 0h64v20H76z"/><path fill="url(#shine)" d="M0 0h140v20H0z"/></g>
<g fill="#fff" text-anchor="middle" font-family="Verdana,Geneva,DejaVu Sans,sans-serif" font-size="11"><text x="38" y="14">coverage</text><text x="108" y="14">{message}</text></g>
</svg>
'''


def publish(output, data=None, *, commit=None, run_url=None):
    output.mkdir(parents=True, exist_ok=True)
    (output / "badge.svg").write_text(badge(data), encoding="utf-8")
    if data is None:
        page = """# Code coverage

Coverage is pending the first successful run of all Rust, Python and R test suites.

[Unit test runs](https://github.com/elong0527/yamaa/actions/workflows/unit-tests.yml)
"""
    else:
        (output / "summary.json").write_text(
            json.dumps({"commit": commit, "run_url": run_url, **data}, indent=2) + "\n",
            encoding="utf-8",
        )
        rows = []
        for language, counts in data["languages"].items():
            rows.append(
                f"| {language} | {counts['covered']:,} | {counts['total']:,} "
                f"| {percent(counts):.1f}% | [Raw report]({REPORTS[language]}) |"
            )
        page = f"""# Code coverage

Combined line coverage: **{percent(data):.1f}%** ({data["covered"]:,} of {data["total"]:,} reported lines).

| Language | Covered lines | Reported lines | Coverage | Download |
| --- | ---: | ---: | ---: | --- |
{chr(10).join(rows)}

The combined percentage divides total covered lines by total reported lines;
it does not average the three percentages.

Measured on Ubuntu 24.04 using the Rust shared crates (core, engine and adapters),
the Python 3.14 package and the R 4.6.1 package. Rust coverage comes from the debug
test run; the release test suites must also pass. The native installation workflow
separately checks the supported installation matrix.

Source: [{commit[:12]}](https://github.com/elong0527/yamaa/commit/{commit}).
All three suites passed in [this unit test run]({run_url}).
"""
    (output / "index.md").write_text(page, encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--commit")
    parser.add_argument("--run-url")
    parser.add_argument("--unavailable", action="store_true")
    args = parser.parse_args()
    if args.unavailable:
        publish(args.output)
    else:
        if not (args.input and args.commit and args.run_url):
            parser.error("measured coverage requires --input, --commit and --run-url")
        data = collect(args.input)
        publish(args.output, data, commit=args.commit, run_url=args.run_url)
        for filename in REPORTS.values():
            shutil.copy2(args.input / filename, args.output / filename)


if __name__ == "__main__":
    main()
