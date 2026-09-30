"""Turn the yamaa benchmarks into opencode agentic-evaluation tasks.

Every benchmark becomes one task: a prompt written from its README and a
starting workspace holding its inputs. The answer -- the entry specification
of a positive benchmark, and `expected/` of every benchmark -- never enters
the workspace. A finished workspace is judged by the same comparator the
conformance runner uses, `yamaa.adapters.conformance.compare_example`, so a
task passes exactly when the engine's own conformance check would.

    generate   write one prompt and one starting workspace per benchmark
    verify     prove the task set matches the benchmarks one to one
    run        run each task through `opencode run` with a chosen model
    grade      judge finished workspaces and count the exact matches
    calibrate  grade reference, empty, and perturbed agents to prove the judge

Run it with the repository's Python environment, for example
`uv run --project python python automation/opencode-eval/opencode_eval.py`.
"""

from __future__ import annotations

import argparse
import csv
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from collections import Counter
from collections.abc import Callable, Iterable, Sequence
from concurrent.futures import ThreadPoolExecutor
from dataclasses import asdict, dataclass, field
from pathlib import Path

import yaml

from yamaa import __version__
from yamaa.adapters.conformance import (
    ERROR_CONTRACT,
    EXPECTED_DIR,
    ComparisonFinding,
    ConformanceError,
    DiagnosticObservation,
    ExampleReport,
    compare_example,
    entry_specification,
    execute_example,
    expected_kind,
)
from yamaa.io import ProjectResources, approve_roots
from yamaa.io.parquet import parse_parquet
from yamaa.planning import plan_workflow
from yamaa.specification._yaml import read_yaml_document
from yamaa.specification.schema import load_schema_bundle

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
BENCHMARKS = REPO / "benchmarks"
SCHEMA_ROOT = REPO / "yaml"
WORKSPACE_TEMPLATE = HERE / "workspace"

# Never copied into a workspace: the prose the prompt is written from, the
# reference run scripts, and the committed answer.
WITHHELD = {"README.md", "run.R", "run.py", EXPECTED_DIR}
# The judge reads these committed artifacts and no others.
JUDGED_SUFFIXES = (".csv", ".parquet")
# Workspace files the harness owns rather than the agent.
HARNESS_FILES = {"AGENTS.md", "opencode.json", "yamaa_run.py", "reference"}
OUTPUT = "output"

# Language reference copied into every workspace. `benchmarks/` is not in it,
# and neither is the article that indexes the benchmarks.
REFERENCE = {
    "yaml": REPO / "yaml",
    "rules": REPO / "rules",
    "articles": REPO / "docs" / "articles",
    "python.md": REPO / "python" / "README.md",
}
REFERENCE_EXCLUDED = {"benchmark.md"}

# Strings whose appearance in a transcript means the agent reached the
# committed answers rather than working from the workspace. The reference
# docs link to the repository and its dashboards, so a bare repository URL
# would mark every agent that read them; these appear in none of them.
CONTAMINATION = (
    str(REPO) + os.sep,
    "/expected/",
    "raw.githubusercontent.com/elong0527/yamaa",
    "api.github.com/repos/elong0527/yamaa",
)


# --------------------------------------------------------------------------
# README -> sections


@dataclass(frozen=True)
class Readme:
    title: str
    sections: tuple[tuple[str, str], ...]
    standard: str | None
    domain: str | None

    def section(self, label: str) -> str | None:
        for name, text in self.sections:
            if name == label:
                return text
        return None

    def others(self, *skip: str) -> tuple[tuple[str, str], ...]:
        return tuple((name, text) for name, text in self.sections if name not in skip)


_LABEL = re.compile(r"^\*\*([A-Z][A-Za-z ]*):\*\*\s*")
_STANDARD = re.compile(
    r"^\**Standard:\**\s*(?P<standard>\S+)\s*\|\s*\**Domain:\**\s*(?P<domain>\S+)"
)


_ITEM = re.compile(r"^\s*(?:[-*]|\d+\.)\s")


def _prose(text: str) -> str:
    """Unwrap a README's hard-wrapped lines the way a person types a request.

    Each list item becomes one line, each other paragraph one line with its
    first letter capitalised, and a table or code block stays as written.
    """
    paragraphs = []
    for paragraph in text.split("\n\n"):
        lines = paragraph.split("\n")
        if lines[0].lstrip().startswith(("|", "```")):
            paragraphs.append(paragraph)
            continue
        blocks: list[str] = []
        for line in lines:
            if _ITEM.match(line) or not blocks:
                blocks.append(line.strip() if not _ITEM.match(line) else line.rstrip())
            else:
                blocks[-1] += " " + line.strip()
        blocks = [_without_answer_paths(block) for block in blocks]
        blocks = [block for block in blocks if block.strip(" -.")]
        if not blocks:
            continue
        if not _ITEM.match(blocks[0]):
            blocks[0] = blocks[0][:1].upper() + blocks[0][1:]
        paragraphs.append("\n".join(blocks))
    return "\n\n".join(paragraphs)


_SENTENCE = re.compile(r"(?<=\.)\s+")


