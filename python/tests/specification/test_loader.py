from __future__ import annotations

import shutil
from pathlib import Path

import pytest

from yamaa.specification import SpecificationError, load_specification
from yamaa.specification._yaml import read_yaml_document
from yamaa.specification.schema import (
    load_schema_bundle,
    normalize_specification,
)

REPOSITORY_ROOT = Path(__file__).parents[3]
SCHEMA_ROOT = REPOSITORY_ROOT / "yaml"
EXAMPLES = SCHEMA_ROOT / "examples"


def _copy_basic_specification(tmp_path: Path) -> tuple[Path, str]:
    source = (EXAMPLES / "sdtm-dm-basic/spec.yaml").read_text(encoding="ascii")
    path = tmp_path / "spec.yaml"
    return path, source


def _copy_schema_bundle(tmp_path: Path) -> Path:
    schema_root = tmp_path / "schema"
    schema_root.mkdir()
    for source_path in SCHEMA_ROOT.glob("schema*.yaml"):
        shutil.copyfile(source_path, schema_root / source_path.name)
    return schema_root


def _mutate_schema(
    tmp_path: Path,
    filename: str,
    old: str,
    new: str,
) -> Path:
    schema_root = _copy_schema_bundle(tmp_path)
    schema_path = schema_root / filename
    source = schema_path.read_text(encoding="ascii")
    assert old in source
    schema_path.write_text(source.replace(old, new, 1), encoding="ascii")
    return schema_root


def test_loads_and_normalizes_basic_specification() -> None:
    loaded = load_specification(EXAMPLES / "sdtm-dm-basic/spec.yaml", SCHEMA_ROOT)
    specification = loaded.specification

    assert specification.schema_version == "1.0"
    assert specification.domain == "DM"
    assert specification.datasets["ODM"].path == "input/odm.csv"
    assert loaded.origin_path == (EXAMPLES / "sdtm-dm-basic/spec.yaml").resolve()
    assert loaded.schema_path == (SCHEMA_ROOT / "schema.yaml").resolve()

    columns = {column.name: column for column in specification.columns}
    assert columns["STUDYID"].derivation is not None
    assert columns["STUDYID"].derivation.value.root == {
        "source": {"variable": "ODM.StudyOID"}
    }
    assert columns["SEX"].derivation is not None
    mapping = columns["SEX"].derivation.value.root["mapping"]
    assert isinstance(mapping, dict)
    assert mapping["case_sensitive"] is True
    assert columns["AGE"].derivation is not None
    assert columns["AGE"].derivation.value.root == {
        "source": {"variable": "ODM.IT.DM.AGE", "missing": None}
    }


def test_recursive_alias_tries_a_later_union_member(tmp_path: Path) -> None:
    schema_root = _mutate_schema(
        tmp_path,
        "schema.yaml",
        "type: domain_name",
        "type: recursive_domain",
    )
    schema_path = schema_root / "schema.yaml"
    source = schema_path.read_text(encoding="ascii")
    schema_path.write_text(
        f"{source}\nrecursive_domain:\n    type: [recursive_domain, domain_name]\n",
        encoding="ascii",
    )

    loaded = load_specification(EXAMPLES / "sdtm-dm-basic/spec.yaml", schema_root)

    assert loaded.specification.domain == "DM"


def test_normalizes_schema_defaults_with_collection_shorthand(tmp_path: Path) -> None:
    schema_root = _mutate_schema(
        tmp_path,
        "schema.yaml",
        '    - parents:\n        type: [path, "list[path]"]\n        required: false\n',
        "    - parents:\n"
        '        type: [path, "list[path]"]\n'
        "        required: false\n"
        "        default: parent.yaml\n",
    )

    loaded = load_specification(EXAMPLES / "sdtm-dm-basic/spec.yaml", schema_root)

    assert loaded.specification.parents == ["parent.yaml"]


def test_does_not_expand_collection_shorthand_in_larger_union(
    tmp_path: Path,
) -> None:
    schema_root = _mutate_schema(
        tmp_path,
        "schema.yaml",
        "type: domain_name",
        'type: [str, "list[str]", int]',
    )

    loaded = load_specification(EXAMPLES / "sdtm-dm-basic/spec.yaml", schema_root)

    assert loaded.specification.domain == "DM"


