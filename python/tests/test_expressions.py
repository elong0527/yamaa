"""Edge-case tests for behavior added during the 2026-09-22 parity push.

The benchmark corpus covers the golden paths; these pin the boundary
conditions the rules text calls out explicitly (locf value semantics,
ASCII-only casing, dict_yaml contract, correlated-filter qualification,
window order requirement, bare-string case results).
"""

import os

import pytest
import yaml

from yamaa import YamaaError, derive


def run(tmp_path, spec, inputs):
    d = str(tmp_path)
    os.makedirs(os.path.join(d, "input"), exist_ok=True)
    for name, rows in inputs.items():
        with open(os.path.join(d, "input", name), "w", encoding="utf-8") as f:
            f.write(rows)
    spec_path = os.path.join(d, "spec.yaml")
    with open(spec_path, "w", encoding="utf-8") as f:
        yaml.safe_dump(spec, f)
    return derive(spec_path)


def base_spec(**kw):
    spec = {
        "schema_version": "1.0",
        "domain": "ADSL",
        "keys": ["USUBJID"],
        "input": {"DM": {"path": "input/dm.csv"}},
        "output": {"path": "adsl.csv", "columns": ["USUBJID", "X"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            {"name": "X", "type": "str", "derivation": kw["derivation"]},
        ],
    }
    spec.update(kw.get("root", {}))
    return spec


DM = "USUBJID,VAL\nS1,10\nS2,0\n"


def test_locf_zero_and_empty_are_values(tmp_path):
    # REQ-1239: zero and the empty string are values, not missing.
    spec = base_spec(
        derivation={
            "locf": {
                "source": "DM.VAL",
                "window": {
                    "order_by": [{"variable": "DM.USUBJID", "direction": "asc"}]
                },
            }
        }
    )
    out = run(tmp_path, spec, {"dm.csv": DM})
    assert "S2,0" in out.splitlines()


def test_locf_current_source_wins_and_earliest_missing(tmp_path):
    # REQ-1239: the current source wins when present; the earliest row
    # with no earlier non-missing source stays missing.
    rows = "USUBJID,VISITN,AVAL\nS1,1,\nS1,2,7\nS1,3,\n"
    spec = {
        "schema_version": "1.0",
        "domain": "ADLB",
        "keys": ["USUBJID", "VISITN"],
        "input": {
            "LB": {"path": "input/lb.csv", "types": {"VISITN": "int", "AVAL": "float"}}
        },
        "output": {"path": "adlb.csv", "columns": ["USUBJID", "VISITN", "X"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "LB.USUBJID"},
            {"name": "VISITN", "type": "int", "derivation": "LB.VISITN"},
            {
                "name": "X",
                "type": "float",
                "derivation": {
                    "locf": {
                        "source": "LB.AVAL",
                        "window": {
                            "order_by": [{"variable": "LB.VISITN", "direction": "asc"}]
                        },
                    }
                },
            },
        ],
    }
    out = run(tmp_path, spec, {"lb.csv": rows})
    lines = out.splitlines()
    assert lines[1] == "S1,1,"  # earliest: nothing to carry
    assert lines[2] == "S1,2,7"  # current source wins
    assert lines[3] == "S1,3,7"  # carried from visit 2


def test_locf_requires_order_by(tmp_path):
    # REQ-0340: a locf window without order_by fails at the window path.
    spec = base_spec(derivation={"locf": {"source": "DM.VAL", "window": {}}})
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"dm.csv": DM})
    e = ei.value
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "window_order_by_required",
        "REQ-0340",
    )
    assert e.spec_paths == ["columns.X.derivation.locf.window"]


def test_window_order_by_required_applies_to_all_window_kinds(tmp_path):
    for kind, payload in [
        ("row_number", {}),
        ("rank", {}),
        ("row_value", {"source": "DM.VAL", "offset": -1}),
        ("previous_non_missing", {"source": "DM.VAL"}),
    ]:
        spec = base_spec(derivation={kind: dict(payload, window={})})
        with pytest.raises(YamaaError) as ei:
            run(tmp_path, spec, {"dm.csv": DM})
        e = ei.value
        assert e.condition == "window_order_by_required", kind
        assert e.requirement == "REQ-0340", kind


def test_str_sentence_ascii_only(tmp_path):
    # REQ-1240: first scalar up, rest down; non-ASCII passes through.
    rows = "USUBJID,VAL\nS1,hELLO w\u00d6RLD\n"
    spec = base_spec(derivation={"str_sentence": {"source": "DM.VAL"}})
    out = run(tmp_path, spec, {"dm.csv": rows})
    assert "S1,Hello w\u00d6rld" in out.splitlines()


