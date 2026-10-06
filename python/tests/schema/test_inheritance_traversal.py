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
