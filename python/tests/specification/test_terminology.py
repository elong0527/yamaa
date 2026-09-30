from __future__ import annotations

from pathlib import Path
from typing import Any

from yamaa.specification import load_specification
from yamaa.specification.terminology import (
    load_study_document,
    validate_define_codelists,
    validate_spec_codelist_bindings,
    validate_study_terminology,
)

REPOSITORY_ROOT = Path(__file__).parents[3]
SCHEMA_ROOT = REPOSITORY_ROOT / "yaml"
DM = REPOSITORY_ROOT / "benchmarks" / "sdtm-dm-metadata"
AE = REPOSITORY_ROOT / "benchmarks" / "sdtm-ae-coding"
VS = REPOSITORY_ROOT / "benchmarks" / "sdtm-vs-epoch-from-subject-elements"
LB = REPOSITORY_ROOT / "benchmarks" / "sdtm-lb-metadata"


def _standards() -> list[dict[str, Any]]:
    return [
        {"id": "SDTMIG", "type": "IG"},
        {"id": "CT_SDTM", "type": "CT"},
    ]


def _codelist(**overrides: Any) -> dict[str, Any]:
    base: dict[str, Any] = {
        "id": "SEX",
        "name": "Sex",
        "standard": "CT_SDTM",
        "data_type": "text",
        "extensible": False,
        "items": [{"value": "F"}, {"value": "M"}],
    }
    base.update(overrides)
    return base


def _define(*codelists: dict[str, Any]) -> dict[str, Any]:
    return {"standards": _standards(), "codelists": list(codelists)}


def _first(diagnostics: Any) -> tuple[str, tuple[str, ...], str | None]:
    dumped = diagnostics[0].model_dump(mode="json")
    return (
        dumped["condition"],
        tuple(dumped["spec_paths"]),
        dumped["requirement"],
    )


def test_loads_study_documents() -> None:
    for define_path in (DM / "define.yaml", AE / "define.yaml", VS / "define.yaml"):
        study = load_study_document(define_path, SCHEMA_ROOT)
        assert study.specifications
        assert not validate_define_codelists(study.document)


def test_rejects_items_and_external_together() -> None:
    define = _define(
        _codelist(external={"dictionary": "MedDRA", "version": "26.1"}),
    )
    assert _first(validate_define_codelists(define)) == (
        "codelist_shape_invalid",
        ("codelists.SEX",),
        "REQ-0947",
    )


def test_rejects_neither_items_nor_external() -> None:
    codelist = _codelist()
    del codelist["items"]
    assert _first(validate_define_codelists(_define(codelist))) == (
        "codelist_shape_invalid",
        ("codelists.SEX",),
        "REQ-0947",
    )


def test_rejects_duplicate_codelist_id() -> None:
    other = _codelist(name="Other name")
    assert _first(validate_define_codelists(_define(_codelist(), other))) == (
        "duplicate_define_identifier",
        ("codelists.SEX",),
        "REQ-0948",
    )


def test_rejects_duplicate_codelist_name() -> None:
    other = _codelist(id="OTHER", name="Sex")
    assert _first(validate_define_codelists(_define(_codelist(), other))) == (
        "duplicate_define_identifier",
        ("codelists.OTHER",),
        "REQ-0948",
    )


def test_rejects_duplicate_coded_value() -> None:
    codelist = _codelist(items=[{"value": "F"}, {"value": "M"}, {"value": "M"}])
    assert _first(validate_define_codelists(_define(codelist))) == (
        "codelist_duplicate_value",
        ("codelists.SEX.items[2]",),
        "REQ-0949",
    )


def test_rejects_numeric_duplicate_across_int_and_float() -> None:
    codelist = _codelist(
        data_type="float",
        items=[{"value": 1}, {"value": 1.0}],
    )
    assert _first(validate_define_codelists(_define(codelist))) == (
        "codelist_duplicate_value",
        ("codelists.SEX.items[1]",),
        "REQ-0949",
    )


def test_rejects_value_outside_data_type() -> None:
    codelist = _codelist(data_type="integer", items=[{"value": "F"}])
    assert _first(validate_define_codelists(_define(codelist))) == (
        "codelist_shape_invalid",
        ("codelists.SEX.items[0]",),
        "REQ-0950",
    )


def test_rejects_partial_decode() -> None:
    codelist = _codelist(
        items=[{"value": "F", "decode": "Female"}, {"value": "M"}],
    )
    assert _first(validate_define_codelists(_define(codelist))) == (
        "codelist_partial_item_field",
        ("codelists.SEX",),
        "REQ-0951",
    )


def test_rejects_partial_rank() -> None:
    codelist = _codelist(
        items=[{"value": "F", "rank": 1}, {"value": "M"}],
    )
    assert _first(validate_define_codelists(_define(codelist))) == (
        "codelist_partial_item_field",
        ("codelists.SEX",),
        "REQ-0951",
    )


def test_rejects_extension_on_non_extensible_list() -> None:
    codelist = _codelist(items=[{"value": "X", "extended": True}])
    assert _first(validate_define_codelists(_define(codelist))) == (
        "codelist_extension_not_admitted",
        ("codelists.SEX.items[0]",),
        "REQ-0952",
    )


def test_rejects_extension_without_standard() -> None:
    codelist = _codelist(extensible=True, standard=None)
    codelist["items"] = [{"value": "X", "extended": True}]
    assert _first(validate_define_codelists(_define(codelist))) == (
        "codelist_extension_not_admitted",
        ("codelists.SEX.items[0]",),
        "REQ-0952",
    )


