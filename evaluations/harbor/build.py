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
from pathlib import Path

import yaml

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
BENCHMARKS = ROOT / "benchmarks"
IMAGE = "yamaa-harbor-env:0.1"
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


def contract_for(benchmark: Path, language: str) -> dict:
    """Outputs, columns, keys and column types, checked against the golden.

    The contract also names the required script (`result.R` or `result.py`),
    which the grader checks alongside the datasets.
    """
    if language not in LANGUAGES:
        raise BuildError(f"unknown language {language!r}; want r or python")
    spec_path = benchmark / "spec.yaml"
    if not spec_path.is_file():
        raise BuildError(f"{benchmark.name}: only single spec.yaml benchmarks")
    spec = yaml.safe_load(spec_path.read_text(encoding="utf-8"))
    output = spec["output"]
    if output.get("warning_log") or output.get("verification_log"):
        raise BuildError(f"{benchmark.name}: log outputs are not graded yet")
    if (benchmark / "environment.yaml").exists():
        raise BuildError(f"{benchmark.name}: project functions are not supported")
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
    golden = benchmark / "expected" / name
    with golden.open(newline="", encoding="utf-8") as handle:
        reader = csv.reader(handle)
        header = next(reader)
        rows = list(reader)
    if header != columns:
        raise BuildError(f"{benchmark.name}: golden header differs from the spec")
    seen = set()
    for row in rows:
        record = dict(zip(header, row, strict=True))
        key = tuple(record[k] for k in keys)
        if key in seen or any(part == "" for part in key):
            raise BuildError(f"{benchmark.name}: golden key {key} is not unique")
        seen.add(key)
        literal = [v for v in row if v != "" and v.strip() in NULL_TOKENS]
        if literal:
            raise BuildError(f"{benchmark.name}: golden holds {literal[0]!r}")
    return {
        "benchmark": benchmark.name,
        "language": language,
        "script": LANGUAGES[language]["script"],
        "outputs": [{"file": name, "columns": columns, "keys": keys, "types": types}],
    }


def task_toml(
    benchmark: str, tags: dict[str, str], domain: str, commit: str, language: str
) -> str:
    standard, lifecycle = tags["standard"], tags["lifecycle"]
    label = LANGUAGES[language]["label"]
    return f"""\
schema_version = "1.4"
artifacts = ["/app", "/logs/agent/trajectory.json"]

[task]
name = "yamaa/{benchmark}-{language}"
version = "0.1.0"
description = "Create the {domain} dataset of the yamaa benchmark {benchmark} in {label}."
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
    domain = yaml.safe_load((benchmark / "spec.yaml").read_text())["domain"]
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

    shutil.copytree(benchmark / "input", task / "environment" / "input")
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
        "python3 /tests/grade.py\n"
    )
    (task / "tests" / "Dockerfile").write_text(
        f"FROM {image}\n"
        "COPY --chmod=755 test.sh /tests/test.sh\n"
        "COPY grade.py contract.json /tests/\n"
        "COPY expected/ /tests/expected/\n"
    )
    solve = task / "solution" / "solve.sh"
    script = LANGUAGES[language]["script"]
    solve.write_text(
        "#!/usr/bin/env bash\n"
        "# Oracle: the golden files plus a placeholder script, to prove\n"
        "# packaging and grading. The placeholder satisfies the script\n"
        "# check; the datasets carry the grade.\n"
        "set -euo pipefail\n"
        "mkdir -p /app/output\n"
        "cp /solution/expected/*.csv /app/output/\n"
        f"printf '# oracle placeholder for {script}\\n' > /app/output/{script}\n"
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
    tasks = [
        build_task(BENCHMARKS / n, tasks_dir, args.image, commit, language)
        for n in names
        for language in args.language
    ]
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
