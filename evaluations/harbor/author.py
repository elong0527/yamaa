"""Write prompts in Harbor sandboxes that hold no expected data.

A benchmark's full prompt is written by an agent whose sandbox holds the
benchmark's sources (`packet.py`) and nothing it is graded on, with the
recipe (`prompts/recipe.md`) as its instruction. Solvers that see only a
candidate prompt and the inputs, as an evaluated agent does, then try it.
Nothing flows back to a writer: `report` tells the person reviewing the
candidates which columns the solvers failed. From the repository root::

    python -m evaluations.harbor.author write --model <provider/model>
    python -m evaluations.harbor.author run -c <out>/write-full/configs/<job>.json
    python -m evaluations.harbor.author collect <out>/jobs/<job> --into <dir>
    python -m evaluations.harbor.author solve --prompts <dir> --model <provider/model>
    python -m evaluations.harbor.author report <out>/jobs/<job>

Without Docker, `prepare` builds the same writer folders, and with
`--solvers` the candidate solver folders, for agents without a sandbox, and
`grade` grades the solver folders, rerunning each saved script in a clean
copy of its folder::

    python -m evaluations.harbor.author prepare --benchmarks <name> --out <dir>
    python -m evaluations.harbor.author prepare --solvers --prompts <dir> --out <dir>
    python -m evaluations.harbor.author grade <dir>

`--tier conventions --prompts <dir>` narrows the full prompts in `<dir>`, or
the committed ones, with the tier contract (`prompts/README.md`) as the
instruction. These jobs run locally and are never uploaded to Harbor Hub.
The output directory defaults to `~/.cache/yamaa-harbor/authoring`.

A session that hands the folders to subagents follows the session rules in
section 7 of the recipe: it proves its base provides `prepare` and `grade`
before a round, records outside-folder reads without rejecting a candidate
for the agent guides a runtime requires, checks that its Python provides
the system prompt's packages before the solvers run, and counts two fresh
sessions as its two solver attempts where its tool allows no model choice.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import shutil
import sys
import tempfile
import tomllib
from collections import defaultdict
from collections.abc import Iterable
from pathlib import Path

from evaluations.harbor import build, packet
from evaluations.harbor import grade as grader

HERE = Path(__file__).resolve().parent
OUT = build.OUT / "authoring"
# A writer's instruction is the document that defines its prompt tier.
INSTRUCTIONS = {
    "full": HERE / "prompts" / "recipe.md",
    "conventions": HERE / "prompts" / "README.md",
}
# What a passing writing task leaves, required files first; `collect` copies
# the optional ones when they exist.
WRITTEN = {"full": ("full.md", "trace.json"), "conventions": ("conventions.md",)}
OPTIONAL = {"full": ("questions.md",), "conventions": ()}
VERIFIED = "--reward /logs/verifier/reward.json"
# What a solver folder's instruction says before the evaluated agent's, which
# names `/app` paths: the folder stands for `/app`, and the script names its
# paths relative to the folder and runs from it, as `grade_folders` reruns it
# in a clean copy of the folder and Harbor's verifier reruns it from `/app`.
LOCAL_PATHS = (
    "This folder stands for `/app`: below, `/app/input` means `input/`\n"
    "here and `/app/output` means `output/`. Read and write only inside\n"
    "this folder. Name both paths relative to it in your script, which\n"
    "runs from here and is graded by a rerun from a clean copy of this\n"
    "folder, as `{runner} output/{script}`."
)


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


def _request(tier: str, prompts: Path | None) -> None:
    if tier not in INSTRUCTIONS:
        raise build.BuildError(f"no writing task for the {tier} prompt")
    if tier == "full" and prompts is not None:
        raise build.BuildError(
            "a full prompt is written from the benchmark's sources alone"
        )


def _sources(
    benchmark: Path, folder: Path, tier: str, prompts: Path | None
) -> Path | None:
    """Write a writer's `sources/`, and `input/` for the full prompt, in
    `folder`; return the full prompt a conventions writer narrows.

    The full prompt is written from the benchmark's sources alone
    (`packet.py`): no earlier candidate or solver result joins them. The
    conventions prompt is written from the full prompt alone, the candidate
    in `prompts/<benchmark>/full.md` or else the committed one.
    """
    _request(tier, prompts)
    if tier == "full":
        packet.write(benchmark, folder)
        return None
    candidate = prompts / benchmark.name / "full.md" if prompts else None
    if candidate is None or not candidate.is_file():
        candidate = build.PROMPTS / benchmark.name / "full.md"
    if not candidate.is_file():
        raise build.BuildError(f"{benchmark.name}: no full prompt to narrow")
    sources = folder / "sources"
    sources.mkdir(parents=True)
    shutil.copyfile(candidate, sources / "full.md")
    for name in packet.CHECKS:
        shutil.copyfile(HERE / name, sources / name)
    return candidate


def writing_task(
    benchmark: Path,
    tasks: Path,
    commit: str,
    tier: str = "full",
    *,
    prompts: Path | None = None,
) -> Path:
    """One Harbor task in which an agent writes a benchmark's prompt of
    `tier` from its sources (`_sources`). The writer's instruction is the
    document that defines its tier, and the verifier runs `check.py`, which
    needs no expected data.
    """
    _request(tier, prompts)
    task = tasks / f"{benchmark.name}-write-{tier}"
    if task.exists():
        shutil.rmtree(task)
    full = _sources(benchmark, task / "environment", tier, prompts)
    tests = task / "tests"
    tests.mkdir(parents=True)
    if full is None:
        shutil.copyfile(task / "environment/sources/check.json", tests / "check.json")
        mounts = "sources/ /app/sources/", "input/ /app/input/"
        data = "check.json"
        check = (
            "full /app/output/full.md --trace /app/output/trace.json "
            "--data /tests/check.json"
        )
    else:
        shutil.copyfile(full, tests / "full.md")
        mounts = ("sources/ /app/sources/",)
        data = "full.md"
        check = "conventions /app/output/conventions.md --full /tests/full.md"
    for name in packet.CHECKS:
        shutil.copyfile(HERE / name, tests / name)
    (task / "environment" / "Dockerfile").write_text(
        f"FROM {build.AGENT_IMAGES['python']}\n"
        + "".join(f"COPY --chown=agent:agent {m}\n" for m in mounts)
    )
    (task / "instruction.md").write_text(INSTRUCTIONS[tier].read_text(encoding="utf-8"))
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


def writer_folder(
    benchmark: Path, out: Path, tier: str = "full", *, prompts: Path | None = None
) -> Path:
    """A writer's folder for an agent without a sandbox,
    `out/<benchmark>-write-<tier>/`: what a writing task's container holds
    (`_sources`), the instruction, and an empty `output/`."""
    _request(tier, prompts)
    folder = out / f"{benchmark.name}-write-{tier}"
    if folder.exists():
        shutil.rmtree(folder)
    _sources(benchmark, folder, tier, prompts)
    (folder / "instruction.md").write_text(
        INSTRUCTIONS[tier].read_text(encoding="utf-8")
    )
    (folder / "output").mkdir()
    return folder


def _inputs(benchmark: Path, folder: Path) -> None:
    """The benchmark's input data in `folder/input/`, as the evaluated agent
    gets it: an input schema is yamaa's own description, not source data."""
    shutil.copytree(
        benchmark / "input",
        folder / "input",
        ignore=shutil.ignore_patterns("*.yaml", "*.yml"),
    )


