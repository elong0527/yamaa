"""Build Harbor tasks and a job file from yamaa benchmarks.

Every benchmark with a `prompt.md` becomes one Harbor task per language:

    <out>/tasks/<benchmark>-<language>/   <out> is ~/.cache/yamaa-harbor
      task.toml            deny-all network; the job adds the model API host
      instruction.md       the language's system prompt, then the
                           benchmark's prompt.md verbatim
      environment/         FROM the base image, plus the benchmark's input/
      tests/               grade.py, contract.json and the golden files,
                           built into the separate verifier image
      solution/            copies the golden files plus a placeholder
                           result.R/result.py, for Harbor's oracle agent
    <out>/job.json         the agent, model, provider host and API key name
    <out>/jobs/            Harbor job directories

Tasks never name a model or provider; `job.json` does, so one build runs
against any provider. Run from the repository root:

    uv run --project python --group harbor python evaluations/harbor/build.py \\
        --model opencode-go/muse-spark-1.3-contributor
    uv run --project python --group harbor harbor run -c ~/.cache/yamaa-harbor/job.json

Pass `--language r` or `--language python` to build only one track; the
default builds both, so R and Python are assessed independently.
"""

from __future__ import annotations

import argparse
import csv
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

import yaml

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
BENCHMARKS = ROOT / "benchmarks"
IMAGE = "yamaa-harbor-env:0.2"
OPENCODE_VERSION = "1.18.33"
OPENCODE_MODELS_PATH = "/opt/yamaa-eval/opencode-models.json"
# Build output and Harbor job directories stay outside the repository, whose
# validators read every file in the tree.
OUT = Path(os.environ.get("XDG_CACHE_HOME") or Path.home() / ".cache") / "yamaa-harbor"

# Model API host and key variable per provider prefix of `--model`.
PROVIDERS = {
    "anthropic": ("api.anthropic.com", "ANTHROPIC_API_KEY"),
    "openai": ("api.openai.com", "OPENAI_API_KEY"),
    "xai": ("api.x.ai", "XAI_API_KEY"),
    "opencode": ("opencode.ai", "OPENCODE_API_KEY"),
    "opencode-go": ("opencode.ai", "OPENCODE_API_KEY"),
}
# One system prompt per language, shared by all tasks. The benchmark's
# prompt.md stays language-agnostic; the system prompt names the language
# and the required script, so R and Python are assessed independently.
LANGUAGES = {
    "r": {"script": "result.R", "system": "system-r.md", "label": "R"},
    "python": {"script": "result.py", "system": "system-python.md", "label": "Python"},
}
# Harbor installs OpenCode with nvm and npm during agent setup only.
SETUP_HOSTS = (
    "raw.githubusercontent.com",
    "github.com",
    "nodejs.org",
    "registry.npmjs.org",
)
WEB_TOOLS_DENIED = {"webfetch": "deny", "websearch": "deny"}
NULL_TOKENS = {"", "NA", "NaN", "nan", ".", "NULL", "None"}


class BuildError(ValueError):
    pass


def git_commit() -> str:
    def git(*args: str) -> str:
        return subprocess.run(
            ["git", *args], cwd=ROOT, capture_output=True, text=True, check=True
        ).stdout.strip()

    commit = git("rev-parse", "HEAD")
    dirty = git("status", "--porcelain", "--", "benchmarks", "evaluations")
    return commit + ("+dirty" if dirty else "")


def readme_tags(readme: str) -> dict[str, str]:
    lifecycle = re.search(r"Lifecycle-(\w+)", readme)
    standard = re.search(r"Standard:\*?\*?\s*([A-Za-z]+)", readme)
    return {
        "lifecycle": lifecycle.group(1) if lifecycle else "unknown",
        "standard": standard.group(1) if standard else "unknown",
    }


def system_prompt(language: str) -> str:
    """The shared system prompt for one language, read from its file."""
    if language not in LANGUAGES:
        raise BuildError(f"unknown language {language!r}; want r or python")
    return (HERE / LANGUAGES[language]["system"]).read_text(encoding="utf-8").strip()


