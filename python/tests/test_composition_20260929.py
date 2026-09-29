"""Specification composition (specification/composition, REQ-0614 onward).

The four `schema-*` inheritance benchmarks pin one organization, compound,
and study chain. These pin what that chain leaves open: a shared ancestor
contributing once, clears, rebased paths from other directories, whole
replacement of row fields and lookup keys, named windows, dependency
order, pruning, the traversal failures below the entry, and an output only
a grandparent writes.
"""

import os

import pytest
import yaml

from yamaa import YamaaError, derive
from yamaa.compose import resolve

DM = "USUBJID,AGE,SEX\nS1,30,F\nS2,41,M\n"


def write(tmp_path, files):
    for name, content in files.items():
        path = os.path.join(str(tmp_path), name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            if isinstance(content, str):
                f.write(content)
            else:
                yaml.safe_dump(content, f, sort_keys=False)
    return os.path.join(str(tmp_path), "spec.yaml")


def layer(**fields):
    return {"schema_version": "1.0", **fields}


def entry(parents, **fields):
    spec = {
        "schema_version": "1.0",
        "parents": parents,
        "domain": "ADSL",
        "keys": ["USUBJID"],
        "output": {"path": "adsl.csv", "columns": ["USUBJID", "AGE"]},
    }
    spec.update(fields)
    return spec


BASE = layer(
    input={"DM": {"path": "input/dm.csv", "types": {"AGE": "int"}}},
    base="DM",
    columns=[
        {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
        {"name": "AGE", "type": "int", "label": "Age", "derivation": "DM.AGE"},
    ],
)


def resolved(tmp_path, files):
    return resolve(write(tmp_path, files)).document


def failure(tmp_path, files):
    with pytest.raises(YamaaError) as ei:
        derive(write(tmp_path, files))
    return ei.value


def column(doc, name):
    return next(c for c in doc["columns"] if c["name"] == name)


def test_a_shared_ancestor_contributes_once_before_both_parents(tmp_path):
    # REQ-0621: Common -> A -> B -> entry, later contributions winning, so
    # B's label beats A's while A's metadata key survives B's.
    doc = resolved(
        tmp_path,
        {
            "input/dm.csv": DM,
            "common.yaml": BASE,
            "a.yaml": layer(
                parents="common.yaml",
                columns=[{"name": "AGE", "label": "A", "metadata": {"from": "a"}}],
            ),
            "b.yaml": layer(
                parents="common.yaml",
                columns=[{"name": "AGE", "label": "B", "metadata": {"b": "yes"}}],
            ),
            "spec.yaml": entry(["a.yaml", "b.yaml"]),
        },
    )
    assert column(doc, "AGE")["label"] == "B"
    assert column(doc, "AGE")["metadata"] == {"from": "a", "b": "yes"}
    assert "parents" not in doc


def test_a_null_clears_an_inherited_optional_field(tmp_path):
    # REQ-0632: the marker is consumed and the field is absent.
    doc = resolved(
        tmp_path,
        {
            "input/dm.csv": DM,
            "base.yaml": {**BASE, "metadata": {"scope": "base"}},
            "spec.yaml": entry(
                "base.yaml", metadata=None, columns=[{"name": "AGE", "label": None}]
            ),
        },
    )
    assert "metadata" not in doc
    assert "label" not in column(doc, "AGE")


@pytest.mark.parametrize(
    "child, where",
    [
        ({"filter": None}, "filter"),  # no layer wrote a filter
        ({"domain": None}, "domain"),  # required
        ({"columns": [{"name": "AGE", "type": None}]}, "columns.AGE.type"),
        ({"columns": [{"name": "USUBJID", "label": None}]}, "columns.USUBJID.label"),
        ({"columns": [{"name": None}]}, "columns[0].name"),  # identity
    ],
)
def test_an_invalid_clear_fails(tmp_path, child, where):
    # REQ-0632/REQ-0660: a required field, an identity field, or a field no
    # layer supplied cannot be cleared.
    e = failure(
        tmp_path,
        {
            "input/dm.csv": DM,
            "base.yaml": BASE,
            "spec.yaml": entry("base.yaml", **child),
        },
    )
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "invalid_clear",
        "REQ-0660",
    )
    assert e.spec_paths == [where]


def test_a_null_below_the_boundary_is_a_value(tmp_path):
    # REQ-0633: `{literal: null}` replaces the derivation with a missing
    # value rather than clearing it, so AGE derives blank.
    spec = write(
        tmp_path,
        {
            "input/dm.csv": DM,
            "base.yaml": BASE,
            "spec.yaml": entry(
                "base.yaml", columns=[{"name": "AGE", "derivation": {"literal": None}}]
            ),
        },
    )
    assert column(resolve(spec).document, "AGE")["derivation"] == {
        "value": {"literal": None}
    }
    assert derive(spec) == "USUBJID,AGE\nS1,\nS2,\n"


def test_paths_from_another_directory_are_rebased_to_the_entry(tmp_path):
    # REQ-0636: the input, the inherited output, and a mapping dictionary
    # file are restated from the entry, still naming the same files.
    shared = {
        **BASE,
        "domain": "ADSL",
        "keys": ["USUBJID"],
        "input": {"DM": {"path": "../input/dm.csv", "types": {"AGE": "int"}}},
        "output": {"path": "out/adsl.csv", "columns": ["USUBJID", "SEXN"]},
        "columns": [
            BASE["columns"][0],
            {
                "name": "SEXN",
                "type": "str",
                "derivation": {"mapping": {"source": "DM.SEX", "dict": "sex.yaml"}},
            },
        ],
    }
    spec = write(
        tmp_path,
        {
            "input/dm.csv": DM,
            "shared/sex.yaml": {"F": "Female", "M": "Male"},
            "shared/spec_shared.yaml": shared,
            "spec.yaml": {
                "schema_version": "1.0",
                "parents": "shared/spec_shared.yaml",
            },
        },
    )
    doc = resolve(spec).document
    assert doc["input"]["DM"]["path"] == "input/dm.csv"
    assert doc["output"]["path"] == "shared/out/adsl.csv"
    mapping = column(doc, "SEXN")["derivation"]["value"]["mapping"]
    assert mapping["dict"] == "shared/sex.yaml"
    assert derive(spec) == "USUBJID,SEXN\nS1,Female\nS2,Male\n"


def test_a_layer_outside_the_project_root_reads_the_project_data(tmp_path):
    # REQ-0772/REQ-0781: the location a layer outside the project root names
    # is not an anchor, so its spelling is read from the project root.
    study = os.path.join("study", "spec.yaml")
    files = {
        "study/input/dm.csv": DM,
        "shared/base.yaml": BASE,
        study: entry("../shared/base.yaml"),
    }
    write(tmp_path, files)
    spec = os.path.join(str(tmp_path), study)
    assert resolve(spec).document["input"]["DM"]["path"] == "../shared/input/dm.csv"
    assert derive(spec) == "USUBJID,AGE\nS1,30\nS2,41\n"


def test_a_row_member_field_and_a_lookup_key_replace_whole(tmp_path):
    # REQ-0630: only a `columns` member composes by kind. A row's
    # `derivations` and an intermediate's `key` replace whole.
    parent = {
        **BASE,
        "intermediates": [{"id": "LK", "dataset": "DM", "key": {"USUBJID": "USUBJID"}}],
        "columns": [
            *BASE["columns"],
            {"name": "SEX", "type": "str", "derivation": "LK.SEX"},
        ],
        "rows": [
            {
                "id": "subject",
                "dataset": "DM",
                "derivations": {"USUBJID": "DM.USUBJID", "AGE": "DM.AGE"},
            }
        ],
    }
    doc = resolved(
        tmp_path,
        {
            "input/dm.csv": DM,
            "base.yaml": parent,
            "spec.yaml": entry(
                "base.yaml",
                output={"path": "adsl.csv", "columns": ["USUBJID", "AGE", "SEX"]},
                intermediates=[{"id": "LK", "key": ["USUBJID"]}],
                rows=[{"id": "subject", "derivations": {"USUBJID": "DM.USUBJID"}}],
            ),
        },
    )
    assert doc["intermediates"][0]["key"] == ["USUBJID"]
    assert doc["intermediates"][0]["dataset"] == "DM"
    assert list(doc["rows"][0]["derivations"]) == ["USUBJID"]


def test_long_and_short_spellings_compose_alike(tmp_path):
    # REQ-0626: the parent's bare source and the child's long form are one
    # value, so the child adds `absent` without restating the variable.
    doc = resolved(
        tmp_path,
        {
            "input/dm.csv": DM,
            "base.yaml": BASE,
            "spec.yaml": entry(
                "base.yaml",
                columns=[
                    {"name": "AGE", "derivation": {"value": {"source": {"absent": 0}}}}
                ],
            ),
        },
    )
    assert column(doc, "AGE")["derivation"] == {
        "value": {"source": {"variable": "DM.AGE", "absent": 0}}
    }


def test_a_redefined_named_window_replaces_the_inherited_one(tmp_path):
    # REQ-1254: the later definition replaces the whole window, and the
    # inherited reference takes the final one. `windows` itself is consumed.
    parent = {
        **BASE,
        "windows": {"by_age": {"order_by": ["AGE"], "filter": "AGE > 0"}},
        "columns": [
            *BASE["columns"],
            {
                "name": "N",
                "type": "int",
                "derivation": {"row_number": {"window": "by_age"}},
            },
        ],
    }
    spec = write(
        tmp_path,
        {
            "input/dm.csv": DM,
            "base.yaml": parent,
            "spec.yaml": entry(
                "base.yaml",
                output={"path": "adsl.csv", "columns": ["USUBJID", "AGE", "N"]},
                windows={
                    "by_age": {"order_by": [{"variable": "AGE", "direction": "desc"}]}
                },
            ),
        },
    )
    doc = resolve(spec).document
    assert "windows" not in doc
    assert column(doc, "N")["derivation"]["value"]["row_number"]["window"] == {
        "order_by": [{"variable": "AGE", "direction": "desc", "nulls": "last"}]
    }
    assert derive(spec) == "USUBJID,AGE,N\nS1,30,2\nS2,41,1\n"


def test_columns_follow_their_dependencies_then_contribution_order(tmp_path):
    # REQ-0643: a parent column reading one the child declares moves after
    # it; independent columns keep first-contribution order.
    doc = resolved(
        tmp_path,
        {
            "input/dm.csv": DM,
            "base.yaml": {
                **BASE,
                "columns": [
                    {"name": "AGEGR", "type": "str", "derivation": "AGEC"},
                    *BASE["columns"],
                ],
            },
            "spec.yaml": entry(
                "base.yaml",
                output={"path": "adsl.csv", "columns": ["USUBJID", "AGE", "AGEGR"]},
                columns=[{"name": "AGEC", "type": "str", "derivation": "AGE"}],
            ),
        },
    )
    assert [c["name"] for c in doc["columns"]] == ["USUBJID", "AGE", "AGEC", "AGEGR"]


def test_a_dead_declaration_is_pruned_with_its_unresolved_reference(tmp_path):
    # REQ-0641: an unknown reference only a dead declaration makes is
    # discarded with it rather than failing, and so is the input only a
    # dead intermediate reads.
    spec = write(
        tmp_path,
        {
            "input/dm.csv": DM,
            "base.yaml": {
                **BASE,
                "input": {**BASE["input"], "AE": "input/missing.csv"},
                "intermediates": [{"id": "AEX", "dataset": "AE", "key": ["USUBJID"]}],
                "columns": [
                    *BASE["columns"],
                    {"name": "DEAD", "type": "str", "derivation": "NO_SUCH.COLUMN"},
                    {"name": "AEX1", "type": "str", "derivation": "AEX.AETERM"},
                ],
            },
            "spec.yaml": entry("base.yaml"),
        },
    )
    doc = resolve(spec).document
    assert list(doc["input"]) == ["DM"]
    assert "intermediates" not in doc
    assert [c["name"] for c in doc["columns"]] == ["USUBJID", "AGE"]
    assert derive(spec) == "USUBJID,AGE\nS1,30\nS2,41\n"


def test_an_output_only_a_grandparent_writes_is_inherited(tmp_path):
    # REQ-0623/REQ-0657: the entry and its parent omit `output`; the
    # grandparent's is the resolved one.
    spec = write(
        tmp_path,
        {
            "input/dm.csv": DM,
            "org.yaml": {
                **BASE,
                "domain": "ADSL",
                "keys": ["USUBJID"],
                "output": {"path": "adsl.csv", "columns": ["USUBJID", "AGE"]},
            },
            "compound.yaml": layer(parents="org.yaml", metadata={"scope": "compound"}),
            "spec.yaml": layer(parents="compound.yaml"),
        },
    )
    assert derive(spec) == "USUBJID,AGE\nS1,30\nS2,41\n"


def test_no_layer_writing_output_fails_on_the_resolved_spec(tmp_path):
    # REQ-0657: missing_required_field at `output`, no requirement attached.
    e = failure(
        tmp_path,
        {
            "input/dm.csv": DM,
            "base.yaml": {**BASE, "domain": "ADSL", "keys": ["USUBJID"]},
            "spec.yaml": layer(parents="base.yaml"),
        },
    )
    assert (e.condition, e.requirement, e.spec_paths) == (
        "missing_required_field",
        None,
        ["output"],
    )


@pytest.mark.parametrize(
    "grandparent, condition, requirement",
    [
        ("https://example.test/org.yaml", "invalid_parent_path", "REQ-0653"),
        ("missing.yaml", "parent_not_found", "REQ-0654"),
        ("spec.yaml", "inheritance_cycle", "REQ-0655"),
    ],
)
def test_a_parent_below_the_entry_fails_its_traversal(
    tmp_path, grandparent, condition, requirement
):
    # REQ-0653 to REQ-0655 hold at every depth, not only for the entry's
    # own parents.
    e = failure(
        tmp_path,
        {
            "input/dm.csv": DM,
            "base.yaml": {**BASE, "parents": grandparent},
            "spec.yaml": entry("base.yaml"),
        },
    )
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        condition,
        requirement,
    )