def test_str_title_ascii_only(tmp_path):
    # REQ-1241: each [A-Za-z]+ run titled; digits/separators untouched.
    rows = "USUBJID,VAL\nS1,o'neil-2x SMITH\n"
    spec = base_spec(derivation={"str_title": {"source": "DM.VAL"}})
    out = run(tmp_path, spec, {"dm.csv": rows})
    assert "S1,O'Neil-2X Smith" in out.splitlines()


def test_str_sentence_rejects_non_str_source(tmp_path):
    # REQ-0308: static type check against the declared column type.
    spec = {
        "schema_version": "1.0",
        "domain": "ADSL",
        "keys": ["USUBJID"],
        "input": {"DM": {"path": "input/dm.csv", "types": {"SITENUM": "int"}}},
        "output": {"path": "adsl.csv", "columns": ["USUBJID", "X"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            {"name": "SITENUM", "type": "int", "derivation": "DM.SITENUM"},
            {
                "name": "X",
                "type": "str",
                "derivation": {"str_title": {"source": "SITENUM"}},
            },
        ],
    }
    rows = "USUBJID,SITENUM\nS1,3\n"
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"dm.csv": rows})
    e = ei.value
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "incompatible_input_type",
        "REQ-0308",
    )
    assert e.spec_paths == ["columns.X.derivation.str_title.source"]
    assert e.context == {"source": "SITENUM", "expected": "str", "actual": "int"}


def test_dict_yaml_missing_file(tmp_path):
    # A dict_yaml path that reaches no entry fails resource_path_missing.
    spec = base_spec(
        derivation={"mapping": {"source": "DM.VAL", "dict_yaml": "input/nope.yaml"}}
    )
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"dm.csv": DM})
    e = ei.value
    assert (e.phase, e.condition) == ("validation", "resource_path_missing")


def test_dict_yaml_exactly_one_of(tmp_path):
    # REQ-1110: dict and dict_yaml together are rejected.
    spec = base_spec(
        derivation={
            "mapping": {
                "source": "DM.VAL",
                "dict": {"10": "ten"},
                "dict_yaml": "input/d.yaml",
            }
        }
    )
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"dm.csv": DM})
    e = ei.value
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "invalid_field_type",
        "REQ-1110",
    )


def test_bare_string_case_result_is_source_read(tmp_path):
    # case_result is [str, expression]: bare then/otherwise read a source.
    rows = "USUBJID,TRT\nS1,A\nS2,B\n"
    spec = base_spec(
        derivation={
            "case": [
                {"when": "DM.TRT = 'A'", "then": "DM.TRT"},
                {"otherwise": "DM.USUBJID"},
            ]
        }
    )
    spec["columns"][1]["type"] = "str"
    out = run(tmp_path, spec, {"dm.csv": rows})
    lines = out.splitlines()
    assert lines[1] == "S1,A"
    assert lines[2] == "S2,S2"


def test_correlated_filter_unqualified_fails(tmp_path):
    # REQ-0120: every filter field is qualified.
    spec = {
        "schema_version": "1.0",
        "domain": "ADLB",
        "keys": ["USUBJID"],
        "input": {
            "PLAN": {"path": "input/plan.csv"},
            "LB": {"path": "input/lb.csv"},
        },
        "base": "PLAN",
        "intermediates": [
            {
                "id": "DONOR",
                "dataset": "LB",
                "key": ["USUBJID"],
                "filter": "LBSTRESN IS NOT NULL",
                "keep": "first",
                "order_by": [{"variable": "LB.VISITN", "direction": "asc"}],
            }
        ],
        "output": {"path": "adlb.csv", "columns": ["USUBJID", "X"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "PLAN.USUBJID"},
            {"name": "X", "type": "float", "derivation": "DONOR.LBSTRESN"},
        ],
    }
    plan = "USUBJID\nS1\n"
    lb = "USUBJID,LBSTRESN,VISITN\nS1,5,1\n"
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"plan.csv": plan, "lb.csv": lb})
    e = ei.value
    assert (e.phase, e.condition) == ("validation", "unknown_field")
    # REQ-0120: the bare donor field gets its qualified spelling suggested.
    assert e.context.get("suggestion") == "LB.LBSTRESN"


def test_correlated_filter_unknown_qualifier_fails(tmp_path):
    # REQ-0132: an unknown qualifier fails unknown_field.
    spec = {
        "schema_version": "1.0",
        "domain": "ADLB",
        "keys": ["USUBJID"],
        "input": {
            "PLAN": {"path": "input/plan.csv"},
            "LB": {"path": "input/lb.csv"},
        },
        "base": "PLAN",
        "intermediates": [
            {
                "id": "DONOR",
                "dataset": "LB",
                "key": ["USUBJID"],
                "filter": "XX.LBSTRESN IS NOT NULL",
                "keep": "first",
                "order_by": [{"variable": "LB.VISITN", "direction": "asc"}],
            }
        ],
        "output": {"path": "adlb.csv", "columns": ["USUBJID", "X"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "PLAN.USUBJID"},
            {"name": "X", "type": "float", "derivation": "DONOR.LBSTRESN"},
        ],
    }
    plan = "USUBJID\nS1\n"
    lb = "USUBJID,LBSTRESN,VISITN\nS1,5,1\n"
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"plan.csv": plan, "lb.csv": lb})
    e = ei.value
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "unknown_field",
        "REQ-0132",
    )
    assert e.context.get("suggestion") == "LB.LBSTRESN"


