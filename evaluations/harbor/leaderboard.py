"""Record Harbor agent runs and render the site's leaderboards.

    python evaluations/harbor/leaderboard.py add ~/.cache/yamaa-harbor/jobs/<job>
    python evaluations/harbor/leaderboard.py render [--check]
    python evaluations/harbor/leaderboard.py export <leaderboard> --package <org/name>

The leaderboards follow Harbor Hub's curated-leaderboard model. Each
`leaderboards/<name>.yaml` names the tasks and attempts a run must cover and,
under `harbor:`, the Hub definition: the metadata and metrics every row
carries, the columns, and the ordered `rank_by` rules.

`add` records one Harbor job as `results/<job>.json`: the per-trial facts and
trial ids a row is built from, and the leaderboards the run belongs to. A
row's metrics aggregate its trials as Harbor does: the mean counts an errored
trial as 0, and pass@k is Harbor's unbiased estimator averaged over tasks.
`render` rebuilds `docs/articles/leaderboard.md`, ranking rows with Harbor's
own comparison; `--check` fails when the committed page is out of date.
`export` writes the configs `harbor hub leaderboard create` and
`harbor hub leaderboard row create` take.
"""

from __future__ import annotations

import argparse
import functools
import json
import os
import re
import sys
import textwrap
from collections import Counter, defaultdict
from datetime import datetime
from pathlib import Path

import yaml

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
RESULTS = HERE / "results"
LEADERBOARDS = HERE / "leaderboards"
PAGE = ROOT / "docs" / "articles" / "leaderboard.md"
REPOSITORY = "https://github.com/elong0527/yamaa/blob/main"
HUB_DOCS = "https://docs.harborframework.com/core-concepts/harbor-hub/leaderboards"
EXPORT = (
    Path(os.environ.get("XDG_CACHE_HOME") or Path.home() / ".cache")
    / "yamaa-harbor"
    / "hub"
)
FORMATS = ("percent", "count", "usd")
PHASE_MULTIPLIERS = (
    "agent_timeout_multiplier",
    "verifier_timeout_multiplier",
    "agent_setup_timeout_multiplier",
    "environment_build_timeout_multiplier",
)


def _seconds(phase: dict | None) -> float | None:
    if not phase or not phase.get("started_at") or not phase.get("finished_at"):
        return None
    start = datetime.fromisoformat(phase["started_at"])
    return (datetime.fromisoformat(phase["finished_at"]) - start).total_seconds()


def _read_json(path: Path) -> dict:
    return json.loads(path.read_text()) if path.is_file() else {}


def _default_timeouts(config: dict) -> bool:
    return config.get("timeout_multiplier", 1.0) == 1.0 and all(
        config.get(name) in (None, 1.0) for name in PHASE_MULTIPLIERS
    )


def collect(job_dir: Path) -> dict:
    """The facts of one Harbor job that a leaderboard row is built from."""
    trials = []
    agents = set()
    default_timeouts = _default_timeouts(_read_json(job_dir / "config.json"))
    for path in sorted(job_dir.glob("*/result.json")):
        result = json.loads(path.read_text())
        config = result.get("config") or {}
        info = result.get("agent_info") or {}
        model = (config.get("agent") or {}).get("model_name")
        agents.add((info.get("name"), info.get("version"), model))
        default_timeouts = default_timeouts and _default_timeouts(config)
        rewards = (result.get("verifier_result") or {}).get("rewards") or {}
        usage = result.get("agent_result") or {}
        error = result.get("exception_info") or {}
        trials.append(
            {
                "id": result.get("id"),
                "name": result.get("trial_name"),
                "benchmark": result["task_name"].rpartition("/")[2],
                "reward": float(rewards.get("reward", 0.0)),
                "cell_accuracy": float(rewards.get("cell_accuracy", 0.0)),
                "row_accuracy": float(rewards.get("row_accuracy", 0.0)),
                "web_tool_calls": float(rewards.get("web_tool_calls", 0.0)),
                "input_tokens": usage.get("n_input_tokens"),
                "output_tokens": usage.get("n_output_tokens"),
                "cost_usd": usage.get("cost_usd"),
                "agent_seconds": _seconds(result.get("agent_execution")),
                "error": error.get("exception_type"),
            }
        )
    if not trials:
        raise SystemExit(f"{job_dir} holds no trial results")
    if len(agents) != 1:
        raise SystemExit(f"{job_dir} mixes agents or models: {sorted(agents)}")
    ((agent, version, model),) = agents
    job = json.loads((job_dir / "result.json").read_text())
    attempts = _read_json(job_dir / "config.json").get("n_attempts")
    lock = _read_json(job_dir / "lock.json")
    return {
        "job": job_dir.name,
        "job_id": job.get("id"),
        "date": job["started_at"][:10],
        "harbor_version": (lock.get("harbor") or {}).get("version"),
        "agent": agent,
        "agent_version": version,
        "model": model,
        "attempts": attempts or min(Counter(t["benchmark"] for t in trials).values()),
        "default_timeouts": default_timeouts,
        "status": "display",
        "leaderboards": [],
        "trials": sorted(trials, key=lambda t: (t["benchmark"], t["name"] or "")),
    }