def test_expands_list_valued_collection_shorthand(tmp_path: Path) -> None:
    schema_root = _mutate_schema(
        tmp_path,
        "schema.yaml",
        "type: domain_name",
        'type: [items, "list[items]"]',
    )
    schema_path = schema_root / "schema.yaml"
    schema_path.write_text(
        schema_path.read_text(encoding="ascii") + "\nitems:\n    type: list[str]\n",
        encoding="ascii",
    )

    bundle = load_schema_bundle(schema_root)
    normalized = normalize_specification(
        {"domain": ["DM"]},
        bundle,
    )

    assert normalized == {"domain": [["DM"]]}


def test_does_not_expand_class_shorthand_when_field_is_a_union(
    tmp_path: Path,
) -> None:
    schema_root = _mutate_schema(
        tmp_path,
        "schema.yaml",
        "type: domain_name",
        "type: [str, domain_wrapper]",
    )
    schema_path = schema_root / "schema.yaml"
    source = schema_path.read_text(encoding="ascii")
    schema_path.write_text(
        f"{source}\n"
        "domain_wrapper:\n"
        "    - value: {type: [str, int], required: true}\n"
        "    - label: {type: str, default: domain}\n",
        encoding="ascii",
    )

    bundle = load_schema_bundle(schema_root)
    normalized = normalize_specification({"domain": "DM"}, bundle)

    assert normalized == {"domain": "DM"}


def test_negative_column_type_matches_committed_diagnostic() -> None:
    with pytest.raises(SpecificationError) as caught:
        load_specification(
            EXAMPLES / "negative-column-type-unknown/spec.yaml",
            SCHEMA_ROOT,
        )

    assert [item.model_dump(mode="json") for item in caught.value.diagnostics] == [
        {
            "phase": "validation",
            "condition": "value_not_permitted",
            "spec_paths": ["columns.AVAL.type"],
            "context": {
                "value": "number",
                "permitted": ["str", "int", "float", "date", "datetime"],
            },
        }
    ]


def test_negative_nested_expression_matches_committed_diagnostic() -> None:
    with pytest.raises(SpecificationError) as caught:
        load_specification(
            EXAMPLES / "negative-variable-nested-expression/spec.yaml",
            SCHEMA_ROOT,
        )

    assert caught.value.diagnostics[0].model_dump(mode="json") == {
        "phase": "validation",
        "condition": "invalid_field_type",
        "spec_paths": ["columns.COUNTRY.derivation.str_upper.source"],
        "context": {"expected": "variable", "actual": "mapping"},
    }


def test_rejects_unknown_fields_with_a_stable_path(tmp_path: Path) -> None:
    path, source = _copy_basic_specification(tmp_path)
    path.write_text(
        source.replace("domain: DM\n", "domain: DM\nunexpected: value\n"),
        encoding="ascii",
    )

    with pytest.raises(SpecificationError) as caught:
        load_specification(path, SCHEMA_ROOT)

    assert caught.value.diagnostics[-1].model_dump(mode="json") == {
        "phase": "validation",
        "condition": "unknown_field",
        "spec_paths": ["unexpected"],
        "context": {"field": "unexpected", "class": "root_class"},
    }


def test_rejects_unknown_registry_operations_with_a_stable_path(
    tmp_path: Path,
) -> None:
    path, source = _copy_basic_specification(tmp_path)
    path.write_text(
        source.replace(
            "      source: ODM.StudyOID\n",
            "      unknown_operation: ODM.StudyOID\n",
            1,
        ),
        encoding="ascii",
    )

    with pytest.raises(SpecificationError) as caught:
        load_specification(path, SCHEMA_ROOT)

    assert caught.value.diagnostics[0].model_dump(mode="json") == {
        "phase": "validation",
        "condition": "unknown_operation",
        "spec_paths": ["columns.STUDYID.derivation.unknown_operation"],
        "context": {
            "registry": "expressions",
            "operation": "unknown_operation",
        },
    }


def test_accepts_integer_for_float_schema_field(tmp_path: Path) -> None:
    path, source = _copy_basic_specification(tmp_path)
    path.write_text(
        source.replace(
            "    label: Study Identifier\n",
            "    label: Study Identifier\n"
            "    verifications:\n"
            "      - range: {min: 1}\n",
            1,
        ),
        encoding="ascii",
    )

    loaded = load_specification(path, SCHEMA_ROOT)

    verification = loaded.specification.columns[0].verifications
    assert verification is not None
    assert verification[0].root == {"range": {"min": 1}}


