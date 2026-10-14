"""Repository field metadata after whole-source public native admission.

Rust owns the Parquet profile, field/value admission and failure vocabulary.
PyArrow reads physical codec metadata only after that admission succeeds.
"""

import io
import json
from pathlib import Path
from tempfile import TemporaryDirectory

import pyarrow as pa
import pyarrow.parquet as pq
from yamaa import domain


class ParquetProfileFailure(ValueError):
    """The exact shared source refusal, with no host profile decisions."""

    def __init__(self, condition, requirement, context):
        self.condition = condition
        self.requirement = requirement
        self.context = context
        super().__init__(f"{condition}: {context}")


def source_issue(content):
    """Admit every source field and value using an owned ordinal-only build."""
    with TemporaryDirectory(prefix="yamaa-validator-parquet-") as temporary:
        root = Path(temporary)
        (root / "source.parquet").write_bytes(content)
        specification = root / "spec.yaml"
        specification.write_text(
            json.dumps(
                {
                    "schema_version": "1.0",
                    "domain": "VALIDATOR",
                    "keys": ["ROWID"],
                    "input": {
                        "SOURCE": {
                            "path": "source.parquet",
                            "ordinal": "ValidatorOrdinal",
                        }
                    },
                    "columns": [
                        {
                            "name": "ROWID",
                            "type": "int",
                            "label": "Row identity",
                            "derivation": "SOURCE.ValidatorOrdinal",
                        }
                    ],
                    "output": {"path": "result.csv", "columns": ["ROWID"]},
                }
            ),
            encoding="ascii",
        )
        result = domain(specification)
        rows = result.issues.to_dicts()
        if not rows:
            return None
        if len(rows) == 1:
            row = rows[0]
            if row["condition"] == "ordinal_field_collision":
                # Complete source profile admission precedes the generated field.
                return None
            if row["condition"] in {
                "source_parquet_invalid",
                "source_field_name_empty",
                "source_field_name_duplicate",
                "source_field_type_unsupported",
                "source_field_value_invalid",
            }:
                return row["condition"], row["requirement"], json.loads(row["context"])
        raise RuntimeError(f"native Parquet fixture refused: {rows!r}")


def parquet_source_fields(content):
    """Return physical metadata and field carrier names for an admitted source."""
    issue = source_issue(content)
    if issue is not None:
        raise ParquetProfileFailure(*issue)
    # This is representation translation for an already admitted closed profile.
    # No logical metadata, values, ranges, names or profile rules are validated here.
    carriers = {
        pa.string(): "str",
        pa.large_string(): "str",
        pa.int64(): "int",
        pa.float64(): "float",
        pa.date32(): "date",
        pa.date64(): "date",
        pa.timestamp("us"): "datetime",
    }
    metadata = pq.ParquetFile(io.BytesIO(content), arrow_extensions_enabled=False)
    fields = []
    for field in metadata.schema_arrow:
        kind = carriers.get(field.type)
        if kind is None:
            raise RuntimeError(
                f"admitted native field has an unknown codec carrier: {field.type}"
            )
        fields.append((field.name, kind))
    return metadata, tuple(fields)
