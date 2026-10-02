from __future__ import annotations

from pathlib import Path

import pytest

from yamaa.io import ProjectResources, load_source_tables
from yamaa.runtime import ExecutionSuccess, execute_with_source_provider
from yamaa.specification import load_specification

SCHEMA_ROOT = Path(__file__).parents[3] / "yaml"

# Each group carries several long-form records, and one column reads the
# value of the TARGET record alone.
SPECIFICATION = """schema_version: "1.0"
domain: EXAMPLE
keys: [GROUP_ID]
input:
  SOURCE: input/source.csv
output:
  path: example.csv
  columns: [GROUP_ID, TARGET_VALUE]
  order_by: [GROUP_ID]
columns:
  - name: GROUP_ID
    type: str
    label: Group Identifier
    derivation: SOURCE.GroupID
  - name: TARGET_VALUE
    type: str
    label: Target Value
    derivation:
      source:
        filter: "SOURCE.RecordKind = 'TARGET'"
        variable: SOURCE.Value
"""

G1_UNRELATED = "G1,UNRELATED,g1-unrelated"
G1_TARGET = "G1,TARGET,"
G2_UNRELATED = "G2,UNRELATED,g2-unrelated"
G2_TARGET = "G2,TARGET,g2-target"


def _run(tmp_path: Path, records: list[str]) -> list[tuple[object, ...]]:
    (tmp_path / "input").mkdir(exist_ok=True)
    (tmp_path / "input/source.csv").write_text(
        "GroupID,RecordKind,Value\n" + "".join(f"{line}\n" for line in records)
    )
    (tmp_path / "spec.yaml").write_text(SPECIFICATION)
    specification = load_specification(tmp_path / "spec.yaml", SCHEMA_ROOT)
    resources = ProjectResources(tmp_path)
    result = execute_with_source_provider(
        specification.specification,
        lambda datasets: load_source_tables(datasets, resources),
    )
    assert isinstance(result, ExecutionSuccess)
    return result.artifact.frame.rows()


@pytest.mark.parametrize(
    "records",
    [
        [G1_UNRELATED, G1_TARGET, G2_UNRELATED, G2_TARGET],
        [G1_TARGET, G1_UNRELATED, G2_TARGET, G2_UNRELATED],
        [G2_UNRELATED, G1_UNRELATED, G2_TARGET, G1_TARGET],
    ],
)
def test_an_empty_target_never_inherits_another_records_value(
    tmp_path: Path, records: list[str]
) -> None:
    # #1567: G1's TARGET record carries no value, so its row is missing in
    # whatever order the unselected records arrive, and G2 keeps its own.
    assert _run(tmp_path, records) == [("G1", None), ("G2", "g2-target")]
