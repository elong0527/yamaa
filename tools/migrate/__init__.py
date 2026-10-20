"""The internal 0.5-to-1.0 converter of #3030 Phase 2 (D35), driven by the rename table.

`python -m tools.migrate SOURCE... --output DIR` converts benchmark
directories, project files, YAML files and Markdown files into a separate
target directory, never in place, and prints what needs manual follow-up.
A directory that holds a `benchmark.yaml`, a `spec*.yaml` or a project file
is one unit; any other directory is searched for units, and its loose files
are converted one by one.

What it writes, entry by entry of `tools/migrate/renames.yaml`:

- the header (D4), `model` on entry specifications (P-MODEL-1), and the
  D8 merge of each project's environment files into `type: function` files;
- every rename, deletion, value change and transform of the table: the
  expression merge of `literal`, `compute` and the string form of `source`
  (D18), YAML null for `{literal: null}`, `clear` for a child layer's YAML
  null (P-CLR-1), operand and handler renames, `derive` to `derivations`,
  `tie_break` to `window.order_by`, `closed`, `origins`, `standards`,
  `transport` and the empty mappings of P-HND-4;
- today's inferred join keys, read from CSV headers, Parquet schemas and
  schema producers (D23, D63);
- the spec paths of negative contracts, through the table's `paths` block;
- the YAML blocks of Markdown files.

The converter is temporary: P2-13 deletes it and keeps `renames.yaml`.
"""

import os
from pathlib import Path

import yaml

# Tools run from the repository root, whose yaml/ directory holds the
# specification schemas. Without PyYAML installed, `import yaml` finds that
# directory as an empty namespace package, and the converter would fail later
# on a missing attribute; refuse whatever stands in for PyYAML by name.
if not (hasattr(yaml, "__version__") and hasattr(yaml, "BaseLoader")):
    found = (
        getattr(yaml, "__file__", None)
        or ", ".join(getattr(yaml, "__path__", []))
        or repr(yaml)
    )
    raise ImportError(
        f"tools.migrate needs PyYAML, but `import yaml` found {found}, "
        "which is not PyYAML: install the locked tooling environment with "
        "`uv sync --project python --locked --group tooling --no-install-package yamaa`"
    )

from tools.migrate import table as table_module
from tools.migrate.text import compose, value_of
from tools.migrate.unit import (
    PROJECT_FIELDS,
    Result,
    Unit,
    check_table,
    classify,
)

UNIT_MARKERS = ("benchmark.yaml",)


class SourceError(ValueError):
    pass


def _is_unit(directory):
    if any((directory / name).is_file() for name in UNIT_MARKERS):
        return True
    if list(directory.glob("spec*.yaml")):
        return True
    for path in directory.glob("*.yaml"):
        try:
            root = compose(path.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, yaml.YAMLError):
            continue
        if classify(path, root) == "project":
            return True
    return False


def _skipped(name):
    """Hidden directories and caches hold no source a conversion reads."""
    return name.startswith(".") or name == "__pycache__"


def _files(directory):
    return sorted(
        path
        for path in directory.rglob("*")
        if path.is_file()
        and not any(_skipped(part) for part in path.relative_to(directory).parts[:-1])
    )


def _project_files(project):
    """A project file with its environment files, listed specifications and their parents."""
    found = [project]
    root = compose(project.read_text(encoding="utf-8"))
    value = value_of(root) or {}
    for host_path in (value.get("environments") or {}).values():
        if isinstance(host_path, str):
            found.append((project.parent / host_path).resolve())
    for section in ("sdtm", "adam", "send"):
        for spec in (
            ((value.get(section) or {}).get("specs") or [])
            if isinstance(value.get(section), dict)
            else []
        ):
            path = (project.parent / spec).resolve()
            while path.is_file() and path not in found:
                found.append(path)
                layer = value_of(compose(path.read_text(encoding="utf-8"))) or {}
                for dataset in (layer.get("input") or {}).values():
                    if isinstance(dataset, dict) and isinstance(
                        dataset.get("schema"), str
                    ):
                        producer = (path.parent / dataset["schema"]).resolve()
                        if producer.is_file() and producer not in found:
                            found.append(producer)
                parent = layer.get("parent")
                if not isinstance(parent, str):
                    break
                path = (path.parent / parent).resolve()
    return [path for path in found if path.is_file()]


def convert(sources, base=None, table=None):
    """Convert `sources` and return the Result: files to write, files dropped, notes."""
    table = table or table_module.load()
    check_table(table)
    sources = [Path(source).resolve() for source in sources]
    for source in sources:
        if not source.exists():
            raise SourceError(f"{source} does not exist")
    if base is None:
        anchors = [source.parent for source in sources]
        base = Path(os.path.commonpath(anchors)) if anchors else Path.cwd()
    base = Path(base).resolve()
    result = Result()

    def visit(directory):
        if _is_unit(directory):
            Unit(directory, _files(directory), base, result, table).run()
            return
        loose = []
        for child in sorted(directory.iterdir()):
            if child.is_dir() and not _skipped(child.name):
                visit(child)
            elif child.is_file():
                loose.append(child)
        if loose:
            Unit(None, loose, base, result, table).run()

    files = []
    for source in sources:
        if source.is_dir():
            visit(source)
        elif source.suffix.lower() in (".yaml", ".yml"):
            root = compose(source.read_text(encoding="utf-8"))
            names = set()
            if isinstance(root, yaml.MappingNode):
                names = {key.value for key, _ in root.value}
            if "schema_version" in names and names & PROJECT_FIELDS:
                Unit(source.parent, _project_files(source), base, result, table).run()
            else:
                files.append(source)
        else:
            files.append(source)
    if files:
        Unit(None, files, base, result, table, explicit=True).run()
    return result


def write(result, target, sources=()):
    """Write a Result into `target`, refusing any target that overlaps a source."""
    target = Path(target).resolve()
    for source in sources:
        source = Path(source).resolve()
        if target == source or source in target.parents or target in source.parents:
            raise SourceError(
                f"the target {target} overlaps the source {source}; the converter never writes in place"
            )
    for relative, content in sorted(result.files.items()):
        path = target / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(content, bytes):
            path.write_bytes(content)
        else:
            path.write_text(content, encoding="utf-8", newline="")
    return target


__all__ = ["SourceError", "convert", "write"]