def output_specs(benchmark: Path) -> list[dict]:
    """Each specification that writes a golden dataset, in file order.

    `spec.yaml`, or else the `spec_<name>.yaml` files: domains built
    together (DM and SUPPDM) each write their own golden, and variants that
    write the same one count once.
    """
    single = benchmark / "spec.yaml"
    paths = [single] if single.is_file() else sorted(benchmark.glob("spec_*.yaml"))
    specs, seen = [], set()
    for path in paths:
        spec = yaml.safe_load(path.read_text(encoding="utf-8"))
        output = spec.get("output") or {}
        name = Path(output.get("path") or "").name
        if not name or name in seen or not (benchmark / "expected" / name).is_file():
            continue
        if output.get("warning_log") or output.get("verification_log"):
            raise BuildError(f"{benchmark.name}: log outputs are not graded yet")
        seen.add(name)
        specs.append(spec)
    if not specs:
        raise BuildError(f"{benchmark.name}: no specification writes a golden dataset")
    return specs


def _output_contract(benchmark: Path, spec: dict) -> dict:
    output = spec["output"]
    name = Path(output["path"]).name
    columns = list(output["columns"])
    keys = list(spec["keys"])
    declared = {c["name"]: c.get("type") for c in spec.get("columns", [])}
    types = {}
    for column in columns:
        kind = declared.get(column)
        if kind not in ("str", "int", "float", "date", "datetime"):
            raise BuildError(f"{benchmark.name}: {column} has type {kind!r}")
        types[column] = kind
    with (benchmark / "expected" / name).open(newline="", encoding="utf-8") as handle:
        reader = csv.reader(handle)
        header = next(reader)
        rows = list(reader)
    if header != columns:
        raise BuildError(
            f"{benchmark.name}: golden {name} header differs from the spec"
        )
    seen = set()
    for row in rows:
        record = dict(zip(header, row, strict=True))
        key = tuple(record[k] for k in keys)
        if key in seen or any(part.strip() == "" for part in key):
            raise BuildError(f"{benchmark.name}: golden key {key} is not unique")
        seen.add(key)
        # The grader reads these tokens as no value, so a golden cell that
        # holds one as text could never be matched.
        literal = [v for v in row if v.strip() and v.strip() in NULL_TOKENS]
        if literal:
            raise BuildError(f"{benchmark.name}: golden {name} holds {literal[0]!r}")
    return {"file": name, "columns": columns, "keys": keys, "types": types}


def contract_for(benchmark: Path, language: str) -> dict:
    """Outputs, columns, keys and column types, checked against the golden.

    The contract also names the required script (`result.R` or `result.py`),
    which the grader checks, reruns, and grades alongside the datasets.
    """
    if language not in LANGUAGES:
        raise BuildError(f"unknown language {language!r}; want r or python")
    outputs = [_output_contract(benchmark, s) for s in output_specs(benchmark)]
    return {
        "benchmark": benchmark.name,
        "language": language,
        "script": LANGUAGES[language]["script"],
        "outputs": outputs,
    }


def oracle_script(language: str, golden: list[Path]) -> str:
    """A script in the track's language that writes the golden files byte for
    byte, so the oracle also passes the grader's rerun."""
    lines = []
    if language == "r":
        lines.append("# Oracle: writes the golden datasets byte for byte.")
        for path in golden:
            data = path.read_bytes()
            values = ", ".join(f"0x{b:02x}" for b in data)
            lines.append(f"writeBin(as.raw(c({values})), '/app/output/{path.name}')")
    else:
        lines += [
            "# Oracle: writes the golden datasets byte for byte.",
            "from pathlib import Path",
            "",
        ]
        for path in golden:
            lines.append(
                f"Path('/app/output/{path.name}').write_bytes("
                f"bytes.fromhex('{path.read_bytes().hex()}'))"
            )
    return "\n".join(lines) + "\n"