def _without_answer_paths(text: str) -> str:
    """Drop the sentences and clauses that point at a committed answer.

    A few READMEs name `expected/...` for the dashboard's sake. The file is
    not in the workspace, and the pointer says where the answer is kept.
    """
    kept = []
    for sentence in _SENTENCE.split(text):
        if "expected/" not in sentence:
            kept.append(sentence)
            continue
        head, _, tail = sentence.rpartition(";")
        if head and "expected/" in tail:
            kept.append(head.rstrip() + ".")
    return " ".join(kept)


def parse_readme(text: str) -> Readme:
    """Split a benchmark README into its labelled sections.

    Everything from `## How to fix` on is the reference answer of a negative
    benchmark and is dropped, as are the badges. A paragraph that opens with
    no label continues the section before it.
    """
    body = text.split("\n## How to fix", 1)[0]
    lines = [line for line in body.splitlines() if not line.startswith("[![")]
    title = next(line[2:].strip() for line in lines if line.startswith("# "))
    paragraphs = [
        block.strip()
        for block in "\n".join(
            line for line in lines if not line.startswith("# ")
        ).split("\n\n")
        if block.strip()
    ]
    sections: list[tuple[str, list[str]]] = []
    standard = domain = None
    for paragraph in paragraphs:
        found = _STANDARD.match(paragraph)
        if found:
            standard, domain = found["standard"], found["domain"]
            continue
        label = _LABEL.match(paragraph)
        if label:
            sections.append((label[1], [paragraph[label.end() :]]))
        elif sections:
            sections[-1][1].append(paragraph)
        else:
            sections.append(("Summary", [paragraph]))
    return Readme(
        title=title,
        sections=tuple(
            (name, _prose("\n\n".join(part for part in parts if part)))
            for name, parts in sections
        ),
        standard=standard,
        domain=domain,
    )


# --------------------------------------------------------------------------
# Task model


@dataclass(frozen=True)
class Deliverable:
    file: str
    columns: tuple[str, ...]


@dataclass(frozen=True)
class Variable:
    name: str
    label: str
    type: str


@dataclass
class Task:
    id: str
    case: str
    kind: str
    title: str
    standard: str
    domain: str
    entry_spec: str
    given: list[str]
    deliverables: list[Deliverable]
    judged: list[str]
    keys: list[str] = field(default_factory=list)
    variables: list[Variable] = field(default_factory=list)
    order_by: list[str] = field(default_factory=list)
    decimals: int | None = None
    unjudged_expected: list[str] = field(default_factory=list)

    @classmethod
    def load(cls, path: Path) -> Task:
        data = json.loads(path.read_text(encoding="utf-8"))
        data["deliverables"] = [
            Deliverable(item["file"], tuple(item["columns"]))
            for item in data["deliverables"]
        ]
        data["variables"] = [Variable(**item) for item in data["variables"]]
        return cls(**data)


def _files(root: Path) -> list[Path]:
    return sorted(path for path in root.rglob("*") if path.is_file())


def _tracked_files(case: Path) -> list[Path]:
    """Return the files git tracks under a benchmark, never a local cache."""
    listed = subprocess.run(
        ["git", "ls-files", "-z", "--", str(case)],
        cwd=REPO,
        capture_output=True,
        check=True,
    ).stdout.decode("utf-8")
    return sorted(REPO / name for name in listed.split("\0") if name)


def _given_files(case: Path, kind: str, entry: Path) -> list[str]:
    given = []
    for path in _tracked_files(case):
        relative = path.relative_to(case)
        if relative.parts[0] in WITHHELD:
            continue
        if kind == "positive" and path == entry:
            continue
        given.append(relative.as_posix())
    return given


def _csv_table(payload: bytes) -> tuple[list[str], int]:
    rows = list(csv.reader(io.StringIO(payload.decode("utf-8"))))
    return (rows[0] if rows else []), max(len(rows) - 1, 0)


def _artifact_columns(path: Path) -> tuple[str, ...]:
    if path.suffix == ".parquet":
        return tuple(parse_parquet(path.read_bytes()).frame.columns)
    return tuple(_csv_table(path.read_bytes())[0])


def _resolved_specification(entry: Path):
    approved = approve_roots(entry)
    resources = ProjectResources(
        approved.project_root,
        base_directory=entry.parent,
        data_roots=approved.data_roots,
    )
    workflow = plan_workflow(entry, load_schema_bundle(SCHEMA_ROOT), resources)
    node = next(
        node for node in workflow.nodes if node.entry_path == workflow.entry_path
    )
    return node.resolved.specification


def _declared_output(entry: Path) -> str | None:
    document = read_yaml_document(entry)
    if isinstance(document, dict) and isinstance(document.get("output"), dict):
        path = document["output"].get("path")
        return Path(path).name if isinstance(path, str) else None
    return None