def solver_folder(benchmark: Path, candidate: str, language: str, out: Path) -> Path:
    """A candidate solver's folder for an agent without a sandbox,
    `out/<benchmark>-candidate-<language>/`, standing for `/app`: the
    inputs, an empty `output/`, and an instruction that maps the `/app`
    paths to the folder (`LOCAL_PATHS`), then gives the evaluated agent's
    instruction verbatim. No grading file or task page is in it."""
    build.contract_for(benchmark, language)
    folder = out / build.task_name(benchmark.name, language, candidate=True)
    if folder.exists():
        shutil.rmtree(folder)
    _inputs(benchmark, folder)
    paths = LOCAL_PATHS.format(**build.LANGUAGES[language])
    (folder / "instruction.md").write_text(
        f"{paths}\n\n---\n\n{build.instruction(language, candidate)}"
    )
    (folder / "output").mkdir()
    return folder


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
    """What a candidate solver's grade says failed or left unchecked, by
    output file and column only: no value, key or row count."""
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
    rerun = grade.get("rerun", {})
    if rerun.get("problems"):
        failed.add("the script does not reproduce its datasets from a clean state")
    if rerun.get("file_only"):
        failed.add(f"a file-only check, {rerun['file_only']}")
    network = grade.get("network", {})
    if network.get("violations") or network.get("language_violations"):
        failed.add("the solver broke the language or web-tool policy")
    return failed