def _declared(schema: dict, key: str) -> bool:
    return key in schema.get("properties", {}) or any(
        re.fullmatch(pattern, key) for pattern in schema.get("patternProperties", {})
    )


def board_problems(board: dict) -> list[str]:
    """What keeps a leaderboard definition from rendering or exporting."""
    harbor = board["harbor"]
    schemas = {
        "metadata": harbor["metadata_schema"],
        "metrics": harbor["metrics_schema"],
    }
    found = []
    if not board["tasks"] or board["attempts"] < 1:
        found.append("needs at least one task and one attempt")
    ids = {column["id"] for column in harbor["columns"]}
    for accessor in [c["accessor"] for c in harbor["columns"]] + [
        r["accessor"] for r in harbor["rank_by"]
    ]:
        root, _, key = accessor.partition(".")
        if not _declared(schemas[root], key):
            found.append(f"{accessor} is not in the {root} schema")
    for column, style in board.get("formats", {}).items():
        if column not in ids or style not in FORMATS:
            found.append(f"format {column}: {style} names no column or format")
    return found


def load_leaderboards() -> list[dict]:
    boards = []
    for path in sorted(LEADERBOARDS.glob("*.yaml")):
        board = yaml.safe_load(path.read_text(encoding="utf-8"))
        found = board_problems(board)
        if board["harbor"]["name"] != path.stem:
            found.append(f"harbor.name must be {path.stem!r}")
        if found:
            raise SystemExit(f"{path.name}: " + "; ".join(found))
        boards.append(board)
    return boards


def load_results() -> list[dict]:
    return [json.loads(p.read_text()) for p in sorted(RESULTS.glob("*.json"))]


def run_problems(run: dict, board: dict) -> list[str]:
    """Why a run cannot be a row of a leaderboard; empty when it can."""
    counts = Counter(t["benchmark"] for t in run["trials"])
    found = [
        f"{task} has {counts[task]} of {board['attempts']} attempts"
        for task in board["tasks"]
        if counts[task] < board["attempts"]
    ]
    if not run["default_timeouts"]:
        found.append("the job changed the tasks' timeouts")
    return found


def _eligible_k(fewest: int) -> list[int]:
    ks = set()
    k = 2
    while k <= fewest:
        ks.add(k)
        k *= 2
    k = 5
    while k <= fewest:
        ks.add(k)
        k += 5
    return sorted(ks)


def _pass_at_k_for_task(n: int, c: int, k: int) -> float:
    if n - c < k:
        return 1.0
    product = 1.0
    for i in range(k):
        product *= (n - c - i) / (n - i)
    return 1.0 - product


def pass_at_k(trials: list[dict]) -> dict[int, float]:
    """Harbor's pass@k: the unbiased estimate per task, averaged over tasks.

    Harbor skips pass@k when a verifier writes several rewards, as grade.py
    does, so it is computed here from `reward` alone, with Harbor's k values:
    2, 4, 8, ... and 5, 10, ... up to the fewest attempts on any task.
    """
    successes: dict[str, list[int]] = defaultdict(list)
    for trial in trials:
        if trial["reward"] not in (0.0, 1.0):
            return {}
        successes[trial["benchmark"]].append(int(trial["reward"]))
    if not successes:
        return {}
    fewest = min(len(s) for s in successes.values())
    return {
        k: sum(_pass_at_k_for_task(len(s), sum(s), k) for s in successes.values())
        / len(successes)
        for k in _eligible_k(fewest)
    }


def _mean(values: list[float]) -> float:
    return sum(values) / len(values)


def _total(trials: list[dict], field: str) -> float | None:
    values = [t[field] for t in trials]
    return None if any(v is None for v in values) else sum(values)