def test_a_grandparent_of_another_version_fails(tmp_path):
    # REQ-0622/REQ-0656: every layer declares the bundle version.
    e = failure(
        tmp_path,
        {
            "input/dm.csv": DM,
            "org.yaml": {"schema_version": "2.0", "metadata": {"scope": "org"}},
            "base.yaml": {**BASE, "parents": "org.yaml"},
            "spec.yaml": entry("base.yaml"),
        },
    )
    assert (e.condition, e.requirement) == ("schema_version_mismatch", "REQ-0656")


def test_one_layer_declaring_an_identifier_twice_fails(tmp_path):
    # REQ-0624/REQ-0659: identity is per layer before composition merges.
    e = failure(
        tmp_path,
        {
            "input/dm.csv": DM,
            "base.yaml": {**BASE, "columns": [*BASE["columns"], BASE["columns"][1]]},
            "spec.yaml": entry("base.yaml"),
        },
    )
    assert (e.condition, e.requirement) == ("duplicate_identifier", "REQ-0659")


def test_a_handler_free_value_wrapper_around_a_window_derives_it(tmp_path):
    # REQ-0266: the resolved form writes every derivation under `value`; a
    # window keyword there still ranks the whole partition.
    spec = write(
        tmp_path,
        {
            "input/dm.csv": DM,
            "spec.yaml": {
                "schema_version": "1.0",
                "domain": "ADSL",
                "keys": ["USUBJID"],
                "input": BASE["input"],
                "base": "DM",
                "output": {"path": "adsl.csv", "columns": ["USUBJID", "N"]},
                "columns": [
                    *BASE["columns"],
                    {
                        "name": "N",
                        "type": "int",
                        "derivation": {
                            "value": {"row_number": {"window": {"order_by": ["AGE"]}}}
                        },
                    },
                ],
            },
        },
    )
    assert derive(spec) == "USUBJID,N\nS1,1\nS2,2\n"
