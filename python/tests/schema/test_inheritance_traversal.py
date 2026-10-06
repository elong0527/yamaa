"""Independent traversal and source-identity requirements for R017 inheritance."""

from pathlib import Path

import pytest

from yamaa.schema import resolve_specification
from yamaa.specification import SpecificationError
from yamaa.specification.schema import load_schema_bundle

SCHEMA_ROOT = Path(__file__).parents[3] / "yaml"


@pytest.mark.parametrize("returns_to_entry", [True, False])
def test_cycle_reports_the_complete_canonical_path_cycle(
    tmp_path: Path, returns_to_entry: bool
) -> None:
    """REQ-0655 reports the cycle itself, including its repeated endpoint."""
    (tmp_path / "nested").mkdir()
    entry, first, second = [
        tmp_path / name for name in ("entry.yaml", "a.yaml", "b.yaml")
    ]
    entry.write_text('schema_version: "1.0"\nparents: a.yaml\n', encoding="ascii")
    first.write_text('schema_version: "1.0"\nparents: b.yaml\n', encoding="ascii")
    repeated = entry if returns_to_entry else first
    second.write_text(
        f'schema_version: "1.0"\nparents: nested/../{repeated.name}\n', encoding="ascii"
    )
    cycle = (
        [entry, first, second, entry] if returns_to_entry else [first, second, first]
    )

    with pytest.raises(SpecificationError) as caught:
        resolve_specification(entry, load_schema_bundle(SCHEMA_ROOT))

    assert len(caught.value.diagnostics) == 1
    diagnostic = caught.value.diagnostics[0]
    assert diagnostic.phase == "validation"
    assert diagnostic.condition == "inheritance_cycle"
    assert diagnostic.requirement == "REQ-0655"
    assert diagnostic.spec_paths == ("parents",)
    assert diagnostic.context["cycle"] == [str(path.resolve()) for path in cycle]
    assert diagnostic.context["reason"] == (
        "parent_chain_returns_to_entry"
        if returns_to_entry
        else "parent_chain_returns_to_active_layer"
    )


def test_path_rebasing_uses_captured_canonical_directories_without_more_io(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Keep source identity fixed while respelling data paths (REQ-0618/0629)."""
    from yamaa.schema.inheritance import _rebase_path

    root = tmp_path.resolve()
    layer = root / "shared" / "base.yaml"
    entry = root / "study" / "entry.yaml"

    def unexpected_resolve(*args, **kwargs):
        raise AssertionError("canonical directory authority was reopened")

    monkeypatch.setattr(Path, "resolve", unexpected_resolve)
    assert _rebase_path("input/../data.csv", layer, entry) == "../shared/data.csv"
    assert _rebase_path("unchanged.csv", entry, entry) == "unchanged.csv"


@pytest.mark.parametrize(
    (
        "entry_document",
        "parent_document",
        "condition",
        "path",
        "requirement",
        "context",
        "source",
        "include_entry",
    ),
    [
        (
            {"schema_version": "0.0", "parents": []},
            None,
            "schema_version_mismatch",
            "schema_version",
            "REQ-0245",
            {"expected": "1.0", "actual": "0.0"},
            "entry",
            True,
        ),
        (
            {"parents": []},
            None,
            "schema_version_mismatch",
            "schema_version",
            "REQ-0656",
            {"expected": "1.0", "actual": None},
            "entry",
            True,
        ),
        (
            {"schema_version": "1.0", "parents": "parent.yaml"},
            {"schema_version": "0.0"},
            "schema_version_mismatch",
            "parents",
            "REQ-0656",
            {"entry_version": "1.0", "parent_version": "0.0"},
            "parent",
            True,
        ),
        (
            {"schema_version": "1.0", "parents": "parent.yaml"},
            {"parents": []},
            "schema_version_mismatch",
            "schema_version",
            "REQ-0656",
            {"expected": "1.0", "actual": None},
            "parent",
            True,
        ),
        (
            {"schema_version": "1.0", "parents": "parent.yaml"},
            {"schema_version": "1.0", "parents": "https://example.test/base.yaml"},
            "invalid_parent_path",
            "parents",
            "REQ-0653",
            {"reason": "remote_reference", "parent": "https://example.test/base.yaml"},
            "parent",
            False,
        ),
        (
            {"schema_version": "1.0", "parents": "missing.yaml"},
            None,
            "parent_not_found",
            "parents",
            "REQ-0654",
            {"path": "missing.yaml"},
            "entry",
            False,
        ),
        (
            {"schema_version": "1.0", "parents": "parent.yaml"},
            {"schema_version": "1.0", "unexpected": "value"},
            "unknown_field",
            "unexpected",
            "REQ-0658",
            {"field": "unexpected", "class": "root_class"},
            "parent",
            True,
        ),
    ],
)
def test_traversal_failures_retain_implicated_file_identities(
    tmp_path,
    entry_document,
    parent_document,
    condition,
    path,
    requirement,
    context,
    source,
    include_entry,
):
    """REQ-0653/0654/0656 identify the declaring/invalid layer and entry where relevant."""
    import json

    root = tmp_path.resolve()
    entry, parent = root / "entry.yaml", root / "parent.yaml"
    entry.write_text(json.dumps(entry_document), encoding="ascii")
    if parent_document is not None:
        parent.write_text(json.dumps(parent_document), encoding="ascii")
    expected = {**context, "source": str(entry if source == "entry" else parent)}
    if include_entry:
        expected["entry"] = str(entry)
    with pytest.raises(SpecificationError) as caught:
        resolve_specification(entry, load_schema_bundle(SCHEMA_ROOT))
    assert [d.model_dump(mode="json") for d in caught.value.diagnostics] == [
        {
            "phase": "validation",
            "condition": condition,
            "spec_paths": [path],
            "requirement": requirement,
            "context": expected,
        }
    ]


def test_parent_read_failure_retains_declaring_file_and_requested_path(
    tmp_path, monkeypatch
):
    """A read failure after canonicalization keeps both source and path (REQ-0654)."""
    from yamaa.schema import inheritance

    root = tmp_path.resolve()
    entry, parent = root / "entry.yaml", root / "parent.yaml"
    entry.write_text('schema_version: "1.0"\nparents: parent.yaml\n', encoding="ascii")
    parent.write_text('schema_version: "1.0"\n', encoding="ascii")
    original = inheritance.read_bundle_document
    reads = []

    def read(path, bundle):
        reads.append(path)
        if path == parent:
            raise OSError("unreadable parent")
        return original(path, bundle)

    monkeypatch.setattr(inheritance, "read_bundle_document", read)
    with pytest.raises(SpecificationError) as caught:
        resolve_specification(entry, load_schema_bundle(SCHEMA_ROOT))
    assert caught.value.diagnostics[0].context == {
        "source": str(entry),
        "path": str(parent),
    }
    assert reads == [entry, parent]