def metrics(trials: list[dict]) -> dict:
    """Harbor's aggregation of the trials' rewards, plus usage totals."""
    seconds = [t["agent_seconds"] for t in trials]
    result = {
        "reward": _mean([t["reward"] for t in trials]),
        "cell_accuracy": _mean([t["cell_accuracy"] for t in trials]),
        "row_accuracy": _mean([t["row_accuracy"] for t in trials]),
        "n_trials": len(trials),
        "n_errors": sum(t["error"] is not None for t in trials),
        "input_tokens": _total(trials, "input_tokens"),
        "output_tokens": _total(trials, "output_tokens"),
        "cost_usd": _total(trials, "cost_usd"),
        "agent_seconds": None if None in seconds else _mean(seconds),
    }
    for k, value in pass_at_k(trials).items():
        result[f"pass_at_{k}"] = value
    return result


def rows_for(board: dict, runs: list[dict]) -> list[dict]:
    """The leaderboard's rows, one per run that names it, in Hub row shape
    plus the trials they aggregate."""
    name = board["harbor"]["name"]
    tasks = set(board["tasks"])
    rows, seen = [], set()
    for run in runs:
        if name not in run["leaderboards"]:
            continue
        found = run_problems(run, board)
        if found:
            raise SystemExit(f"{run['job']} on {name}: " + "; ".join(found))
        trials = [t for t in run["trials"] if t["benchmark"] in tasks]
        trial_ids = [t["id"] for t in trials if t["id"]]
        if seen.intersection(trial_ids):
            raise SystemExit(f"{run['job']} on {name}: a trial is on two rows")
        seen.update(trial_ids)
        record = f"{REPOSITORY}/evaluations/harbor/results/{run['job']}.json"
        rows.append(
            {
                "metadata": {
                    "agent": run["agent"],
                    "agent_version": run["agent_version"],
                    "model": run["model"],
                    "date": run["date"],
                    "attempts": run["attempts"],
                    "job": run["job"],
                    "job_id": run["job_id"],
                    "harbor_version": run["harbor_version"],
                    "record": {"url": record, "label": run["job"]},
                },
                "metrics": metrics(trials),
                "status": run["status"],
                "trial_ids": trial_ids,
                "trials": trials,
            }
        )
    return rows


def _sortable(value: object) -> float | str | None:
    if isinstance(value, bool):
        return float(value)
    if isinstance(value, (int, float)):
        return float(value)
    if isinstance(value, str):
        return value.lower()
    return None


def _value(row: dict, accessor: str) -> object:
    root, _, key = accessor.partition(".")
    return row[root].get(key)


def rank(rows: list[dict], rank_by: list[dict]) -> list[dict]:
    """Order rows as Harbor Hub does: rule by rule, nulls last unless the
    rule says first, and mixed number and string values compared as text."""

    def compare(a: dict, b: dict) -> int:
        for rule in rank_by:
            av = _sortable(_value(a, rule["accessor"]))
            bv = _sortable(_value(b, rule["accessor"]))
            if av is None and bv is None:
                continue
            nulls_first = rule.get("nulls") == "first"
            if av is None:
                return -1 if nulls_first else 1
            if bv is None:
                return 1 if nulls_first else -1
            if isinstance(av, float) and isinstance(bv, float):
                if av == bv:
                    continue
                result = -1 if av < bv else 1
            else:
                if str(av) == str(bv):
                    continue
                result = -1 if str(av) < str(bv) else 1
            return -result if rule["direction"] == "desc" else result
        return 0

    return sorted(rows, key=functools.cmp_to_key(compare))


def _tokens(value: float) -> str:
    return f"{value / 1000:.1f}k" if value >= 1000 else f"{value:.0f}"


def _cost(value: float) -> str:
    return f"${value:.2f}" if value >= 1 else f"${value:.3f}"


def _cell(value: object, column: dict, style: str | None) -> str:
    if value is None:
        return "n/a"
    kind = column["type"]
    if kind == "link":
        if isinstance(value, dict):
            return f"[{value['label']}]({value['url']})"
        return f"<{value}>"
    if kind == "boolean":
        return "yes" if value else "no"
    if kind == "number":
        if style == "percent":
            return f"{value:.1%}"
        if style == "usd":
            return _cost(value)
        if style == "count":
            return _tokens(value)
        return f"{value:g}"
    text = str(value)
    return text if kind == "markdown" else text.replace("|", "\\|")


def _align(column: dict) -> str:
    align = column.get("align") or ("right" if column["type"] == "number" else "left")
    return {"left": "---", "center": ":---:", "right": "---:"}[align]


