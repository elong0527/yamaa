"""Repository source facts from the installed, held native engine capability.

Static admission and header/type binding accept no project activation, cell
execution or Save authority. Source bytes enter only by an explicitly requested
native descriptor slot; profile fixtures contain no project functions.
"""

import csv
import io
import json
from pathlib import Path

import _native_csv
import _native_parquet
import yaml12
from yamaa._native import _prepare_file_project

import yaml

ROOT = Path(__file__).resolve().parents[3]
MAX_CAPTURE = 8_388_608


def schema_closure(directory, entry):
    pending, captured = [entry], {}
    while pending:
        name = pending.pop(0)
        if name not in captured:
            source = (directory / name).read_bytes()
            captured[name] = source
            pending.extend(yaml.safe_load(source).get("includes", []))
    return list(captured.items())


def prepare(spec_path, *, project_root=None, environment=None, schema_root=ROOT):
    spec_path = Path(spec_path).absolute()
    root = Path(project_root or spec_path.parent).absolute()
    directory = schema_root / "yaml"
    environment_directory = directory
    if environment is None and not (directory / "schema_environment.yaml").exists():
        # A custom structural fixture need not author an unused environment
        # schema. No environment document or runtime is created by this closure.
        environment_directory = ROOT / "yaml"
    return _prepare_file_project(
        str(root),
        str(spec_path.parent),
        spec_path.name,
        str(environment) if environment is not None else None,
        [],
        schema_closure(directory, "schema.yaml"),
        0,
        schema_closure(environment_directory, "schema_environment.yaml"),
        0,
    )


def issue_rows(
    spec_path, *, project_root=None, environment=None, schema_root=ROOT, origins=None
):
    owner = prepare(
        spec_path,
        project_root=project_root,
        environment=environment,
        schema_root=schema_root,
    )
    static = owner.check_issues()
    if origins is not None:
        origins.update(owner.diagnostic_origins())
    if static or owner.preparation_status() != "ready":
        return static
    sources = []
    for index, (identity, dataset, declaring_source, written, profile) in enumerate(
        owner.input_metadata()
    ):
        raw, issues = owner.capture_input_metadata(index, MAX_CAPTURE)
        if issues:
            return issues
        if profile == "csv":
            refused = _native_csv.profile_issues(raw)
            if refused:
                observations = []
                for row in refused:
                    context = json.loads(row["context"])
                    if "dataset" in context:
                        context["dataset"] = dataset
                    if "path" in context:
                        context["path"] = written
                    observations.append(
                        (
                            row["phase"],
                            row["condition"],
                            row["requirement"],
                            [f"input.{dataset}.path"],
                            json.dumps(context),
                        )
                    )
                return observations
            names = next(csv.reader(io.StringIO(raw.decode("utf-8"), newline="")))
            fields = [(name, "str") for name in names]
        else:
            try:
                _, fields = _native_parquet.parquet_source_fields(raw)
            except _native_parquet.ParquetProfileFailure as failure:
                return [
                    (
                        "validation",
                        failure.condition,
                        failure.requirement,
                        [f"input.{dataset}.path"],
                        json.dumps(failure.context),
                    )
                ]
        sources.append((identity, dataset, list(fields)))
    return owner.check_source_metadata(sources)


DEFERRED_ENVIRONMENTS = set()


def r_static_rows(specification, environment):
    """Only a real installed R runner can admit its authored environment lane."""
    import shutil
    import subprocess

    runner = shutil.which("Rscript")
    if runner is None:
        DEFERRED_ENVIRONMENTS.add(str(environment))
        return None
    available = subprocess.run(
        [
            runner,
            "--vanilla",
            "-e",
            (
                'quit(status=if (requireNamespace("yamaa", quietly=TRUE) && '
                'requireNamespace("jsonlite", quietly=TRUE)) 0L else 9L)'
            ),
        ],
        capture_output=True,
        check=False,
    )
    authored = yaml.safe_load(Path(environment).read_bytes())
    lock = authored.get("lock") if isinstance(authored, dict) else None
    if available.returncode != 0 or (
        isinstance(lock, str) and not (Path(environment).parent / lock).is_file()
    ):
        # Qualification stages the real SDK lock and installed package. A bare
        # R executable or absent benchmark lock supplies no real-host witness.
        DEFERRED_ENVIRONMENTS.add(str(environment))
        return None
    program = (
        "args <- commandArgs(trailingOnly=TRUE); "
        "issues <- yamaa::yamaa_check(args[[1]], environment=args[[2]]); "
        "issues$spec_paths <- lapply(issues$spec_paths, I); "
        'jsonlite::write_json(issues, stdout(), dataframe="rows", auto_unbox=TRUE, na="null")'
    )
    completed = subprocess.run(
        [runner, "--vanilla", "-e", program, str(specification), str(environment)],
        text=True,
        capture_output=True,
        check=True,
    )
    values = json.loads(completed.stdout)
    return [
        (
            value["phase"],
            value["condition"],
            value["requirement"],
            value["spec_paths"],
            value["context"],
        )
        for value in values
    ]