def build_task(case: Path, index: int) -> Task:
    kind = expected_kind(case)
    entry = entry_specification(case)
    readme = parse_readme((case / "README.md").read_text(encoding="utf-8"))
    expected = case / EXPECTED_DIR
    judged = sorted(
        path for path in expected.iterdir() if path.suffix in JUDGED_SUFFIXES
    )
    task = Task(
        id=f"task-{index:03d}",
        case=case.name,
        kind=kind,
        title=readme.title,
        standard=readme.standard or "",
        domain=readme.domain or "",
        entry_spec=entry.name,
        given=_given_files(case, kind, entry),
        deliverables=[],
        judged=[path.name for path in judged] if kind == "positive" else [],
        unjudged_expected=sorted(
            path.name
            for path in expected.iterdir()
            if path.suffix not in JUDGED_SUFFIXES and path.name != ERROR_CONTRACT
        ),
    )
    if kind == "positive":
        specification = _resolved_specification(entry)
        task.domain = task.domain or specification.domain
        task.deliverables = [
            Deliverable(path.name, _artifact_columns(path)) for path in judged
        ]
        declared = {column.name: column for column in specification.columns}
        task.keys = list(specification.keys)
        task.variables = [
            Variable(
                name,
                declared[name].label or "" if name in declared else "",
                str(declared[name].type) if name in declared else "",
            )
            for name in specification.output.columns
        ]
        task.order_by = [
            f"{term.variable}"
            + (" descending" if term.direction == "desc" else "")
            + (", missing first" if term.nulls == "first" else "")
            for term in specification.output.order_by or ()
        ]
        task.decimals = specification.output.decimals
    else:
        declared_output = _declared_output(entry)
        if declared_output:
            task.deliverables = [Deliverable(declared_output, ())]
    return task


# --------------------------------------------------------------------------
# Prompt


def _table(header: Sequence[str], rows: Iterable[Sequence[str]]) -> str:
    lines = ["| " + " | ".join(header) + " |", "|" + "---|" * len(header)]
    lines.extend("| " + " | ".join(row) + " |" for row in rows)
    return "\n".join(lines)


def _manifest(case: Path, task: Task) -> tuple[str, list[str]]:
    rows = []
    others = []
    for relative in task.given:
        path = case / relative
        if relative.startswith("input/") and path.suffix == ".csv":
            try:
                columns, records = _csv_table(path.read_bytes())
            except UnicodeDecodeError:
                # Some negative benchmarks commit a malformed input on purpose.
                rows.append((f"`{relative}`", "--", "--"))
                continue
            rows.append((f"`{relative}`", ", ".join(columns), str(records)))
        elif relative.startswith("input/") and path.suffix == ".parquet":
            frame = parse_parquet(path.read_bytes()).frame
            rows.append((f"`{relative}`", ", ".join(frame.columns), str(frame.height)))
        elif relative.startswith("input/"):
            rows.append((f"`{relative}`", "--", "--"))
        else:
            others.append(relative)
    return _table(("File", "Columns", "Records"), rows), others


def _goal(readme: Readme) -> str:
    goal = readme.section("Goal") or readme.section("Summary") or ""
    # A negative README says "attempt ..." because it knows the run fails;
    # the person asking for the dataset does not.
    goal = re.sub(r"^[Aa]ttempt (to )?", "", goal)
    return goal[:1].upper() + goal[1:]


def _positive_prompt(case: Path, task: Task, readme: Readme) -> str:
    manifest, others = _manifest(case, task)
    what = f"the {task.domain} dataset" if task.domain else "a dataset"
    standard = f" ({task.standard})" if task.standard else ""
    parts = [
        f"# {task.title}",
        f"I need {what}{standard} built as a yamaa specification from the "
        "files in this workspace. Please write the specification, run it with "
        "`python yamaa_run.py`, and leave the result in `output/`.",
        "## What I need",
        _goal(readme),
    ]
    source = readme.section("Input")
    parts += ["## Source data", *([source] if source else []), manifest]
    if others:
        parts.append(
            "Also in the workspace, to use as given: "
            + ", ".join(f"`{name}`" for name in others)
            + "."
        )
    parts.append("## Variables")
    identity = [f"One record per {', '.join(task.keys)}."] if task.keys else []
    if task.order_by:
        identity.append(f"Rows are ordered by {'; then '.join(task.order_by)}.")
    if task.decimals is not None:
        identity.append(f"Numbers are written with {task.decimals} decimals.")
    if identity:
        parts.append(" ".join(identity))
    parts.append(
        _table(
            ("Variable", "Label", "Type"),
            ((f"`{v.name}`", v.label, v.type) for v in task.variables),
        )
    )
    variables = readme.section("Variables")
    if variables:
        parts.append(variables)
    notes = readme.others("Goal", "Summary", "Input", "Variables")
    if notes:
        parts.append("## Notes")
        parts.extend(
            text if name == "Note" else f"**{name}:** {text}" for name, text in notes
        )
    parts.append("## Deliverables")
    deliverables = [
        f"- `{task.entry_spec}`: the entry specification. Any further "
        "specification it needs goes beside it as `spec_<name>.yaml`, named by "
        "the entry as a parent or a producer.",
        "- `python yamaa_run.py` must publish exactly these files, with the "
        "columns in this order:",
    ]
    deliverables.extend(
        f"  - `output/{item.file}`: {', '.join(item.columns)}"
        for item in task.deliverables
    )
    deliverables.append(
        "- Everything in `output/` must come from that run; do not write or "
        "edit it by hand."
    )
    parts.append("\n".join(deliverables))
    return "\n\n".join(parts) + "\n"