def _derive_intermediate_spec(intermediates, derive):
    return {
        "schema_version": "1.0",
        "domain": "ADEX",
        "keys": ["STUDYID", "USUBJID"],
        "input": {
            "EX": {
                "path": "input/ex.csv",
                "types": {"EXSEQ": "int", "EXDOSE": "float"},
            },
            "DS": {"path": "input/ds.csv", "types": {"CAPDOSE": "float"}},
        },
        "intermediates": intermediates,
        "output": {"path": "adex.csv", "columns": ["STUDYID", "USUBJID", "CUMDOSE"]},
        "columns": [
            {"name": "STUDYID", "type": "str", "derivation": "EX.STUDYID"},
            {"name": "USUBJID", "type": "str", "derivation": "EX.USUBJID"},
            {
                "name": "CUMDOSE",
                "type": "float",
                "derivation": {
                    "aggregate": {
                        "key": ["STUDYID", "USUBJID"],
                        "derive": derive,
                        "expr": "SUM(CAPPED)",
                    }
                },
            },
        ],
        "rows": [
            {
                "id": "exposure",
                "dataset": "EX",
                "filter": "EX.EXSEQ = 1",
                "derivations": {},
            },
        ],
    }


_KEEP_DERIVE = [
    {"name": "DOSE", "type": "float", "derivation": "EX.EXDOSE"},
    {"name": "CAP", "type": "float", "derivation": "DOSECAP.CAPDOSE"},
    {
        "name": "CAPPED",
        "type": "float",
        "derivation": {"least": {"sources": ["DOSE", "CAP"]}},
    },
]

_KEEP_INTERMEDIATES = [
    {
        "id": "DOSECAP",
        "dataset": "DS",
        "key": ["STUDYID", "USUBJID"],
        "order_by": ["DS.CAPDOSE"],
        "keep": "first",
    }
]

_EX_TWO_SUBJECTS = (
    "STUDYID,USUBJID,EXSEQ,EXDOSE\n"
    "PILOT7,P7-101,1,100\n"
    "PILOT7,P7-101,2,200\n"
    "PILOT7,P7-102,1,50\n"
)
_DS_TWO_SUBJECTS = (
    "STUDYID,USUBJID,CAPDOSE\nPILOT7,P7-101,500\nPILOT7,P7-101,150\nPILOT7,P7-102,30\n"
)


def test_derive_binding_reads_keep_intermediate_per_row(tmp_path):
    # REQ-1242: the intermediate resolves once per output row through the
    # row's normal lookup; P7-101 caps at 150, P7-102 at 30.
    spec = _derive_intermediate_spec(_KEEP_INTERMEDIATES, _KEEP_DERIVE)
    out = run(tmp_path, spec, {"ex.csv": _EX_TWO_SUBJECTS, "ds.csv": _DS_TWO_SUBJECTS})
    lines = out.splitlines()
    assert "PILOT7,P7-101,250" in lines
    assert "PILOT7,P7-102,30" in lines


def test_derive_rejects_intermediate_without_keep(tmp_path):
    # REQ-1242: a binding may not read an intermediate that does not
    # declare keep (no single-record-per-row promise).
    intermediates = [
        {
            "id": "DOSECAP",
            "dataset": "DS",
            "key": ["STUDYID", "USUBJID"],
        }
    ]
    spec = _derive_intermediate_spec(intermediates, _KEEP_DERIVE)
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"ex.csv": _EX_TWO_SUBJECTS, "ds.csv": _DS_TWO_SUBJECTS})
    e = ei.value
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "prohibited_construct",
        "REQ-1242",
    )
    assert e.context == {"intermediate": "DOSECAP"}


def test_derive_rejects_two_relations(tmp_path):
    # REQ-1191: a keep-declared intermediate does not excuse naming two
    # driving relations.
    derive = [
        {"name": "DOSE", "type": "float", "derivation": "EX.EXDOSE"},
        {"name": "CAP", "type": "float", "derivation": "DS.CAPDOSE"},
        {
            "name": "CAPPED",
            "type": "float",
            "derivation": {"least": {"sources": ["DOSE", "CAP"]}},
        },
    ]
    spec = _derive_intermediate_spec([], derive)
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"ex.csv": _EX_TWO_SUBJECTS, "ds.csv": _DS_TWO_SUBJECTS})
    e = ei.value
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "prohibited_construct",
        "REQ-1191",
    )