def _summary(grades: Iterable[tuple[str, dict | None]]) -> dict[str, str]:
    """Per benchmark, how many solver attempts passed and what failed, by
    output file and column only (`_failures`)."""
    attempts: dict[str, int] = defaultdict(int)
    passed: dict[str, int] = defaultdict(int)
    failed: dict[str, dict[str, int]] = defaultdict(lambda: defaultdict(int))
    for benchmark, grade in grades:
        attempts[benchmark] += 1
        if grade is None:
            failed[benchmark]["the attempt was not graded"] += 1
            continue
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


def report(jobs: list[Path]) -> dict[str, str]:
    """What candidate solvers in Harbor jobs failed, per benchmark, for the
    person who reviews the candidates. No writer receives it."""

    def grades():
        for job in jobs:
            for path in sorted(job.glob("*/result.json")):
                name = _task_name(json.loads(path.read_text(encoding="utf-8")))
                benchmark, marker, _ = name.rpartition("-candidate-")
                if not marker:
                    continue
                grade_path = path.parent / "verifier" / "grade.json"
                yield (
                    benchmark,
                    (
                        json.loads(grade_path.read_text(encoding="utf-8"))
                        if grade_path.is_file()
                        else None
                    ),
                )

    return _summary(grades())


def _rerun(case: Path, contract: dict, output: Path, run: grader.RunScript) -> dict:
    """Rerun a solver's saved script as Harbor's verifier does
    (`grade.rerun_script`): from a clean copy of its folder, which holds the
    benchmark's inputs and an output folder with only the script, by the
    track's interpreter, within the verifier's timeout. Nothing runs where
    that interpreter is not on PATH: the grade is then a file-only check."""
    script = contract["script"]
    interpreter = grader.INTERPRETERS[contract["language"]]
    if shutil.which(interpreter) is None:
        return {
            "passed": False,
            "problems": [],
            "outputs": [],
            "file_only": f"{script} not rerun without {interpreter}",
        }
    with tempfile.TemporaryDirectory(prefix="yamaa-solver-rerun-") as temporary:
        clean = Path(temporary)
        _inputs(case, clean)
        (clean / "output").mkdir()
        shutil.copyfile(output / script, clean / "output" / script)
        return grader.rerun_script(contract, case / "expected", clean / "output", run)


def grade_folders(
    root: Path, run: grader.RunScript = grader.run_script
) -> dict[str, str]:
    """Grade each solver folder (`solver_folder`) as `report` grades a
    Harbor job: the datasets it saved against the golden files, then its
    script rerun from a clean copy of the folder (`_rerun`), whose datasets
    must match the golden files too. A saved dataset alone is never a pass.
    The session that runs the solvers reads it; no writer receives it."""

    def grades():
        for folder in sorted(p for p in root.iterdir() if "-candidate-" in p.name):
            benchmark, _, language = folder.name.rpartition("-candidate-")
            case = build.BENCHMARKS / benchmark
            contract = build.contract_for(case, language)
            output = folder / "output"
            outputs = [
                grader.grade_output(spec, case / "expected", output)
                for spec in contract["outputs"]
            ]
            script = grader.grade_script(contract, output)
            rerun = (
                _rerun(case, contract, output, run)
                if script["passed"]
                else {"passed": False, "problems": [], "outputs": []}
            )
            yield (
                benchmark,
                {
                    "passed": script["passed"]
                    and rerun["passed"]
                    and all(o["passed"] for o in outputs),
                    "outputs": outputs,
                    "script": script,
                    "rerun": rerun,
                },
            )

    return _summary(grades())


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