def _negative_prompt(case: Path, task: Task, readme: Readme) -> str:
    manifest, others = _manifest(case, task)
    what = f"{task.domain} " if task.domain else ""
    standard = f" ({task.standard})" if task.standard else ""
    target = (
        f"`output/{task.deliverables[0].file}`"
        if task.deliverables
        else "the dataset it declares"
    )
    # Every negative README title opens with "Reject" or "Fail"; the person
    # handing over a reviewed specification does not know that it will.
    parts = [
        f"# Run the {what}specification",
        f"`{task.entry_spec}` in this workspace is the reviewed {what}"
        f"specification{standard}. Please run it on the files here with "
        f"`python yamaa_run.py` and deliver {target}.",
        "## What the specification is for",
        _goal(readme),
        "## Files",
        manifest,
        "Given, beside the inputs: " + ", ".join(f"`{name}`" for name in others) + ".",
        "## Ground rules",
        "\n".join(
            [
                "- The specification, `input/`, and the other given files are "
                "under change control: do not edit, rename, or delete them.",
                "- If the run cannot produce the dataset, do not work around "
                "it. Record the diagnostic in `output/error.yaml` as AGENTS.md "
                "describes, and put the cause and the change you would propose "
                "in `output/NOTES.md`.",
            ]
        ),
    ]
    return "\n\n".join(parts) + "\n"


def render_prompt(case: Path, task: Task) -> str:
    readme = parse_readme((case / "README.md").read_text(encoding="utf-8"))
    if task.kind == "positive":
        return _positive_prompt(case, task, readme)
    return _negative_prompt(case, task, readme)


# --------------------------------------------------------------------------
# generate


def benchmark_cases(names: Sequence[str] | None = None) -> list[Path]:
    cases = sorted(path for path in BENCHMARKS.iterdir() if path.is_dir())
    if names:
        wanted = set(names)
        cases = [case for case in cases if case.name in wanted]
        missing = wanted - {case.name for case in cases}
        if missing:
            raise SystemExit(f"unknown benchmarks: {', '.join(sorted(missing))}")
    return cases


def _copy_given(source: Path, target: Path) -> None:
    """Copy one given file, keeping a symbolic link a link.

    `negative-path-symlink` commits a link on purpose; copying what it points
    at would hand the engine a plain file and nothing to reject.
    """
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.is_symlink() or target.exists():
        target.unlink()
    shutil.copyfile(source, target, follow_symlinks=False)


def _same_given(mine: Path, handed_out: Path) -> bool:
    if handed_out.is_symlink():
        return mine.is_symlink() and os.readlink(mine) == os.readlink(handed_out)
    return not mine.is_symlink() and mine.read_bytes() == handed_out.read_bytes()


def generate(out: Path, names: Sequence[str] | None = None) -> list[Task]:
    """Write `tasks.json` and one directory per task under `out`."""
    tasks = []
    for index, case in enumerate(benchmark_cases(), start=1):
        if names and case.name not in names:
            continue
        task = build_task(case, index)
        directory = out / "tasks" / task.case
        if directory.exists():
            shutil.rmtree(directory)
        (directory / "workspace").mkdir(parents=True)
        for relative in task.given:
            _copy_given(case / relative, directory / "workspace" / relative)
        (directory / "PROMPT.md").write_text(render_prompt(case, task), "utf-8")
        (directory / "task.json").write_text(
            json.dumps(asdict(task), indent=2) + "\n", "utf-8"
        )
        tasks.append(task)
    (out / "tasks.json").write_text(
        json.dumps(
            [{"id": t.id, "case": t.case, "kind": t.kind} for t in tasks], indent=2
        )
        + "\n",
        "utf-8",
    )
    return tasks


def load_tasks(out: Path, names: Sequence[str] | None = None) -> list[Task]:
    index = json.loads((out / "tasks.json").read_text(encoding="utf-8"))
    tasks = [Task.load(out / "tasks" / item["case"] / "task.json") for item in index]
    if names:
        tasks = [task for task in tasks if task.case in set(names)]
    return tasks


# --------------------------------------------------------------------------
# verify