def _ranking(board: dict) -> str:
    headers = {c["accessor"]: c["header"] for c in board["harbor"]["columns"]}
    rules = [
        f"{headers.get(r['accessor'], r['accessor']).lower()} "
        f"({'highest' if r['direction'] == 'desc' else 'lowest'} first)"
        for r in board["harbor"]["rank_by"]
    ]
    return "Rows are ranked by " + ", then ".join(rules) + "."


def _wrap(text: str) -> str:
    return textwrap.fill(text, width=72, break_long_words=False, break_on_hyphens=False)


def _board_section(board: dict, rows: list[dict]) -> list[str]:
    harbor = board["harbor"]
    tasks = ", ".join(f"[{t}](../benchmark/{t}.html)" for t in board["tasks"])
    attempts = board["attempts"]
    rule = (
        f"Tasks: {tasks}. A run is ranked here when it makes at least "
        f"{attempts} attempt{'s' if attempts > 1 else ''} on each task at the "
        f"tasks' own timeouts. {_ranking(board)}"
    )
    lines = [f"## {harbor['title']}", "", _wrap(harbor["description"]), ""]
    lines += [_wrap(rule), ""]
    rows = [r for r in rows if r["status"] == "display"]
    if not rows:
        return [*lines, "No runs are recorded on this leaderboard yet.", ""]

    columns = harbor["columns"]
    formats = board.get("formats", {})
    lines += [
        "| # | " + " | ".join(c["header"] for c in columns) + " | Trials |",
        "|---:|" + "".join(f"{_align(c)}|" for c in columns) + "---:|",
    ]
    for position, row in enumerate(rows, 1):
        cells = [
            _cell(_value(row, c["accessor"]), c, formats.get(c["id"])) for c in columns
        ]
        lines.append(
            f"| {position} | " + " | ".join(cells) + f" | {len(row['trials'])} |"
        )

    headers = [
        f"#{position} `{row['metadata']['model'].rpartition('/')[2]}`"
        for position, row in enumerate(rows, 1)
    ]
    lines += [
        "",
        "Each cell below is the task result and the share of golden cells the",
        "agent reproduced; with several attempts, the passes out of the attempts.",
        "",
        "| Benchmark | " + " | ".join(headers) + " |",
        "|---|" + "---|" * len(headers),
    ]
    for benchmark in board["tasks"]:
        cells = []
        for row in rows:
            trials = [t for t in row["trials"] if t["benchmark"] == benchmark]
            passed = sum(t["reward"] == 1.0 for t in trials)
            accuracy = _mean([t["cell_accuracy"] for t in trials])
            verdict = "**pass**" if passed == len(trials) else "fail"
            if len(trials) > 1:
                verdict = f"{passed} / {len(trials)} pass"
            errors = sorted({t["error"] for t in trials if t["error"]})
            note = f" ({', '.join(errors)})" if errors else ""
            cells.append(f"{verdict}, {accuracy:.1%}{note}")
        link = f"[{benchmark}](../benchmark/{benchmark}.html)"
        lines.append(f"| {link} | " + " | ".join(cells) + " |")
    return [*lines, ""]


def render(boards: list[dict], runs: list[dict]) -> str:
    names = {b["harbor"]["name"] for b in boards}
    for run in runs:
        unknown = sorted(set(run["leaderboards"]) - names)
        if unknown:
            raise SystemExit(f"{run['job']} names unknown leaderboards: {unknown}")
    lines = [
        "---",
        "title: Leaderboard",
        "hide:",
        "  - actions",
        "---",
        "",
        "# Agent leaderboards",
        "",
        "How well AI coding agents turn a benchmark's request and input datasets",
        "into the requested dataset. Each agent gets only the benchmark's",
        "`prompt.md` and its input files, in a sandbox that reaches nothing",
        "but the model API, and its output is graded cell by cell against the",
        "benchmark's golden file. A task passes when every cell matches.",
        f"[The evaluation]({REPOSITORY}/evaluations/harbor/README.md) runs on",
        "[Harbor](https://github.com/harbor-framework/harbor); how each request",
        "is written is in",
        f"[the benchmark prompt recipe]({REPOSITORY}/automation/benchmark_prompt.md).",
        "",
        "Each leaderboard below is defined the way",
        f"[Harbor Hub leaderboards]({HUB_DOCS}) are: a fixed set of tasks, the",
        "metadata and metrics each row carries, the columns shown, and ordered",
        "ranking rules. A row is one recorded run, and its metrics aggregate the",
        "run's trials as Harbor does, so an errored trial counts as a failure.",
        "",
    ]
    for board in boards:
        rows = rank(rows_for(board, runs), board["harbor"]["rank_by"])
        lines += _board_section(board, rows)
    lines += [
        "## Reading the results",
        "",
        "- **Attempts.** A leaderboard that asks for one attempt per task is a",
        "  pilot: a single pass or failure says little about a model on its",
        "  own. With several attempts, rows also carry Harbor's pass@k.",
        "- **Public data.** The inputs and golden files are published in this",
        "  repository, so a model may have seen them in training; the sandbox",
        "  only rules out looking them up during the run.",
        "- **Contributor models.** A model whose name ends in `-contributor`",
        "  runs at a discount in exchange for its provider training on the",
        "  prompts and answers, which may inflate that provider's later scores.",
        "",
    ]
    return "\n".join(lines)


