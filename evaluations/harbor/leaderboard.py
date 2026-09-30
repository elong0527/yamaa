"""Record Harbor agent runs and render the site's leaderboard page.

    python evaluations/harbor/leaderboard.py add evaluations/harbor/jobs/<job>
    python evaluations/harbor/leaderboard.py render [--check]

`add` reads a Harbor job directory and writes `results/<job>.json`, the
per-trial facts the page needs: benchmark, agent, model, rewards, tokens,
cost, and time. `render` rebuilds `docs/articles/leaderboard.md` from every
results file; `--check` fails when the committed page is out of date.
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import defaultdict
from datetime import datetime
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
RESULTS = HERE / "results"
PAGE = ROOT / "docs" / "articles" / "leaderboard.md"
REPOSITORY = "https://github.com/elong0527/yamaa/blob/main"


def _seconds(phase: dict | None) -> float | None:
    if not phase or not phase.get("started_at") or not phase.get("finished_at"):
        return None
    start = datetime.fromisoformat(phase["started_at"])
    return (datetime.fromisoformat(phase["finished_at"]) - start).total_seconds()


def collect(job_dir: Path) -> dict:
    """The facts of one Harbor job that the leaderboard shows."""
    trials = []
    agents = set()
    for path in sorted(job_dir.glob("*/result.json")):
        result = json.loads(path.read_text())
        info = result.get("agent_info") or {}
        model = (result["config"]["agent"] or {}).get("model_name")
        agents.add((info.get("name"), info.get("version"), model))
        rewards = (result.get("verifier_result") or {}).get("rewards") or {}
        usage = result.get("agent_result") or {}
        error = result.get("exception_info") or {}
        trials.append(
            {
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
    return {
        "job": job_dir.name,
        "date": job["started_at"][:10],
        "agent": agent,
        "agent_version": version,
        "model": model,
        "trials": sorted(trials, key=lambda t: t["benchmark"]),
    }


def load_results() -> list[dict]:
    return [json.loads(p.read_text()) for p in sorted(RESULTS.glob("*.json"))]


def _tokens(value: float) -> str:
    return f"{value / 1000:.1f}k" if value >= 1000 else f"{value:.0f}"


def _cost(value: float | None) -> str:
    if value is None:
        return "n/a"
    return f"${value:.2f}" if value >= 1 else f"${value:.3f}"


def _sum(trials: list[dict], field: str) -> float | None:
    values = [t[field] for t in trials]
    return None if any(v is None for v in values) else sum(values)


def render(results: list[dict]) -> str:
    lines = [
        "---",
        "title: Leaderboard",
        "hide:",
        "  - actions",
        "---",
        "",
        "# Agent leaderboard",
        "",
        "How well AI coding agents turn a benchmark's request and input datasets",
        "into the requested dataset. Each agent gets only the benchmark's",
        "`instruction.md` and its input files, in a sandbox that reaches nothing",
        "but the model API, and its output is graded cell by cell against the",
        "benchmark's golden file. A task passes when every cell matches.",
        f"[The evaluation]({REPOSITORY}/evaluations/harbor/README.md) runs on",
        "[Harbor](https://github.com/harbor-framework/harbor); how each request",
        "is written is in",
        f"[the benchmark prompt recipe]({REPOSITORY}/automation/benchmark_prompt.md).",
        "",
    ]
    if not results:
        return "\n".join([*lines, "No agent runs are recorded yet.", ""])

    lines += [
        "## Models",
        "",
        "| Agent | Model | Passed | Cell accuracy | Tokens in / out | Cost | Run |",
        "|---|---|---|---|---|---|---|",
    ]
    for run in results:
        trials = run["trials"]
        passed = sum(t["reward"] == 1.0 for t in trials)
        cells = sum(t["cell_accuracy"] for t in trials) / len(trials)
        tokens_in, tokens_out = (
            _sum(trials, "input_tokens"),
            _sum(trials, "output_tokens"),
        )
        tokens = (
            f"{_tokens(tokens_in)} / {_tokens(tokens_out)}"
            if tokens_in is not None and tokens_out is not None
            else "n/a"
        )
        cost = _sum(trials, "cost_usd")
        lines.append(
            f"| {run['agent']} {run['agent_version']} | `{run['model']}` "
            f"| {passed} / {len(trials)} | {cells:.1%} | {tokens} "
            f"| {_cost(cost)} | {run['date']} |"
        )

    columns = [f"`{run['model'].rpartition('/')[2]}`" for run in results]
    lines += [
        "",
        "## Benchmarks",
        "",
        "Each cell is the task result and the share of golden cells the agent",
        "reproduced; with several attempts, the passes out of the attempts.",
        "",
        "| Benchmark | " + " | ".join(columns) + " |",
        "|---|" + "---|" * len(columns),
    ]
    by_run = []
    for run in results:
        grouped = defaultdict(list)
        for trial in run["trials"]:
            grouped[trial["benchmark"]].append(trial)
        by_run.append(grouped)
    benchmarks = sorted({b for grouped in by_run for b in grouped})
    for benchmark in benchmarks:
        cells = []
        for grouped in by_run:
            trials = grouped.get(benchmark)
            if not trials:
                cells.append("not run")
                continue
            passed = sum(t["reward"] == 1.0 for t in trials)
            accuracy = sum(t["cell_accuracy"] for t in trials) / len(trials)
            verdict = "**pass**" if passed == len(trials) else "fail"
            if len(trials) > 1:
                verdict = f"{passed} / {len(trials)} pass"
            errors = sorted({t["error"] for t in trials if t["error"]})
            note = f" ({', '.join(errors)})" if errors else ""
            cells.append(f"{verdict}, {accuracy:.1%}{note}")
        link = f"[{benchmark}](../benchmark/{benchmark}.html)"
        lines.append(f"| {link} | " + " | ".join(cells) + " |")

    lines += [
        "",
        "## Reading the results",
        "",
        "- **Few attempts.** A pilot run makes one attempt per benchmark, so a",
        "  single pass or failure says little about a model on its own.",
        "- **Public data.** The inputs and golden files are published in this",
        "  repository, so a model may have seen them in training; the sandbox",
        "  only rules out looking them up during the run.",
        "- **Contributor models.** A model whose name ends in `-contributor`",
        "  runs at a discount in exchange for its provider training on the",
        "  prompts and answers, which may inflate that provider's later scores.",
        "",
    ]
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    add = commands.add_parser("add", help="record a Harbor job, then render")
    add.add_argument("job_dir", type=Path)
    check = commands.add_parser("render", help="rebuild the leaderboard page")
    check.add_argument("--check", action="store_true")
    args = parser.parse_args()

    if args.command == "add":
        run = collect(args.job_dir)
        RESULTS.mkdir(exist_ok=True)
        path = RESULTS / f"{run['job']}.json"
        path.write_text(json.dumps(run, indent=2) + "\n")
        print(f"wrote {path.relative_to(ROOT)}")
    page = render(load_results())
    if getattr(args, "check", False):
        if not PAGE.is_file() or PAGE.read_text() != page:
            sys.exit(f"{PAGE.relative_to(ROOT)} is out of date; run render")
        return
    PAGE.write_text(page)
    print(f"wrote {PAGE.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