def verify(out: Path) -> list[str]:
    """Return every way the task set fails to match the benchmarks one to one."""
    problems: list[str] = []
    cases = {case.name: case for case in benchmark_cases()}
    tasks = {task.case: task for task in load_tasks(out)}
    manifest = read_yaml_document(BENCHMARKS / "execution-manifest.yaml")["examples"]
    validation = read_yaml_document(BENCHMARKS / "validation-manifest.yaml")["fixtures"]

    def same(label: str, left: set[str], right: set[str], names: str) -> None:
        if left != right:
            problems.append(
                f"{label}: only in {names.split('/')[0]}: {sorted(left - right)}; "
                f"only in {names.split('/')[1]}: {sorted(right - left)}"
            )

    same("cases vs tasks", set(cases), set(tasks), "benchmarks/tasks")
    same(
        "cases vs execution manifest", set(cases), set(manifest), "benchmarks/manifest"
    )

    for name, case in cases.items():
        task = tasks.get(name)
        if task is None:
            continue
        contract = case / EXPECTED_DIR / ERROR_CONTRACT
        if (task.kind == "negative") != contract.is_file():
            problems.append(f"{name}: kind {task.kind} disagrees with {contract.name}")
        workspace = out / "tasks" / name / "workspace"
        present = {path.relative_to(workspace).as_posix() for path in _files(workspace)}
        if present != set(task.given):
            problems.append(f"{name}: workspace files differ from the given list")
        for relative in present & set(task.given):
            if not _same_given(workspace / relative, case / relative):
                problems.append(f"{name}: {relative} differs from the benchmark")
        for relative in present:
            if Path(relative).parts[0] in WITHHELD:
                problems.append(f"{name}: workspace carries withheld {relative}")
        if task.kind == "positive" and task.entry_spec in present:
            problems.append(f"{name}: workspace carries the entry specification")
        prompt = (out / "tasks" / name / "PROMPT.md").read_text(encoding="utf-8")
        for leak in ("How to fix", "schema_version", "derivation:", "expected/"):
            if leak in prompt:
                problems.append(f"{name}: prompt carries {leak!r}")

        if task.kind == "positive":
            judged = sorted(
                path.name
                for path in (case / EXPECTED_DIR).iterdir()
                if path.suffix in JUDGED_SUFFIXES
            )
            promised = sorted(item.file for item in task.deliverables)
            if not judged:
                problems.append(f"{name}: positive benchmark commits no artifact")
            if promised != judged:
                problems.append(f"{name}: deliverables {promised} != judged {judged}")
            for item in task.deliverables:
                committed = _artifact_columns(case / EXPECTED_DIR / item.file)
                if item.columns != committed:
                    problems.append(f"{name}: {item.file} columns differ")
                if f"`output/{item.file}`: {', '.join(item.columns)}" not in prompt:
                    problems.append(f"{name}: prompt omits {item.file}")
            primary = [
                item
                for item in task.deliverables
                if tuple(v.name for v in task.variables) == item.columns
            ]
            if not primary:
                problems.append(f"{name}: no deliverable carries output.columns")
        else:
            contract_data = read_yaml_document(contract)
            phase = contract_data.get("phase")
            if (phase == "validation") != (name in validation):
                problems.append(f"{name}: validation manifest disagrees ({phase})")
            if task.entry_spec not in present:
                problems.append(f"{name}: negative workspace lacks its specification")
    return problems


# --------------------------------------------------------------------------
# workspace, agents, and the judge


def materialize(out: Path, task: Task, destination: Path) -> Path:
    """Build one fresh workspace: the task's files plus the harness files."""
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(out / "tasks" / task.case / "workspace", destination, symlinks=True)
    for name in ("AGENTS.md", "opencode.json", "yamaa_run.py"):
        shutil.copyfile(WORKSPACE_TEMPLATE / name, destination / name)
    reference = destination / "reference"
    for name, source in REFERENCE.items():
        if source.is_dir():
            shutil.copytree(
                source,
                reference / name,
                ignore=shutil.ignore_patterns(*REFERENCE_EXCLUDED, "__pycache__"),
            )
        else:
            reference.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, reference / name)
    (destination / OUTPUT).mkdir(exist_ok=True)
    return destination


@dataclass
class Verdict:
    id: str
    case: str
    kind: str
    passed: bool
    findings: list[str]
    reproduced_rows: dict[str, int] = field(default_factory=dict)
    expected_rows: dict[str, int] = field(default_factory=dict)


def _rows(path: Path) -> int:
    if path.suffix == ".parquet":
        return parse_parquet(path.read_bytes()).frame.height
    return _csv_table(path.read_bytes())[1]


def _same_artifact(left: Path, right: Path) -> bool:
    if left.suffix == ".parquet":
        # Parquet has no byte guarantee (REQ-0742); compare what it reads as.
        one = parse_parquet(left.read_bytes())
        two = parse_parquet(right.read_bytes())
        return one.columns == two.columns and one.frame.equals(two.frame)
    return left.read_bytes() == right.read_bytes()


def _describe_finding(item: ComparisonFinding) -> str:
    def short(value: object) -> str:
        text = json.dumps(value, ensure_ascii=False)
        return text if len(text) <= 120 else text[:117] + "..."

    return (
        f"{item.kind}: {item.detail} "
        f"(committed {short(item.expected)}, produced {short(item.actual)})"
    )


def _delivered_report(workspace: Path, task: Task) -> tuple[ExampleReport, list[str]]:
    """Read the diagnostic an agent recorded as the report a run would give."""
    path = workspace / OUTPUT / ERROR_CONTRACT
    if not path.is_file():
        return ExampleReport(
            runtime_version=__version__, example=task.case, outcome="success"
        ), [f"{OUTPUT}/{ERROR_CONTRACT}: not delivered"]
    try:
        data = yaml.safe_load(path.read_text(encoding="utf-8"))
        diagnostic = DiagnosticObservation(
            phase=data["phase"],
            condition=data["condition"],
            spec_paths=tuple(data["spec_paths"]),
            requirement=data.get("requirement"),
            context=dict(data.get("context") or {}),
        )
    except Exception as error:  # noqa: BLE001 - an unreadable file is a finding
        return ExampleReport(
            runtime_version=__version__, example=task.case, outcome="error"
        ), [f"{OUTPUT}/{ERROR_CONTRACT}: unreadable ({type(error).__name__})"]
    return ExampleReport(
        runtime_version=__version__,
        example=task.case,
        outcome="failure",
        diagnostics=(diagnostic,),
    ), []