def export(board: dict, runs: list[dict], package: str, out: Path) -> list[Path]:
    """Write the Hub create configs for one leaderboard and its rows."""
    name = board["harbor"]["name"]
    out.mkdir(parents=True, exist_ok=True)
    definition = out / f"{name}.leaderboard.yaml"
    definition.write_text(
        yaml.safe_dump({"package": package, **board["harbor"]}, sort_keys=False)
    )
    written = [definition]
    rows = rank(rows_for(board, runs), board["harbor"]["rank_by"])
    if rows:
        hub_rows = [
            {key: row[key] for key in ("metadata", "metrics", "status", "trial_ids")}
            for row in rows
        ]
        rows_path = out / f"{name}.rows.yaml"
        rows_path.write_text(yaml.safe_dump({"rows": hub_rows}, sort_keys=False))
        written.append(rows_path)
    return written


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    add = commands.add_parser("add", help="record a Harbor job, then render")
    add.add_argument("job_dir", type=Path)
    add.add_argument(
        "--leaderboard",
        action="append",
        help="leaderboard to rank the run on (repeatable); default: every one "
        "it qualifies for",
    )
    add.add_argument("--hide", action="store_true", help="record the run hidden")
    check = commands.add_parser("render", help="rebuild the leaderboard page")
    check.add_argument("--check", action="store_true")
    hub = commands.add_parser("export", help="write Harbor Hub create configs")
    hub.add_argument("leaderboard")
    hub.add_argument("--package", required=True, help="Hub dataset package org/name")
    hub.add_argument("--out", type=Path, default=EXPORT)
    args = parser.parse_args()

    boards = load_leaderboards()
    by_name = {b["harbor"]["name"]: b for b in boards}
    if args.command == "export":
        if args.leaderboard not in by_name:
            sys.exit(f"no leaderboard {args.leaderboard!r}")
        board = by_name[args.leaderboard]
        paths = export(board, load_results(), args.package, args.out)
        for path in paths:
            print(f"wrote {path}")
        ref = f"{args.package}/{args.leaderboard}"
        print("upload each job first: harbor upload <job-dir>")
        print(f"then: harbor hub leaderboard create --config {paths[0]}")
        if len(paths) > 1:
            print(f"then: harbor hub leaderboard row create {ref} --config {paths[1]}")
        return

    if args.command == "add":
        run = collect(args.job_dir)
        wanted = args.leaderboard or list(by_name)
        unknown = sorted(set(wanted) - set(by_name))
        if unknown:
            sys.exit(f"no leaderboard {unknown}")
        problems = {n: run_problems(run, by_name[n]) for n in wanted}
        summary = "; ".join(f"{n}: {', '.join(p)}" for n, p in problems.items() if p)
        run["leaderboards"] = [n for n in wanted if not problems[n]]
        if not run["leaderboards"] or (args.leaderboard and summary):
            sys.exit(f"the run cannot be ranked: {summary}")
        run["status"] = "hide" if args.hide else "display"
        RESULTS.mkdir(exist_ok=True)
        path = RESULTS / f"{run['job']}.json"
        path.write_text(json.dumps(run, indent=2) + "\n")
        print(f"wrote {path.relative_to(ROOT)} ({', '.join(run['leaderboards'])})")
    page = render(boards, load_results())
    if getattr(args, "check", False):
        if not PAGE.is_file() or PAGE.read_text() != page:
            sys.exit(f"{PAGE.relative_to(ROOT)} is out of date; run render")
        return
    PAGE.write_text(page)
    print(f"wrote {PAGE.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