def validate(
    specification,
    spec_path,
    *,
    project_root=None,
    environments=(),
    schema_root=ROOT,
    origins=None,
):
    """Native declaration/profile/header facts; R coverage is explicitly scoped."""
    import shutil
    from contextlib import ExitStack
    from tempfile import TemporaryDirectory

    spec_path = Path(spec_path).absolute()
    project_root = Path(project_root or spec_path.parent).absolute()
    python_environment = None
    r_environments = []
    for environment in environments:
        authored = yaml.safe_load(Path(environment).read_bytes())
        if isinstance(authored, dict) and authored.get("language") == "r":
            r_environments.append(Path(environment).absolute())
        elif python_environment is None:
            python_environment = Path(environment).absolute()
    with ExitStack() as stack:
        # Complete source files stay authoritative. Focused dict fixtures may
        # supply source text without an existing file; their owned staging keeps
        # the original relative layout and native resource policy, including links.
        existing = None
        try:
            existing = yaml12.parse_yaml(spec_path.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, yaml.YAMLError):
            pass
        needs_staging = existing != specification
        if python_environment is not None:
            environment = yaml.safe_load(python_environment.read_bytes())
            needs_staging |= (
                isinstance(environment, dict)
                and environment.get("lock") == "uv.lock"
                and not (python_environment.parent / "uv.lock").exists()
            )
        relocated = None
        if needs_staging:
            temporary = Path(
                stack.enter_context(
                    TemporaryDirectory(prefix="yamaa-validator-metadata-")
                )
            )
            relocated = temporary / "project"
            if project_root.exists():
                shutil.copytree(project_root, relocated, symlinks=True)
            else:
                relocated.mkdir()
            relative = spec_path.relative_to(project_root)
            staged = relocated / relative
            staged.parent.mkdir(parents=True, exist_ok=True)
            if existing != specification:
                staged.write_text(json.dumps(specification), encoding="ascii")
            if python_environment is not None:
                python_environment = relocated / python_environment.relative_to(
                    project_root
                )
                environment = yaml.safe_load(python_environment.read_bytes())
                if (
                    isinstance(environment, dict)
                    and environment.get("lock") == "uv.lock"
                ):
                    # Reuse the real authored SDK lock; no benchmark lock is fabricated.
                    lock = schema_root / "python/uv.lock"
                    if lock.exists():
                        shutil.copyfile(lock, python_environment.parent / "uv.lock")
            original_root, project_root, spec_path = project_root, relocated, staged
        else:
            original_root = project_root
        rows = []
        if python_environment is not None or not r_environments:
            rows.extend(
                issue_rows(
                    spec_path,
                    project_root=project_root,
                    environment=python_environment,
                    schema_root=schema_root,
                    origins=origins,
                )
            )
        for environment in r_environments:
            observed = r_static_rows(spec_path, environment)
            if observed is not None:
                rows.extend(observed)
            # The required R qualification owns source/binding/runtime coverage;
            # a public metadata-only R check does not prove those later facts.
            DEFERRED_ENVIRONMENTS.add(str(environment))
        if relocated is not None:
            if origins is not None:
                origins.update(
                    (path, source.replace(str(relocated), str(original_root)))
                    for path, source in list(origins.items())
                )
            rows = [
                (
                    phase,
                    condition,
                    requirement,
                    paths,
                    context.replace(str(relocated), str(original_root)),
                )
                for phase, condition, requirement, paths, context in rows
            ]
        return list(
            dict.fromkeys(
                (phase, condition, requirement, tuple(paths), context)
                for phase, condition, requirement, paths, context in rows
            )
        )