def grade(out: Path, task: Task, workspace: Path) -> Verdict:
    """Judge one finished workspace against its benchmark's committed answer.

    Three checks, all of which must hold:

    - integrity: every given file is byte-identical to the one handed out;
    - reproduction: the workspace, run afresh by the reference engine on the
      pristine inputs, satisfies `compare_example` for the benchmark;
    - delivery: `output/` holds what that run publishes (positive), or the
      recorded diagnostic itself satisfies `compare_example` (negative).
    """
    case = BENCHMARKS / task.case
    pristine = out / "tasks" / task.case / "workspace"
    findings: list[str] = []
    for relative in task.given:
        mine = workspace / relative
        if not mine.is_file():
            findings.append(f"integrity: {relative} removed")
        elif not _same_given(mine, pristine / relative):
            findings.append(f"integrity: {relative} changed")

    verdict = Verdict(task.id, task.case, task.kind, False, findings)
    for name in task.judged:
        verdict.expected_rows[name] = _rows(case / EXPECTED_DIR / name)

    skipped = {OUTPUT, ".opencode", *HARNESS_FILES}

    def harness_files(directory: str, names: list[str]) -> list[str]:
        # Only the workspace's own top level belongs to the harness; a given
        # `input/reference/` is the benchmark's.
        if Path(directory) != workspace:
            return []
        return [name for name in names if name in skipped]

    with tempfile.TemporaryDirectory() as scratch:
        rerun = Path(scratch) / task.id
        shutil.copytree(workspace, rerun, symlinks=True, ignore=harness_files)
        for relative in task.given:
            _copy_given(pristine / relative, rerun / relative)
        published = Path(scratch) / "published"
        try:
            report = execute_example(
                rerun, schema_root=SCHEMA_ROOT, output_dir=published
            )
        except ConformanceError as error:
            findings.append(f"reproduction: {error}")
            report = None
        if report is not None:
            if report.outcome == "error":
                findings.append(f"reproduction: engine error: {report.error}")
            judged = compare_example(report, case)
            findings.extend(
                f"reproduction: {_describe_finding(item)}" for item in judged.findings
            )
            for artifact in report.artifacts:
                produced = next(published.glob(f"{artifact.name}.*"), None)
                if produced is not None:
                    verdict.reproduced_rows[produced.name] = artifact.row_count

        delivered = workspace / OUTPUT
        if task.kind == "positive":
            if (delivered / ERROR_CONTRACT).is_file():
                findings.append(f"delivery: {ERROR_CONTRACT} on a positive task")
            for name in task.judged:
                mine = delivered / name
                ran = published / name
                if not mine.is_file():
                    findings.append(f"delivery: {OUTPUT}/{name} missing")
                elif not ran.is_file():
                    findings.append(f"delivery: {OUTPUT}/{name} not reproduced")
                elif not _same_artifact(mine, ran):
                    findings.append(
                        f"delivery: {OUTPUT}/{name} is not the run's "
                        f"({_rows(mine)} records delivered, {_rows(ran)} reproduced)"
                    )
        else:
            report_from_agent, problems = _delivered_report(workspace, task)
            findings.extend(f"delivery: {problem}" for problem in problems)
            if not problems:
                judged = compare_example(report_from_agent, case)
                findings.extend(
                    f"delivery: {_describe_finding(item)}" for item in judged.findings
                )
            for item in task.deliverables:
                if (delivered / item.file).is_file():
                    findings.append(f"delivery: {OUTPUT}/{item.file} published")

    verdict.passed = not findings
    return verdict


def agent_python(run_dir: Path) -> Path:
    """Return a Python holding a non-editable yamaa, building it once.

    An editable install would point the agent's `yamaa.__file__` back into
    this checkout, next to `benchmarks/`, and even a plain install of the
    source directory records its path in `direct_url.json`. A wheel built
    into the run directory records only the wheel.
    """
    venv = run_dir / "agent-venv"
    python = venv / "bin" / "python"
    if not python.is_file():
        wheels = run_dir / "agent-wheel"
        subprocess.run(
            ["uv", "build", "--quiet", "--wheel", "--out-dir", str(wheels)],
            cwd=REPO / "python",
            check=True,
        )
        subprocess.run(["uv", "venv", "--quiet", str(venv)], check=True)
        subprocess.run(
            ["uv", "pip", "install", "--quiet", "--python", str(python)]
            + [str(wheel) for wheel in sorted(wheels.glob("yamaa-*.whl"))],
            check=True,
        )
    return python


def _names_checkout(value: str) -> bool:
    # A sibling such as `yamaa-eval/` shares the prefix but not the checkout.
    marker = str(REPO) + os.sep
    return any(marker in part + os.sep for part in value.split(os.pathsep))


def _agent_environment(run_dir: Path, python: Path) -> dict[str, str]:
    """Isolate the agent from this machine's own configuration and checkout."""
    home = run_dir / "agent-home"
    home.mkdir(parents=True, exist_ok=True)
    # Provider credentials and proxies pass through; nothing that names this
    # checkout does, and the home directory is a fresh one, so the agent
    # reads neither this machine's opencode configuration nor its skills.
    environment = {
        key: value
        for key, value in os.environ.items()
        if not _names_checkout(value)
        and key not in {"PWD", "OLDPWD", "PYTHONPATH", "VIRTUAL_ENV"}
        and not key.startswith(("UV_", "XDG_"))
    }
    path = [
        entry
        for entry in os.environ.get("PATH", "").split(os.pathsep)
        if entry and not _names_checkout(entry)
    ]
    environment.update(
        HOME=str(home),
        XDG_CONFIG_HOME=str(home / ".config"),
        XDG_DATA_HOME=str(home / ".local" / "share"),
        XDG_CACHE_HOME=str(home / ".cache"),
        XDG_STATE_HOME=str(home / ".local" / "state"),
        PATH=os.pathsep.join([str(python.parent), *path]),
        VIRTUAL_ENV=str(python.parent.parent),
        PYTHONDONTWRITEBYTECODE="1",
    )
    return environment


