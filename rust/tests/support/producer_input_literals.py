"""Unchanged original authored input literals; no reference evaluator calls."""

from pathlib import Path

SOURCE_REVISION = "c7b88a4fd3a593d5742c9449a23bbdb75291ee20"
SOURCE_PATH = "python/tests/planning/test_workflow.py"


def test_shared_producer_executes_once_and_shares_generated_snapshot(
    tmp_path: Path,
) -> None:
    (tmp_path / "seed.csv").write_text("ID\n01\n", encoding="ascii")
    (tmp_path / "producer.yaml").write_text(
        """schema_version: "1.0"
domain: PRODUCED
input: {SEED: seed.csv}
base: SEED
keys: [ID]
output: {path: produced.parquet, columns: [ID]}
columns:
  - name: ID
    type: str
    label: Identifier
    derivation: {source: SEED.ID}
""",
        encoding="ascii",
    )
    (tmp_path / "spec.yaml").write_text(
        """schema_version: "1.0"
domain: OUT
input:
  A: {path: produced.parquet, schema: producer.yaml}
  B: {path: produced.parquet, schema: producer.yaml}
base: A
keys: [ID]
output: {path: out.csv, columns: [ID]}
columns:
  - name: ID
    type: str
    label: Identifier
    derivation: {source: A.ID}
""",
        encoding="ascii",
    )