def test_reports_invalid_schema_patterns(tmp_path: Path) -> None:
    schema_root = _mutate_schema(
        tmp_path,
        "schema.yaml",
        "pattern: '^[A-Za-z_][A-Za-z0-9_]*$'",
        "pattern: '['",
    )

    with pytest.raises(SpecificationError) as caught:
        load_specification(EXAMPLES / "sdtm-dm-basic/spec.yaml", schema_root)

    diagnostic = caught.value.diagnostics[0]
    assert diagnostic.condition == "invalid_schema_bundle"
    assert "invalid pattern '['" in diagnostic.context["reason"]


@pytest.mark.parametrize(
    ("filename", "old", "new", "reason"),
    [
        (
            "schema.yaml",
            "pattern: '^[A-Za-z_][A-Za-z0-9_]*$'",
            "patern: '^[A-Za-z_][A-Za-z0-9_]*$'",
            "invalid descriptor keyword 'patern'",
        ),
        (
            "schema.yaml",
            "type: domain_name",
            'type: "list[domain_name"',
            "invalid type expression",
        ),
        (
            "schema.yaml",
            "type: domain_name",
            "type: missing_type",
            "unknown schema type 'missing_type'",
        ),
        (
            "schema_expression_mapping.yaml",
            "default: true",
            "default: invalid",
            "invalid default",
        ),
    ],
)
def test_rejects_invalid_schema_declarations(
    tmp_path: Path,
    filename: str,
    old: str,
    new: str,
    reason: str,
) -> None:
    schema_root = _mutate_schema(tmp_path, filename, old, new)

    with pytest.raises(SpecificationError) as caught:
        load_specification(EXAMPLES / "sdtm-dm-basic/spec.yaml", schema_root)

    diagnostic = caught.value.diagnostics[0]
    assert diagnostic.condition == "invalid_schema_bundle"
    assert reason in diagnostic.context["reason"]


def test_allows_registry_operations_named_type_and_registry(tmp_path: Path) -> None:
    schema_root = _copy_schema_bundle(tmp_path)
    schema_path = schema_root / "schema.yaml"
    source = schema_path.read_text(encoding="ascii")
    schema_path.write_text(
        f"{source}\n"
        "special_operation:\n"
        "    registry: special_operations\n"
        "special_operations:\n"
        "    type: {type: str}\n"
        "    registry: {type: str}\n",
        encoding="ascii",
    )

    loaded = load_specification(EXAMPLES / "sdtm-dm-basic/spec.yaml", schema_root)

    assert loaded.specification.domain == "DM"


def test_heterogeneous_duplicate_registry_keys_are_structured(
    tmp_path: Path,
) -> None:
    schema_root = _copy_schema_bundle(tmp_path)
    declaration = (
        "\nheterogeneous_registry:\n    1: {type: str}\n    repeated: {type: str}\n"
    )
    for filename in ("schema_derivation.yaml", "schema_verification.yaml"):
        schema_path = schema_root / filename
        source = schema_path.read_text(encoding="ascii")
        schema_path.write_text(f"{source}{declaration}", encoding="ascii")

    with pytest.raises(SpecificationError) as caught:
        load_specification(EXAMPLES / "sdtm-dm-basic/spec.yaml", schema_root)

    diagnostic = caught.value.diagnostics[0]
    assert diagnostic.condition == "invalid_schema_bundle"
    assert "duplicate registry entry" in diagnostic.context["reason"]


def test_rejects_mismatched_included_schema_version(tmp_path: Path) -> None:
    schema_root = _copy_schema_bundle(tmp_path)
    include_path = schema_root / "schema_shared.yaml"
    include = include_path.read_text(encoding="ascii")
    include_path.write_text(
        include.replace('version: "1.0"', 'version: "9.9"', 1),
        encoding="ascii",
    )

    with pytest.raises(SpecificationError) as caught:
        load_specification(EXAMPLES / "sdtm-dm-basic/spec.yaml", schema_root)

    diagnostic = caught.value.diagnostics[0]
    assert diagnostic.condition == "invalid_schema_bundle"
    assert diagnostic.context["path"] == str(include_path)
    assert "does not match" in diagnostic.context["reason"]