def run_yamaa(workspace: Path, python: Path) -> dict:
    completed = subprocess.run(
        [str(python), "yamaa_run.py"],
        cwd=workspace,
        capture_output=True,
        text=True,
        env=_agent_environment(workspace.parent.parent, python),
    )
    return yaml.safe_load(completed.stdout) or {}


def reference_agent(task: Task, workspace: Path, python: Path) -> None:
    """Do what a perfect agent does, through the same helper an agent uses."""
    if task.kind == "positive":
        shutil.copyfile(
            BENCHMARKS / task.case / task.entry_spec, workspace / task.entry_spec
        )
    result = run_yamaa(workspace, python)
    if task.kind == "negative" and result.get("diagnostics"):
        (workspace / OUTPUT / ERROR_CONTRACT).write_text(
            yaml.safe_dump(result["diagnostics"][0], sort_keys=False), "utf-8"
        )


def empty_agent(task: Task, workspace: Path, python: Path) -> None:
    """Do nothing at all."""


def perturbed_agent(task: Task, workspace: Path, python: Path) -> None:
    """Do everything right, then get one delivered value wrong."""
    reference_agent(task, workspace, python)
    delivered = workspace / OUTPUT
    if task.kind == "negative":
        path = delivered / ERROR_CONTRACT
        data = yaml.safe_load(path.read_text(encoding="utf-8"))
        data["condition"] = f"not_{data['condition']}"
        path.write_text(yaml.safe_dump(data, sort_keys=False), "utf-8")
        return
    # The primary artifact loses its last record, as if one were filtered
    # away by hand after the run.
    primary = next(
        item.file
        for item in task.deliverables
        if item.columns == tuple(variable.name for variable in task.variables)
    )
    path = delivered / primary
    if path.suffix == ".parquet":
        frame = parse_parquet(path.read_bytes()).frame
        frame.head(max(frame.height - 1, 0)).write_parquet(path)
    else:
        lines = path.read_bytes().removesuffix(b"\n").split(b"\n")
        path.write_bytes(b"\n".join(lines[:-1] if len(lines) > 1 else lines) + b"\n")


def tampered_agent(task: Task, workspace: Path, python: Path) -> None:
    """Do everything right, then leave a given file changed."""
    reference_agent(task, workspace, python)
    first = workspace / task.given[0]
    first.write_bytes(first.read_bytes() + b"\n")


AGENTS: dict[str, Callable[[Task, Path, Path], None]] = {
    "reference": reference_agent,
    "empty": empty_agent,
    "perturbed": perturbed_agent,
    "tampered": tampered_agent,
}


def opencode_agent(
    model: str,
    opencode: str,
    timeout: int,
    variant: str | None,
    transcripts: Path,
) -> Callable[[Task, Path, Path], None]:
    def agent(task: Task, workspace: Path, python: Path) -> None:
        prompt = (workspace.parent.parent / "tasks-prompts" / task.case).read_text(
            encoding="utf-8"
        )
        command = [
            opencode,
            "run",
            "--model",
            model,
            "--dir",
            str(workspace),
            "--format",
            "json",
            "--title",
            task.id,
            *(["--variant", variant] if variant else []),
            prompt,
        ]
        transcripts.mkdir(parents=True, exist_ok=True)
        with (
            (transcripts / f"{task.case}.jsonl").open("w", encoding="utf-8") as stdout,
            (transcripts / f"{task.case}.stderr").open("w", encoding="utf-8") as stderr,
        ):
            try:
                subprocess.run(
                    command,
                    cwd=workspace,
                    stdout=stdout,
                    stderr=stderr,
                    env=_agent_environment(workspace.parent.parent, python),
                    timeout=timeout,
                )
            except subprocess.TimeoutExpired:
                stderr.write(f"\n[harness] timed out after {timeout}s\n")

    return agent


def contamination(transcript: Path) -> list[str]:
    if not transcript.is_file():
        return []
    text = transcript.read_text(encoding="utf-8", errors="replace")
    return [marker for marker in CONTAMINATION if marker in text]


def execute(
    out: Path,
    run_dir: Path,
    tasks: Sequence[Task],
    agent: Callable[[Task, Path, Path], None],
    jobs: int,
    transcripts: Path | None = None,
) -> list[Verdict]:
    run_dir = run_dir.resolve()
    if run_dir == REPO or REPO in run_dir.parents:
        # A workspace inside the checkout sits one `cd ..` from the answers.
        raise SystemExit(f"--run-dir must be outside {REPO}")
    python = agent_python(run_dir)
    prompts = run_dir / "tasks-prompts"
    prompts.mkdir(parents=True, exist_ok=True)

    def one(task: Task) -> Verdict:
        shutil.copyfile(out / "tasks" / task.case / "PROMPT.md", prompts / task.case)
        # Workspaces are named by task id, never by benchmark name.
        workspace = materialize(out, task, run_dir / "workspaces" / task.id)
        agent(task, workspace, python)
        verdict = grade(out, task, workspace)
        if transcripts is not None:
            leaked = contamination(transcripts / f"{task.case}.jsonl")
            if leaked:
                verdict.findings.append(f"contamination: {', '.join(leaked)}")
                verdict.passed = False
        return verdict

    with ThreadPoolExecutor(max_workers=jobs) as pool:
        verdicts = list(pool.map(one, tasks))
    write_results(run_dir, verdicts)
    return verdicts


