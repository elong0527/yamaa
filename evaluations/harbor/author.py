"""Write prompts in Harbor sandboxes that hold no expected data.

A benchmark's full prompt is written by an agent whose sandbox holds the
benchmark's sources (`packet.py`) and nothing it is graded on. Solvers that
see only a candidate prompt and the inputs, as an evaluated agent does, then
try it, and the writer hears back which columns they failed, never a value.
`prompts/recipe.md` describes the flow. From the repository root::

    python -m evaluations.harbor.author write --model <provider/model>
    python -m evaluations.harbor.author run -c <out>/write-full/configs/<job>.json
    python -m evaluations.harbor.author collect <out>/jobs/<job> --into <dir>
    python -m evaluations.harbor.author solve --prompts <dir> --model <provider/model>
    python -m evaluations.harbor.author feedback <out>/jobs/<job> --into <dir>
    python -m evaluations.harbor.author write --prompts <dir> --model <provider/model>

`write --tier conventions` writes conventions prompts from full prompts the
same way. These jobs run locally and are never uploaded to Harbor Hub. The
output directory defaults to `~/.cache/yamaa-harbor/authoring`.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import shutil
import sys
import tomllib
from collections import defaultdict
from pathlib import Path

from evaluations.harbor import build, packet

HERE = Path(__file__).resolve().parent
OUT = build.OUT / "authoring"
# The instruction for each prompt tier a writing task can write.
WRITERS = {"full": "author-full.md", "conventions": "author-conventions.md"}
# What a passing writing task leaves, required files first; `collect` copies
# the optional ones when they exist.
WRITTEN = {"full": ("full.md", "trace.json"), "conventions": ("conventions.md",)}
OPTIONAL = {"full": ("questions.md",), "conventions": ()}
VERIFIED = "--reward /logs/verifier/reward.json"


def writing_toml(benchmark: str, tier: str, commit: str) -> str:
    return f"""\
schema_version = "1.4"
artifacts = ["/app/output", "/logs/agent/trajectory.json"]

[task]
name = "yamaa/{benchmark}-write-{tier}"
version = "0.4.0"
description = "Write the {tier} prompt of the yamaa benchmark {benchmark} from sources that hold no expected data."
keywords = ["cdisc", "prompt-writing"]

[metadata]
benchmark = "{benchmark}"
purpose = "write"
prompt = "{tier}"
yamaa_commit = "{commit}"

[agent]
timeout_sec = 1800.0
user = "agent"
network_mode = "allowlist"
allowed_hosts = []

[environment]
build_timeout_sec = 1800.0
cpus = 2
memory_mb = 4096
network_mode = "allowlist"
allowed_hosts = []

[verifier]
environment_mode = "separate"
timeout_sec = 300.0