def task_toml(
    benchmark: str, tags: dict[str, str], domain: str, commit: str, language: str
) -> str:
    standard, lifecycle = tags["standard"], tags["lifecycle"]
    noun = "datasets" if " and " in domain else "dataset"
    label = LANGUAGES[language]["label"]
    return f"""\
schema_version = "1.4"
artifacts = ["/app", "/logs/agent/trajectory.json"]

[task]
name = "yamaa/{benchmark}-{language}"
version = "0.1.0"
description = "Create the {domain} {noun} of the yamaa benchmark {benchmark} in {label}."
keywords = ["cdisc", "{standard.lower()}", "clinical-data"]

[metadata]
benchmark = "{benchmark}"
language = "{language}"
standard = "{standard}"
domain = "{domain}"
lifecycle = "{lifecycle}"
yamaa_commit = "{commit}"
publicly_indexed = true

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


def build_task(
    benchmark: Path, tasks: Path, image: str, commit: str, language: str
) -> Path:
    contract = contract_for(benchmark, language)
    readme = (benchmark / "README.md").read_text(encoding="utf-8")
    domain = " and ".join(s["domain"] for s in output_specs(benchmark))
    task = tasks / f"{benchmark.name}-{language}"
    if task.exists():
        shutil.rmtree(task)
    (task / "environment").mkdir(parents=True)
    (task / "tests").mkdir()
    (task / "solution").mkdir()

    task_text = task_toml(benchmark.name, readme_tags(readme), domain, commit, language)
    (task / "task.toml").write_text(task_text)
    prompt = (benchmark / "prompt.md").read_text(encoding="utf-8").strip()
    (task / "instruction.md").write_text(
        system_prompt(language) + "\n\n---\n\n" + prompt + "\n"
    )

    # Data files only: an input schema (`*.schema.yaml`) is yamaa's own
    # description of how a producer builds the input, not source data.
    shutil.copytree(
        benchmark / "input",
        task / "environment" / "input",
        ignore=shutil.ignore_patterns("*.yaml", "*.yml"),
    )
    (task / "environment" / "Dockerfile").write_text(
        f"FROM {image}\nCOPY --chown=agent:agent input/ /app/input/\n"
    )

    golden = [benchmark / "expected" / o["file"] for o in contract["outputs"]]
    for directory in (task / "tests" / "expected", task / "solution" / "expected"):
        directory.mkdir()
        for path in golden:
            shutil.copyfile(path, directory / path.name)
    shutil.copyfile(HERE / "grade.py", task / "tests" / "grade.py")
    (task / "tests" / "contract.json").write_text(json.dumps(contract, indent=2))
    (task / "tests" / "test.sh").write_text(
        "#!/usr/bin/env bash\n"
        "# No reward.json on a grader crash, so Harbor records a verifier error.\n"
        "set -euo pipefail\n"
        "python3 /tests/grade.py --rerun\n"
    )
    (task / "tests" / "Dockerfile").write_text(
        f"FROM {image}\n"
        "COPY --chmod=755 test.sh /tests/test.sh\n"
        "COPY grade.py contract.json /tests/\n"
        "COPY expected/ /tests/expected/\n"
    )
    script = LANGUAGES[language]["script"]
    (task / "solution" / script).write_text(oracle_script(language, golden))
    runner = "Rscript" if language == "r" else "python3"
    solve = task / "solution" / "solve.sh"
    solve.write_text(
        "#!/usr/bin/env bash\n"
        "# Oracle: a script that writes the golden files, to prove packaging,\n"
        "# the rerun, and grading.\n"
        "set -euo pipefail\n"
        "mkdir -p /app/output\n"
        f"cp /solution/{script} /app/output/{script}\n"
        f"{runner} /app/output/{script}\n"
    )
    solve.chmod(0o755)
    return task


def job_config(
    tasks: list[Path],
    *,
    model: str,
    api_host: str | None,
    key_env: str | None,
    variant: str | None,
    n_attempts: int,
    n_concurrent: int,
    job_name: str | None,
    jobs_dir: Path,
) -> dict:
    provider = model.partition("/")[0]
    known_host, known_key = PROVIDERS.get(provider, (None, None))
    host, key = api_host or known_host, key_env or known_key
    if not host or not key:
        raise BuildError(
            f"unknown provider {provider!r}; pass --api-host and --key-env"
        )
    kwargs: dict = {
        "version": OPENCODE_VERSION,
        "opencode_config": {
            "autoupdate": False,
            "share": "disabled",
            "lsp": False,
            "formatter": False,
            "snapshot": False,
            "permission": dict(WEB_TOOLS_DENIED),
        },
    }
    if variant:
        kwargs["variant"] = variant
    config: dict = {
        "jobs_dir": str(jobs_dir),
        "n_attempts": n_attempts,
        "n_concurrent_trials": n_concurrent,
        "environment": {
            "type": "docker",
            "extra_allowed_hosts": sorted({host, *SETUP_HOSTS}),
        },
        "agents": [
            {
                "name": "opencode",
                "model_name": model,
                "kwargs": kwargs,
                "env": {
                    key: "${" + key + "}",
                    "OPENCODE_PERMISSION": json.dumps(WEB_TOOLS_DENIED),
                    "OPENCODE_DISABLE_MODELS_FETCH": "1",
                    "OPENCODE_MODELS_PATH": OPENCODE_MODELS_PATH,
                    "OPENCODE_DISABLE_AUTOUPDATE": "1",
                    "OPENCODE_DISABLE_LSP_DOWNLOAD": "1",
                    "OPENCODE_DISABLE_CLAUDE_CODE": "1",
                },
                "extra_allowed_hosts": [host],
            }
        ],
        "tasks": [{"path": str(t)} for t in tasks],
    }
    if job_name:
        config["job_name"] = job_name
    return config


def build_selection(
    names: list[str],
    *,
    languages: list[str],
    tasks_dir: Path,
    image: str,
    commit: str,
    strict: bool,
) -> tuple[list[Path], list[str]]:
    """One task per benchmark per language.

    With `strict` (explicit `--benchmarks`) an unsupported benchmark
    raises; otherwise it is skipped and reported, so the default
    every-prompt build keeps working as new prompts land ahead of grader
    support.
    """
    tasks, skipped = [], []
    for name in names:
        for language in languages:
            try:
                tasks.append(
                    build_task(BENCHMARKS / name, tasks_dir, image, commit, language)
                )
            except BuildError as exc:
                if strict:
                    raise
                skipped.append(f"{name}-{language}: {exc}")
    return tasks, skipped


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--benchmarks", nargs="*", help="default: every prompt")
    parser.add_argument(
        "--language",
        nargs="*",
        choices=sorted(LANGUAGES),
        default=sorted(LANGUAGES),
        help="default: both tracks",
    )
    parser.add_argument("--model", required=True, help="provider/model")
    parser.add_argument("--api-host", help="model API host, for other providers")
    parser.add_argument("--key-env", help="API key variable, for other providers")
    parser.add_argument("--variant", help="OpenCode reasoning variant")
    parser.add_argument("--n-attempts", type=int, default=1)
    parser.add_argument("--n-concurrent", type=int, default=1)
    parser.add_argument("--job-name")
    parser.add_argument("--image", default=IMAGE)
    parser.add_argument("--out", type=Path, default=OUT)
    args = parser.parse_args()

    names = args.benchmarks or sorted(
        p.parent.name for p in BENCHMARKS.glob("*/prompt.md")
    )
    commit = git_commit()
    tasks_dir = args.out.resolve() / "tasks"
    # Start clean: a task left from an earlier build would still be picked up
    # by `harbor run -p <tasks>`.
    if tasks_dir.exists():
        shutil.rmtree(tasks_dir)
    tasks, skipped = build_selection(
        names,
        languages=args.language,
        tasks_dir=tasks_dir,
        image=args.image,
        commit=commit,
        strict=args.benchmarks is not None,
    )
    if not tasks:
        raise BuildError("no buildable benchmark with a prompt.md")
    for line in skipped:
        print(f"skip {line}", file=sys.stderr)
    config = job_config(
        tasks,
        model=args.model,
        api_host=args.api_host,
        key_env=args.key_env,
        variant=args.variant,
        n_attempts=args.n_attempts,
        n_concurrent=args.n_concurrent,
        job_name=args.job_name,
        jobs_dir=args.out.resolve() / "jobs",
    )
    job = args.out.resolve() / "job.json"
    job.write_text(json.dumps(config, indent=2) + "\n")
    print(f"built {len(tasks)} task(s) ({', '.join(args.language)}) in {tasks_dir}")
    print(f"job: {job}")


if __name__ == "__main__":
    main()