def write_results(run_dir: Path, verdicts: Sequence[Verdict]) -> dict:
    run_dir.mkdir(parents=True, exist_ok=True)
    with (run_dir / "results.jsonl").open("w", encoding="utf-8") as handle:
        for verdict in verdicts:
            handle.write(json.dumps(asdict(verdict)) + "\n")
    summary = {
        "cases": len(verdicts),
        "exact": sum(verdict.passed for verdict in verdicts),
        "by_kind": _tally(verdicts, lambda verdict: verdict.kind),
        "by_family": _tally(verdicts, _family),
    }
    (run_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    return summary


def _family(verdict: Verdict) -> str:
    """Name a benchmark's slice by its prefix: adam, sdtm, schema, negative."""
    return verdict.case.split("-", 1)[0]


def _tally(
    verdicts: Sequence[Verdict], key: Callable[[Verdict], str]
) -> dict[str, dict[str, int]]:
    tally: dict[str, Counter] = {}
    for verdict in verdicts:
        counter = tally.setdefault(key(verdict), Counter())
        counter["cases"] += 1
        counter["exact"] += verdict.passed
    return {name: dict(counter) for name, counter in sorted(tally.items())}


def print_summary(verdicts: Sequence[Verdict], label: str) -> None:
    exact = sum(verdict.passed for verdict in verdicts)
    print(f"{label}: {exact}/{len(verdicts)} cases exactly match")
    for name, counts in _tally(verdicts, _family).items():
        print(f"  {name}: {counts['exact']}/{counts['cases']}")


# --------------------------------------------------------------------------
# command line


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="opencode_eval", description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)

    def add(name: str, help_text: str) -> argparse.ArgumentParser:
        command = commands.add_parser(name, help=help_text)
        command.add_argument("--tasks", type=Path, required=True, help="task set")
        command.add_argument("cases", nargs="*", help="benchmark names (default all)")
        return command

    add("generate", "write the task set")
    add("verify", "check the task set against the benchmarks")
    for name, help_text in (
        ("run", "run tasks through opencode and grade them"),
        ("calibrate", "grade reference, empty, and perturbed agents"),
        ("grade", "grade the workspaces of an earlier run"),
    ):
        command = add(name, help_text)
        command.add_argument("--run-dir", type=Path, required=True)
        command.add_argument("--jobs", type=int, default=4)
        if name == "run":
            command.add_argument("--model", required=True, help="provider/model")
            command.add_argument("--variant", help="provider reasoning effort")
            command.add_argument("--opencode", default="opencode")
            command.add_argument("--timeout", type=int, default=1800)
    args = parser.parse_args(argv)

    if args.command == "generate":
        tasks = generate(args.tasks, args.cases or None)
        kinds = Counter(task.kind for task in tasks)
        print(f"generated {len(tasks)} tasks: {dict(sorted(kinds.items()))}")
        return 0

    if args.command == "verify":
        problems = verify(args.tasks)
        tasks = load_tasks(args.tasks)
        kinds = Counter(task.kind for task in tasks)
        judged = sum(len(task.judged) for task in tasks)
        unjudged = sorted(
            f"{task.case}/{name}" for task in tasks for name in task.unjudged_expected
        )
        print(f"benchmarks: {len(benchmark_cases())}")
        print(f"tasks:      {len(tasks)} {dict(sorted(kinds.items()))}")
        print(f"judged expected artifacts: {judged}")
        print(f"committed but not judged:  {len(unjudged)} {unjudged}")
        for problem in problems:
            print(f"PROBLEM {problem}")
        print("verify: ok" if not problems else f"verify: {len(problems)} problems")
        return 1 if problems else 0

    tasks = load_tasks(args.tasks, args.cases or None)
    if args.command == "grade":
        verdicts = [
            grade(args.tasks, task, args.run_dir / "workspaces" / task.id)
            for task in tasks
        ]
        write_results(args.run_dir, verdicts)
        print_summary(verdicts, "grade")
        return 0

    if args.command == "calibrate":
        failed = False
        for name, agent in AGENTS.items():
            verdicts = execute(args.tasks, args.run_dir / name, tasks, agent, args.jobs)
            print_summary(verdicts, name)
            want = len(verdicts) if name == "reference" else 0
            got = sum(verdict.passed for verdict in verdicts)
            if got != want:
                failed = True
                for verdict in verdicts:
                    if verdict.passed != (name == "reference"):
                        print(f"  unexpected {verdict.case}: {verdict.findings[:3]}")
        return 1 if failed else 0

    transcripts = args.run_dir / "transcripts"
    agent = opencode_agent(
        args.model, args.opencode, args.timeout, args.variant, transcripts
    )
    verdicts = execute(
        args.tasks, args.run_dir, tasks, agent, args.jobs, transcripts=transcripts
    )
    print_summary(verdicts, args.model)
    return 0


if __name__ == "__main__":
    sys.exit(main())
