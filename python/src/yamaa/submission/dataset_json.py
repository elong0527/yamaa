"""Dataset-JSON 1.1 from a published artifact, with a fixed byte spelling."""

from __future__ import annotations

import json
from functools import cache
from pathlib import Path
from typing import Any

from jsonschema import Draft202012Validator

from yamaa.io.polars import runtime_value
from yamaa.models import MISSING, DateTimeValue, DateValue, TypedTable
from yamaa.specification import SpecificationError, ValidationDiagnostic
from yamaa.submission.composition import ComposedDataset, ComposedStudy
from yamaa.submission.define_xml import scalar_text


def _json(value: Any) -> str:
    if value is None or value is MISSING:
        return "null"
    if isinstance(value, (DateValue, DateTimeValue)):
        value = value.to_text()
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        return scalar_text(value)
    if isinstance(value, list):
        return "[" + ", ".join(_json(item) for item in value) + "]"
    if isinstance(value, dict):
        return (
            "{"
            + ", ".join(_json(key) + ": " + _json(item) for key, item in value.items())
            + "}"
        )
    raise TypeError(f"not a Dataset-JSON value: {type(value).__name__}")


@cache
def _schema() -> Draft202012Validator:
    source = Path(__file__).parent / "schemas" / "dataset.schema.json"
    schema = json.loads(source.read_text(encoding="utf-8"))
    Draft202012Validator.check_schema(schema)
    return Draft202012Validator(schema)


def render_dataset_json(
    study: ComposedStudy, dataset: ComposedDataset, table: TypedTable
) -> bytes:
    """Keep the producing artifact's values and order (REQ-1199..1200)."""
    doc = study.document
    submission = dataset.specification.submission
    assert submission is not None
    obj: dict[str, Any] = {
        "datasetJSONCreationDateTime": doc["creation_datetime"],
        "datasetJSONVersion": "1.1.0",
        "fileOID": doc["file_oid"] + "." + dataset.id,
    }
    if doc.get("originator"):
        obj["originator"] = doc["originator"]
    if doc.get("source_system"):
        obj["sourceSystem"] = {
            "name": doc["source_system"],
            "version": doc["source_system_version"],
        }
    obj.update(
        {
            "studyOID": "STDY." + doc["study"]["id"],
            "metaDataVersionOID": "MDV." + doc["metadata_version"]["id"],
            "metaDataRef": study.output_path.name,
            "itemGroupOID": "IG." + dataset.id,
            "records": table.frame.height,
            "name": dataset.id,
            "label": submission.label,
        }
    )
    columns: list[dict[str, Any]] = []
    for column in dataset.columns:
        meta = column.submission
        assert meta is not None
        datatype = {
            "text": "string",
            "integer": "integer",
            "float": "float",
            "date": "date",
            "datetime": "datetime",
            "time": "time",
            "URI": "URI",
        }.get(meta.data_type, "string")
        col = {
            "itemOID": f"IT.{dataset.id}.{column.name}",
            "name": column.name,
            "label": column.label,
            "dataType": datatype,
        }
        if meta.length is not None:
            col["length"] = meta.length
        if meta.display_format is not None:
            col["displayFormat"] = meta.display_format
        if column.name in dataset.specification.keys:
            col["keySequence"] = dataset.specification.keys.index(column.name) + 1
        columns.append(col)
    obj["columns"] = columns
    obj["rows"] = [
        [runtime_value(value) for value in row] for row in table.frame.iter_rows()
    ]
    lines = ["{"]
    for index, (key, value) in enumerate(obj.items()):
        comma = "," if index < len(obj) - 1 else ""
        if key in {"columns", "rows"} and value:
            lines.append(f"  {_json(key)}: [")
            lines.extend(
                "    " + _json(item) + ("," if item_index < len(value) - 1 else "")
                for item_index, item in enumerate(value)
            )
            lines.append("  ]" + comma)
        else:
            lines.append(f"  {_json(key)}: {_json(value)}{comma}")
    lines.append("}")
    content = ("\n".join(lines) + "\n").encode("utf-8")
    errors = list(_schema().iter_errors(json.loads(content)))
    if errors:
        raise SpecificationError(
            [
                ValidationDiagnostic(
                    condition="value_not_permitted",
                    spec_paths=(f"datasets.{dataset.id}.dataset_json",),
                    requirement="REQ-1231",
                    context={"schema_violation": error.message},
                )
                for error in errors
            ]
        )
    return content