[verifier.environment]
network_mode = "no-network"
"""


def writing_task(
    benchmark: Path,
    tasks: Path,
    commit: str,
    tier: str = "full",
    *,
    prompts: Path | None = None,
) -> Path:
    """One task in which an agent writes a benchmark's prompt of `tier`.

    The full prompt is written from the benchmark's sources (`packet.py`),
    joined by an earlier candidate and its feedback from
    `prompts/<benchmark>/`. The conventions prompt is written from the full
    prompt: the candidate in `prompts/<benchmark>/full.md`, else the
    committed one. The verifier runs `check.py`, which needs no expected data.
    """
    if tier not in WRITERS:
        raise build.BuildError(f"no writing task for the {tier} prompt")
    task = tasks / f"{benchmark.name}-write-{tier}"
    if task.exists():
        shutil.rmtree(task)
    tests = task / "tests"
    tests.mkdir(parents=True)
    previous = prompts / benchmark.name if prompts is not None else None
    if tier == "full":
        packet.write(benchmark, task / "environment", previous=previous)
        shutil.copyfile(task / "environment/sources/check.json", tests / "check.json")
        mounts = "sources/ /app/sources/", "input/ /app/input/"
        data = "check.json"
        check = (
            "full /app/output/full.md --trace /app/output/trace.json "
            "--data /tests/check.json"
        )
    else:
        candidate = previous / "full.md" if previous is not None else None
        if candidate is None or not candidate.is_file():
            candidate = build.PROMPTS / benchmark.name / "full.md"
        if not candidate.is_file():
            raise build.BuildError(f"{benchmark.name}: no full prompt to narrow")
        sources = task / "environment" / "sources"
        sources.mkdir(parents=True)
        shutil.copyfile(candidate, sources / "full.md")
        shutil.copyfile(HERE / "prompts" / "README.md", sources / "tiers.md")
        for name in packet.CHECKS:
            shutil.copyfile(HERE / name, sources / name)
        shutil.copyfile(candidate, tests / "full.md")
        mounts = ("sources/ /app/sources/",)
        data = "full.md"
        check = "conventions /app/output/conventions.md --full /tests/full.md"
    for name in packet.CHECKS:
        shutil.copyfile(HERE / name, tests / name)
    (task / "environment" / "Dockerfile").write_text(
        f"FROM {build.AGENT_IMAGES['python']}\n"
        + "".join(f"COPY --chown=agent:agent {m}\n" for m in mounts)
    )
    (task / "instruction.md").write_text(
        (HERE / WRITERS[tier]).read_text(encoding="utf-8")
    )
    (task / "task.toml").write_text(writing_toml(benchmark.name, tier, commit))
    (tests / "test.sh").write_text(
        "#!/usr/bin/env bash\n"
        "# A prompt that fails its checks scores 0; a crash is a verifier error.\n"
        "set -euo pipefail\n"
        f"python3 /tests/check.py {check} {VERIFIED}\n"
    )
    (tests / "Dockerfile").write_text(
        f"FROM {build.VERIFIER_IMAGES['python']}\n"
        "COPY --chmod=755 test.sh /tests/test.sh\n"
        f"COPY {' '.join(packet.CHECKS)} {data} /tests/\n"
        "RUN chmod 700 /tests\n"
    )
    return task


def solver_tasks(
    prompts: Path, tasks: Path, commit: str, languages: list[str]
) -> tuple[list[Path], list[str]]:
    """One task per candidate full prompt in `prompts/<benchmark>/full.md`
    and language, graded like an evaluation task (`build.build_task`)."""
    built, skipped = [], []
    for folder in sorted(p for p in prompts.iterdir() if (p / "full.md").is_file()):
        candidate = (folder / "full.md").read_text(encoding="utf-8")
        for language in languages:
            try:
                built.append(
                    build.build_task(
                        build.BENCHMARKS / folder.name,
                        tasks,
                        None,
                        commit,
                        language,
                        candidate=candidate,
                    )
                )
            except build.BuildError as exc:
                skipped.append(f"{folder.name}-candidate-{language}: {exc}")
    return built, skipped


def runnable(config: dict) -> None:
    """Refuse a job with anything but writing and candidate tasks: ranked
    evaluations run through `run.py`, which keeps and publishes evidence."""
    if not config.get("tasks"):
        raise ValueError("the job has no tasks")
    for entry in config["tasks"]:
        task = tomllib.loads(
            (Path(entry["path"]) / "task.toml").read_text(encoding="utf-8")
        )
        metadata = task.get("metadata", {})
        protocol = str(metadata.get("grading_protocol", ""))
        if metadata.get("purpose") != "write" and not protocol.endswith("-candidate"):
            raise ValueError(
                f"{task['task']['name']} is neither a writing nor a candidate "
                "task; run evaluations with run.py"
            )


async def run(config: dict) -> None:
    from harbor.job import Job
    from harbor.models.job.config import JobConfig

    runnable(config)
    job = await Job.create(JobConfig.model_validate(config))
    await job.run()


def _task_name(result: dict) -> str:
    return str(result.get("task_name", "")).removeprefix("yamaa/")


def collect(job: Path, into: Path) -> list[Path]:
    """Copy what passing trials wrote into `into/<benchmark>/`: prompts from
    writing jobs, programs from candidate solver jobs. The first passing
    attempt of each benchmark wins."""
    copied = []
    for path in sorted(job.glob("*/result.json")):
        result = json.loads(path.read_text(encoding="utf-8"))
        rewards = (result.get("verifier_result") or {}).get("rewards") or {}
        if rewards.get("reward") != 1.0:
            continue
        benchmark, written, tier = _task_name(result).rpartition("-write-")
        if written:
            required, optional = WRITTEN.get(tier, ()), OPTIONAL.get(tier, ())
        else:
            benchmark, marker, language = _task_name(result).rpartition("-candidate-")
            script = build.LANGUAGES.get(language, {}).get("script") if marker else None
            required, optional = ((script,) if script else ()), ()
        output = path.parent / "artifacts" / "app" / "output"
        target = into / benchmark
        if (
            not required
            or any((target / name).exists() for name in required)
            or not all((output / name).is_file() for name in required)
        ):
            continue
        target.mkdir(parents=True, exist_ok=True)
        for name in (*required, *optional):
            if (output / name).is_file():
                shutil.copyfile(output / name, target / name)
                copied.append(target / name)
    return copied


def _failures(grade: dict) -> set[str]:
    """What a candidate solver's grade says failed, by output file and
    column only: no value, key or row count."""
    failed = set()
    for output in grade.get("outputs", []):
        name, problems = output["file"], output.get("problems", [])
        if any("was not written" in p or "symlink" in p for p in problems):
            failed.add(f"{name} was not written")
            continue
        for problem in problems:
            if problem.startswith("missing columns: "):
                failed |= {
                    f"{name} lacks {c}"
                    for c in problem.removeprefix("missing columns: ").split(", ")
                }
            elif problem.startswith(("unexpected columns", "repeated columns")):
                failed.add(f"{name} has columns the request does not ask for")
            elif problem.startswith(f"{name} is "):
                failed.add(f"{name} is not a readable dataset")
            elif "row(s)" in problem or "key" in problem:
                failed.add(f"{name} has missing, extra or repeated rows")
        failed |= {
            f"{name} {column} differs"
            for column, accuracy in output.get("column_accuracy", {}).items()
            if accuracy < 1
        }
    if not grade.get("script", {}).get("passed", True):
        failed.add("the required script was not saved")
    if grade.get("rerun", {}).get("problems"):
        failed.add("the script does not reproduce its datasets from a clean state")
    network = grade.get("network", {})
    if network.get("violations") or network.get("language_violations"):
        failed.add("the solver broke the language or web-tool policy")
    return failed


def feedback(jobs: list[Path]) -> dict[str, str]:
    """What candidate solvers failed, per benchmark, as `feedback.md` text."""
    attempts: dict[str, int] = defaultdict(int)
    passed: dict[str, int] = defaultdict(int)
    failed: dict[str, dict[str, int]] = defaultdict(lambda: defaultdict(int))
    for job in jobs:
        for path in sorted(job.glob("*/result.json")):
            name = _task_name(json.loads(path.read_text(encoding="utf-8")))
            benchmark, marker, _ = name.rpartition("-candidate-")
            if not marker:
                continue
            attempts[benchmark] += 1
            grade_path = path.parent / "verifier" / "grade.json"
            if not grade_path.is_file():
                failed[benchmark]["the attempt was not graded"] += 1
                continue
            grade = json.loads(grade_path.read_text(encoding="utf-8"))
            if grade.get("passed"):
                passed[benchmark] += 1
            for item in _failures(grade):
                failed[benchmark][item] += 1
    texts = {}
    for benchmark in sorted(attempts):
        total = attempts[benchmark]
        lines = [
            f"{passed[benchmark]} of {total} solver attempts reproduced every "
            "requested dataset."
        ]
        lines += [
            f"- {item}: {count} of {total} attempts"
            for item, count in sorted(failed[benchmark].items())
        ]
        texts[benchmark] = "\n".join(lines) + "\n"
    return texts


def _jobs(args, tasks: list[Path], stem: str, permissions: dict | None) -> list[Path]:
    """Write one job configuration per model (and language, for solvers)."""
    configs = args.out / stem / "configs"
    configs.mkdir(parents=True, exist_ok=True)
    groups = {"": tasks} if permissions else defaultdict(list)
    if not permissions:
        for task in tasks:
            groups[build.task_language(task)].append(task)
    written = []
    for model in args.model:
        for language, own in sorted(groups.items()):
            name = "-".join(
                p for p in (stem, model.rpartition("/")[2], language, args.variant) if p
            )
            config = build.job_config(
                own,
                model=model,
                api_host=args.api_host,
                key_env=args.key_env,
                variant=args.variant,
                n_attempts=args.n_attempts,
                n_concurrent=args.n_concurrent,
                job_name=name,
                jobs_dir=args.out / "jobs",
                permissions=permissions,
            )
            path = configs / f"{name}.json"
            path.write_text(json.dumps(config, indent=2) + "\n")
            written.append(path)
    return written


def _fresh(directory: Path) -> Path:
    if directory.exists():
        shutil.rmtree(directory)
    directory.mkdir(parents=True)
    return directory


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    write = commands.add_parser("write", help="build writing tasks and jobs")
    write.add_argument("--tier", choices=list(WRITERS), default="full")
    write.add_argument(
        "--benchmarks", nargs="+", help="default: every sdtm-* and adam-* benchmark"
    )
    solve = commands.add_parser("solve", help="build candidate solver tasks and jobs")
    solve.add_argument(
        "--language",
        nargs="+",
        choices=sorted(build.LANGUAGES),
        default=sorted(build.DEFAULT_LANGUAGES),
    )
    for command in (write, solve):
        command.add_argument(
            "--prompts",
            type=Path,
            required=command is solve,
            help="candidates, <dir>/<benchmark>/full.md, with any feedback.md",
        )
        command.add_argument("--model", nargs="+", required=True, help="provider/model")
        command.add_argument("--api-host", help="model API host, for other providers")
        command.add_argument("--key-env", help="API key variable, for other providers")
        command.add_argument("--variant", help="OpenCode model variant")
        command.add_argument("--n-attempts", type=int, default=1)
        command.add_argument("--n-concurrent", type=int, default=4)
        command.add_argument("--out", type=Path, default=OUT)
    execute = commands.add_parser("run", help="run a writing or candidate job")
    execute.add_argument("-c", "--config", type=Path, required=True)
    gather = commands.add_parser("collect", help="copy what passing trials wrote")
    report = commands.add_parser("feedback", help="list the columns solvers failed")
    for command in (gather, report):
        command.add_argument("jobs", nargs="+", type=Path)
    gather.add_argument("--into", type=Path, required=True)
    report.add_argument("--into", type=Path, help="write <dir>/<benchmark>/feedback.md")
    args = parser.parse_args()

    if args.command == "run":
        asyncio.run(run(json.loads(args.config.read_text(encoding="utf-8"))))
    elif args.command == "collect":
        copied = [p for job in args.jobs for p in collect(job, args.into)]
        print(f"copied {len(copied)} file(s) into {args.into}")
    elif args.command == "feedback":
        for benchmark, text in feedback(args.jobs).items():
            print(f"{benchmark}\n{text}")
            if args.into is not None:
                (args.into / benchmark).mkdir(parents=True, exist_ok=True)
                (args.into / benchmark / "feedback.md").write_text(text)
    elif args.command == "write":
        commit = build.git_commit()
        tasks_dir = _fresh(args.out / f"write-{args.tier}" / "tasks")
        names = args.benchmarks or sorted(
            p.name
            for p in build.BENCHMARKS.iterdir()
            if p.name.startswith(("sdtm-", "adam-"))
        )
        tasks = [
            writing_task(
                build.BENCHMARKS / name,
                tasks_dir,
                commit,
                args.tier,
                prompts=args.prompts,
            )
            for name in names
        ]
        configs = _jobs(args, tasks, f"write-{args.tier}", build.tool_permissions(None))
        print(f"built {len(tasks)} writing task(s) in {tasks_dir}")
        for path in configs:
            print(f"job: {path}")
    else:
        tasks_dir = _fresh(args.out / "solve" / "tasks")
        tasks, skipped = solver_tasks(
            args.prompts, tasks_dir, build.git_commit(), args.language
        )
        for line in skipped:
            print(f"skip {line}", file=sys.stderr)
        configs = _jobs(args, tasks, "solve", None)
        print(f"built {len(tasks)} candidate solver task(s) in {tasks_dir}")
        for path in configs:
            print(f"job: {path}")


if __name__ == "__main__":
    main()
