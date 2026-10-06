"""Independent R017 reachability cases for the shared compiler migration."""

from pathlib import Path

from yamaa import yamaa_domain
from yamaa.schema import resolve_specification
from yamaa.specification.schema import load_schema_bundle

SCHEMA_ROOT = Path(__file__).parents[3] / "yaml"


def test_record_count_retains_its_only_dataset_reference(tmp_path: Path) -> None:
    """REQ-0639 follows COUNT(D.*); dead declarations remain prunable."""
    (tmp_path / "dm.csv").write_text("ID\na\nb\n", encoding="ascii")
    (tmp_path / "ex.csv").write_text("ID\na\na\nb\n", encoding="ascii")
    entry = tmp_path / "spec.yaml"
    entry.write_text(
        """schema_version: "1.0"
parents: []
domain: OUT
keys: [ID]
input: {DM: dm.csv, EX: ex.csv, DEAD: never-read.csv}
base: DM
output: {path: out.csv, columns: [ID, N]}
columns:
  - {name: ID, type: str, derivation: DM.ID}
  - name: N
    type: int
    derivation: {aggregate: {expr: "COUNT(EX.*)"}}
  - name: UNUSED
    type: int
    derivation: {aggregate: {expr: "COUNT(DEAD.*)"}}
""",
        encoding="ascii",
    )

    resolved = resolve_specification(entry, load_schema_bundle(SCHEMA_ROOT))

    assert list(resolved.document["input"]) == ["DM", "EX"]
    assert [column["name"] for column in resolved.document["columns"]] == ["ID", "N"]
    run = yamaa_domain(entry, schema_root=SCHEMA_ROOT)
    assert run.issues.is_empty()
    assert run.output is not None
    assert run.output.to_dicts() == [{"ID": "a", "N": 2}, {"ID": "b", "N": 1}]