def test_derive_rejects_unknown_qualifier(tmp_path):
    # REQ-0103: a binding may not read a qualifier that names nothing.
    derive = [
        {"name": "A", "type": "float", "derivation": "XX.YY"},
        {"name": "B", "type": "float", "derivation": "A"},
    ]
    spec = _derive_intermediate_spec([], derive)
    with pytest.raises(YamaaError):
        run(tmp_path, spec, {"ex.csv": _EX_TWO_SUBJECTS, "ds.csv": _DS_TWO_SUBJECTS})


def test_derive_rejects_unknown_intermediate_field(tmp_path):
    # REQ-0125: a binding's intermediate field must be a readable column.
    derive = [
        {"name": "DOSE", "type": "float", "derivation": "EX.EXDOSE"},
        {"name": "CAP", "type": "float", "derivation": "DOSECAP.NOSUCH"},
        {
            "name": "CAPPED",
            "type": "float",
            "derivation": {"least": {"sources": ["DOSE", "CAP"]}},
        },
    ]
    spec = _derive_intermediate_spec(_KEEP_INTERMEDIATES, derive)
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"ex.csv": _EX_TWO_SUBJECTS, "ds.csv": _DS_TWO_SUBJECTS})
    e = ei.value
    assert (e.phase, e.condition) == ("validation", "unknown_field")


def test_derive_rejects_duplicate_binding_names(tmp_path):
    # REQ-1189: two bindings may not share a name; the second must not
    # silently overwrite the first.
    derive = [
        {"name": "A", "type": "float", "derivation": "EX.EXDOSE"},
        {"name": "A", "type": "float", "derivation": {"literal": 5}},
    ]
    spec = _derive_intermediate_spec([], derive)
    spec["columns"][2]["derivation"]["aggregate"]["expr"] = "SUM(A)"
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, spec, {"ex.csv": _EX_TWO_SUBJECTS, "ds.csv": _DS_TWO_SUBJECTS})
    e = ei.value
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "prohibited_construct",
        "REQ-1189",
    )
    assert e.context == {"binding": "A"}


def test_normalize_pattern_rejects_unclosed_named_group():
    # 2026-09-25: normalize_pattern("(?<abc") looped forever resetting i to
    # 0 (pattern.find(">") -> -1 -> i = 0), growing `out` until OOM. It must
    # raise instead of hanging.
    import re

    from yamaa.pred import normalize_pattern

    with pytest.raises(re.error, match="unclosed"):
        normalize_pattern("(?<abc")


def test_normalize_pattern_rejects_unclosed_unicode_escape():
    # Same runaway class: pattern.find("}") -> -1 -> i reset to 0.
    import re

    from yamaa.pred import normalize_pattern

    with pytest.raises(re.error, match="unclosed"):
        normalize_pattern("\\u{12")


def test_portable_pattern_error_flags_unclosed_named_group():
    # The validator must reject the malformed pattern with a message, not
    # hang inside normalize_pattern (this was the repository-validation OOM).
    from yamaa.pred import portable_pattern_error

    bad = portable_pattern_error("(?<abc")
    assert bad is not None and "unclosed" in bad


def test_normalize_pattern_named_group_still_works():
    # The portable (?<name>...) syntax keeps normalizing to (?P<name>...).
    from yamaa.pred import normalize_pattern

    rx = normalize_pattern("(?<yr>\\d{4})-(?<mo>\\d{2})")
    m = rx.match("2026-09")
    assert m.group("yr") == "2026" and m.group("mo") == "09"


def test_normalize_pattern_fixed_length_lookbehind_is_not_a_named_group():
    # (?<=...) and (?<!...) are lookbehind assertions, not named groups;
    # the named-group rewrite must leave them alone.
    from yamaa.pred import normalize_pattern

    rx = normalize_pattern("(?<=ab)cd")
    assert rx.pattern == "(?<=ab)cd"
    assert rx.search("abcd") and not rx.search("xcd")
    rx2 = normalize_pattern("(?<!ab)cd")
    assert rx2.pattern == "(?<!ab)cd"
    assert rx2.search("xcd") and not rx2.search("abcd")


def test_normalize_pattern_unclosed_lookbehind_raises():
    # An unclosed (?<= must raise, not loop or be treated as a named group.
    import re

    from yamaa.pred import normalize_pattern

    for bad in ("(?<=ab", "(?<!ab"):
        try:
            normalize_pattern(bad)
        except re.error:
            pass
        else:
            raise AssertionError(f"expected re.error for {bad!r}")
