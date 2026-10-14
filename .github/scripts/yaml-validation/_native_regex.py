"""R022 repository assertions through installed public native specifications.

The public check admits patterns and declares capture bounds. Owned builds read
only a generated one-row source and expose extraction results without saving.
"""

import json
from functools import lru_cache
from pathlib import Path
from tempfile import TemporaryDirectory

from yamaa import check, domain

import yaml


class RegexError(ValueError):
    def __init__(self, reason):
        super().__init__(reason)
        self.reason = reason


def _specification(pattern, subject, groups, no_match):
    columns = [
        {"name": "ID", "type": "str", "label": "ID", "derivation": "SOURCE.ID"},
        {
            "name": "TEXT",
            "type": "str",
            "label": "TEXT",
            "derivation": {"literal": subject},
        },
    ]
    for group in groups:
        columns.append(
            {
                "name": f"GROUP{group}",
                "type": "str",
                "label": f"Group {group}",
                "derivation": {
                    "str_extract": {
                        "source": "TEXT",
                        "pattern": pattern,
                        "group": group,
                        "missing": None,
                        "no_match": no_match,
                    }
                },
            }
        )
    return {
        "schema_version": "1.0",
        "domain": "VALIDATOR",
        "keys": ["ID"],
        "input": {"SOURCE": "source.csv"},
        "output": {
            "path": "result.csv",
            "columns": ["ID", *(f"GROUP{group}" for group in groups)],
        },
        "columns": columns,
    }


def _write(root, specification):
    (root / "source.csv").write_text("ID\n1\n", encoding="ascii")
    path = root / "spec.yaml"
    path.write_text(
        yaml.safe_dump(specification, allow_unicode=False), encoding="ascii"
    )
    return path


@lru_cache(maxsize=4096)
def capture_group_count(pattern):
    """Read the public out-of-range diagnostic's declared group count."""
    with TemporaryDirectory(prefix="yamaa-validator-pattern-") as temporary:
        path = _write(Path(temporary), _specification(pattern, "a", [2**31 - 1], None))
        issues = check(path).issues.to_dicts()
    if len(issues) == 1:
        issue = issues[0]
        context = json.loads(issue["context"])
        if issue["condition"] == "invalid_regex":
            raise RegexError(context["reason"])
        if issue["condition"] == "regex_group_out_of_range":
            return context["group_count"]
    raise RuntimeError(f"native pattern admission refused: {issues!r}")


class _Match:
    def __init__(self, groups):
        self._groups = groups

    def group(self, index):
        return self._groups[index]


class _Pattern:
    def __init__(self, source):
        self.source = source
        self.group_count = capture_group_count(source)

    def search(self, subject):
        return _search(self.source, subject, self.group_count)


@lru_cache(maxsize=8192)
def _search(pattern, subject, group_count):
    no_match = "<yamaa-validator-no-match>"
    while no_match in subject:
        no_match += "!"
    groups = list(range(group_count + 1))
    with TemporaryDirectory(prefix="yamaa-validator-match-") as temporary:
        root = Path(temporary)
        result = domain(
            _write(root, _specification(pattern, subject, groups, no_match))
        )
        issues = result.issues.to_dicts()
        if issues:
            raise RuntimeError(f"native regex fixture refused: {issues!r}")
        output = result.output
        if output is None or output.height != 1:
            raise RuntimeError("native regex fixture did not produce its owned row")
        row = output.row(0, named=True)
        values = [row[f"GROUP{group}"] for group in groups]
    return None if values[0] == no_match else _Match(values)


def compile_pattern(pattern):
    return _Pattern(pattern)


def full_match(pattern, subject):
    return compile_pattern("^(?:" + pattern + ")$").search(subject) is not None