def _benchmarks(names: list[str] | None) -> list[str]:
    return names or sorted(
        p.name
        for p in build.BENCHMARKS.iterdir()
        if p.name.startswith(("sdtm-", "adam-"))
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    write = commands.add_parser("write", help="build writing tasks and jobs")
    prepare = commands.add_parser(
        "prepare", help="build writer or solver folders for agents without a sandbox"
    )
    solve = commands.add_parser("solve", help="build candidate solver tasks and jobs")
    for command in (write, prepare):
        command.add_argument("--tier", choices=list(INSTRUCTIONS), default="full")
        command.add_argument(
            "--benchmarks", nargs="+", help="default: every sdtm-* and adam-* benchmark"
        )
    for command in (write, prepare, solve):
        command.add_argument(
            "--prompts",
            type=Path,
            required=command is solve,
            help="full prompts, <dir>/<benchmark>/full.md: to narrow or to solve",
        )
    for command in (prepare, solve):
        command.add_argument(
            "--language",
            nargs="+",
            choices=sorted(build.LANGUAGES),
            default=sorted(build.DEFAULT_LANGUAGES),
        )
    prepare.add_argument(
        "--solvers", action="store_true", help="solver folders for --prompts"
    )
    prepare.add_argument("--out", type=Path, required=True)
    for command in (write, solve):
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
    summary = commands.add_parser("report", help="list the columns solvers failed")
    for command in (gather, summary):
        command.add_argument("jobs", nargs="+", type=Path)
    gather.add_argument("--into", type=Path, required=True)
    mark = commands.add_parser("grade", help="grade solver folders like `report`")
    mark.add_argument("folders", type=Path)
    args = parser.parse_args()
    solvers = getattr(args, "solvers", False)
    if args.command in ("write", "prepare") and not solvers:
        if args.tier == "full" and args.prompts:
            parser.error("a full prompt is written from the benchmark's sources alone")
    if solvers and args.prompts is None:
        parser.error("--solvers needs the candidates in --prompts")

    if args.command == "run":
        asyncio.run(run(json.loads(args.config.read_text(encoding="utf-8"))))
    elif args.command == "collect":
        copied = [p for job in args.jobs for p in collect(job, args.into)]
        print(f"copied {len(copied)} file(s) into {args.into}")
    elif args.command in ("report", "grade"):
        texts = (
            report(args.jobs)
            if args.command == "report"
            else grade_folders(args.folders)
        )
        for benchmark, text in texts.items():
            print(f"{benchmark}\n{text}")
    elif args.command == "prepare" and solvers:
        folders = []
        for folder in sorted(
            p for p in args.prompts.iterdir() if (p / "full.md").is_file()
        ):
            candidate = (folder / "full.md").read_text(encoding="utf-8")
            for language in args.language:
                try:
                    folders.append(
                        solver_folder(
                            build.BENCHMARKS / folder.name,
                            candidate,
                            language,
                            args.out,
                        )
                    )
                except build.BuildError as exc:
                    print(
                        f"skip {folder.name}-candidate-{language}: {exc}",
                        file=sys.stderr,
                    )
        print(f"built {len(folders)} solver folder(s) in {args.out}")
    elif args.command == "prepare":
        folders = [
            writer_folder(
                build.BENCHMARKS / name, args.out, args.tier, prompts=args.prompts
            )
            for name in _benchmarks(args.benchmarks)
        ]
        print(f"built {len(folders)} writer folder(s) in {args.out}")
    elif args.command == "write":
        commit = build.git_commit()
        tasks_dir = _fresh(args.out / f"write-{args.tier}" / "tasks")
        tasks = [
            writing_task(
                build.BENCHMARKS / name,
                tasks_dir,
                commit,
                args.tier,
                prompts=args.prompts,
            )
            for name in _benchmarks(args.benchmarks)
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