def test_yaml_12_core_scalars_remain_distinct(tmp_path: Path) -> None:
    path = tmp_path / "scalars.yaml"
    path.write_text(
        "yes_value: yes\n"
        "date_value: 2026-09-11\n"
        "bool_value: true\n"
        "int_value: 1\n"
        "float_value: 1e2\n",
        encoding="ascii",
    )

    document = read_yaml_document(path)

    assert document == {
        "yes_value": "yes",
        "date_value": "2026-09-11",
        "bool_value": True,
        "int_value": 1,
        "float_value": 100.0,
    }
    assert isinstance(document, dict)
    assert type(document["bool_value"]) is bool
    assert type(document["int_value"]) is int


@pytest.mark.parametrize(
    "content",
    [
        "value: first\nvalue: second\n",
        "value: &anchor first\n",
        "value: !!str first\n",
        "source: &source {value: first}\ncopy: *source\n",
        "source: &source {value: first}\ncopy: {<<: *source}\n",
        "? [a, b]\n: value\n",
    ],
)
def test_rejects_unsupported_yaml_features(tmp_path: Path, content: str) -> None:
    path = tmp_path / "invalid.yaml"
    path.write_text(content, encoding="ascii")

    with pytest.raises(SpecificationError) as caught:
        read_yaml_document(path)

    assert caught.value.diagnostics[0].condition == "invalid_yaml"


def test_rejects_non_ascii_authored_source(tmp_path: Path) -> None:
    path = tmp_path / "non-ascii.yaml"
    path.write_text("label: caf\N{LATIN SMALL LETTER E WITH ACUTE}\n", encoding="utf-8")

    with pytest.raises(SpecificationError) as caught:
        read_yaml_document(path)

    assert caught.value.diagnostics[0].model_dump(mode="json") == {
        "phase": "validation",
        "condition": "non_ascii_source",
        "spec_paths": ["$"],
        "context": {"path": str(path), "line": 1, "column": 11},
    }


def test_non_ascii_in_schema_include_reports_the_include_path(tmp_path: Path) -> None:
    schema_root = _copy_schema_bundle(tmp_path)
    include_path = schema_root / "schema_shared.yaml"
    source = include_path.read_text(encoding="ascii")
    include_path.write_text(
        source.replace(
            "# Types shared", "# Types shar\N{LATIN SMALL LETTER E WITH ACUTE}d"
        ),
        encoding="utf-8",
    )

    with pytest.raises(SpecificationError) as caught:
        load_specification(EXAMPLES / "sdtm-dm-basic/spec.yaml", schema_root)

    diagnostic = caught.value.diagnostics[0]
    assert diagnostic.condition == "non_ascii_source"
    assert diagnostic.context["path"] == str(include_path)


def test_rejects_surrogate_code_points_from_yaml_escapes(tmp_path: Path) -> None:
    path = tmp_path / "surrogate.yaml"
    path.write_text('label: "\\uD800"\n', encoding="ascii")

    with pytest.raises(SpecificationError) as caught:
        read_yaml_document(path)

    assert caught.value.diagnostics[0].model_dump(mode="json") == {
        "phase": "validation",
        "condition": "invalid_text",
        "spec_paths": ["$.label"],
        "context": {"code_point": "U+D800", "offset": 0},
    }


def test_rejects_schema_version_mismatch(tmp_path: Path) -> None:
    source = (EXAMPLES / "sdtm-dm-basic/spec.yaml").read_text(encoding="ascii")
    path = tmp_path / "wrong-version.yaml"
    path.write_text(source.replace('schema_version: "1.0"', 'schema_version: "9.9"'))

    with pytest.raises(SpecificationError) as caught:
        load_specification(path, SCHEMA_ROOT)

    assert caught.value.diagnostics[0].model_dump(mode="json") == {
        "phase": "validation",
        "condition": "schema_version_mismatch",
        "spec_paths": ["schema_version"],
        "context": {"expected": "1.0", "actual": "9.9"},
    }


def test_malformed_schema_version_has_a_structured_diagnostic(tmp_path: Path) -> None:
    source = (EXAMPLES / "sdtm-dm-basic/spec.yaml").read_text(encoding="ascii")
    path = tmp_path / "malformed-version.yaml"
    path.write_text(
        source.replace('schema_version: "1.0"', "schema_version: {1: value}"),
        encoding="ascii",
    )

    with pytest.raises(SpecificationError) as caught:
        load_specification(path, SCHEMA_ROOT)

    assert caught.value.diagnostics[0].model_dump(mode="json") == {
        "phase": "validation",
        "condition": "schema_version_mismatch",
        "spec_paths": ["schema_version"],
        "context": {"expected": "1.0", "actual_type": "mapping"},
    }