def test_rejects_standard_that_is_not_ct() -> None:
    codelist = _codelist(standard="SDTMIG")
    assert _first(validate_define_codelists(_define(codelist))) == (
        "codelist_shape_invalid",
        ("codelists.SEX.standard",),
        "REQ-0958",
    )


def test_rejects_unknown_codelist() -> None:
    specification = load_specification(DM / "spec.yaml", SCHEMA_ROOT).specification
    diagnostics = validate_spec_codelist_bindings(specification, _define())
    assert _first(diagnostics) == (
        "unknown_codelist",
        ("columns.DOMAIN.submission",),
        "REQ-0953",
    )


def test_rejects_row_level_unknown_codelist() -> None:
    specification = load_specification(LB / "spec.yaml", SCHEMA_ROOT).specification
    define = _define(
        _codelist(
            id="CL_LBORRESU", name="Lab result units", items=[{"value": "mg/dL"}]
        ),
    )
    diagnostics = validate_spec_codelist_bindings(specification, define)
    assert _first(diagnostics) == (
        "unknown_codelist",
        ("rows.glucose.submission.LBORRESU",),
        "REQ-0953",
    )


def test_rejects_codelist_type_mismatch() -> None:
    specification = load_specification(DM / "spec.yaml", SCHEMA_ROOT).specification
    define = _define(
        _codelist(
            id="DOMAIN", name="Domain", data_type="integer", items=[{"value": 1}]
        ),
        _codelist(id="AGEU", name="Age Unit", items=[{"value": "YEARS"}]),
        _codelist(
            id="SEX",
            name="Sex",
            items=[{"value": "F"}, {"value": "M"}, {"value": "U"}],
        ),
        _codelist(
            id="COUNTRY",
            name="Country",
            external={"dictionary": "ISO 3166", "version": "2020"},
        ),
    )
    diagnostics = validate_spec_codelist_bindings(specification, define)
    assert _first(diagnostics) == (
        "codelist_type_mismatch",
        ("columns.DOMAIN.submission",),
        "REQ-0954",
    )


def test_rejects_codelist_values_conflict() -> None:
    specification = load_specification(DM / "spec.yaml", SCHEMA_ROOT).specification
    define = _define(
        _codelist(id="DOMAIN", name="Domain", items=[{"value": "DM"}]),
        _codelist(id="AGEU", name="Age Unit", items=[{"value": "YEARS"}]),
        _codelist(id="SEX", name="Sex", items=[{"value": "F"}, {"value": "M"}]),
        _codelist(
            id="COUNTRY",
            name="Country",
            external={"dictionary": "ISO 3166", "version": "2020"},
        ),
    )
    diagnostics = validate_spec_codelist_bindings(specification, define)
    assert _first(diagnostics) == (
        "codelist_values_conflict",
        ("columns.SEX.submission",),
        "REQ-0955",
    )


def test_accepts_extensible_binding_beside_allowed_values() -> None:
    specification = load_specification(DM / "spec.yaml", SCHEMA_ROOT).specification
    define = _define(
        _codelist(id="DOMAIN", name="Domain", items=[{"value": "DM"}]),
        _codelist(id="AGEU", name="Age Unit", items=[{"value": "YEARS"}]),
        _codelist(
            id="SEX",
            name="Sex",
            extensible=True,
            items=[{"value": "F"}, {"value": "M"}],
        ),
        _codelist(
            id="COUNTRY",
            name="Country",
            external={"dictionary": "ISO 3166", "version": "2020"},
        ),
    )
    # REQ-0944: an extensible binding leaves allowed_values to stand alone,
    # so the SEX subset is not a conflict.
    assert validate_spec_codelist_bindings(specification, define) == []


def test_rejects_unreferenced_codelist() -> None:
    specification = load_specification(DM / "spec.yaml", SCHEMA_ROOT).specification
    study = load_study_document(DM / "define.yaml", SCHEMA_ROOT)
    define = dict(study.document)
    define["codelists"] = [
        *study.document.get("codelists", []),
        {"id": "UNUSED", "name": "Unused", "items": [{"value": "X"}]},
    ]
    diagnostics = validate_study_terminology(define, {"DM": specification})
    assert ("unreferenced_codelist", ("codelists.UNUSED",), "REQ-0956") in [
        _first([item]) for item in diagnostics
    ]


def test_study_composition_rejects_unknown_binding(tmp_path: Path) -> None:
    define_source = (DM / "define.yaml").read_text(encoding="ascii")
    spec_source = (DM / "spec.yaml").read_text(encoding="ascii")
    (tmp_path / "define.yaml").write_text(define_source, encoding="ascii")
    old = "      codelist: DOMAIN\n"
    assert old in spec_source
    (tmp_path / "spec.yaml").write_text(
        spec_source.replace(old, "      codelist: BOGUS\n", 1),
        encoding="ascii",
    )
    try:
        load_study_document(tmp_path / "define.yaml", SCHEMA_ROOT)
    except Exception as caught:  # noqa: BLE001 - diagnostic shape asserted below
        diagnostics = getattr(caught, "diagnostics", ())
        assert _first(diagnostics) == (
            "unknown_codelist",
            ("columns.DOMAIN.submission",),
            "REQ-0953",
        )
    else:
        raise AssertionError("expected unknown_codelist")
