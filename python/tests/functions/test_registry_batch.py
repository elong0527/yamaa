"""Unlocked synthetic registry under Python: caller-supplied bindings and
batch invocation with independently authored expected results.

No packaging lock is requested anywhere in this module; registration is
separate from declared-lock auditing by design.
"""

import pytest

from yamaa import _locked_functions as m


def add(x, y):
    return x + y


def vector_add(x, y):
    return [a + (0 if b is None else b) for a, b in zip(x, y)]


def test_registry_resolves_caller_supplied_callable_without_import():
    registry = {"synthetic.add": add}
    assert m.resolve_registry_callable(registry, "synthetic.add", ["x", "y"]) is add


def test_registry_refuses_missing_binding_and_signature_mismatch():
    with pytest.raises(m.InvalidBinding):
        m.resolve_registry_callable({}, "synthetic.absent", ["x"])
    with pytest.raises(m.InvalidBinding):
        m.resolve_registry_callable({"synthetic.add": add}, "synthetic.add", ["x"])
    with pytest.raises(m.InvalidBinding):
        m.resolve_registry_callable({"synthetic.add": 3}, "synthetic.add", ["x", "y"])


def test_batch_matches_authored_scalar_results():
    # Authored truth: y=None is the accepting-missing null and counts as 0;
    # these rows are exactly the core contract's non-short-circuited rows.
    columns = {"x": [1, 3, 4, 9223372036854775806], "y": [2, 10, None, 1]}
    expected = [3, 13, 4, 9223372036854775807]
    assert m.invoke_batch_callable(vector_add, columns) == expected
    scalar = [add(x, 0 if y is None else y) for x, y in zip(columns["x"], columns["y"])]
    assert scalar == expected


def test_batch_refuses_ragged_columns_wrong_length_and_non_sequence():
    with pytest.raises(m.InvalidBinding):
        m.invoke_batch_callable(vector_add, {"x": [1], "y": [1, 2]})
    with pytest.raises(m.InvalidBinding):
        m.invoke_batch_callable(lambda x, y: [1], {"x": [1, 2], "y": [1, 2]})
    with pytest.raises(m.InvalidBinding):
        m.invoke_batch_callable(lambda x, y: "no", {"x": [1], "y": [1]})
    assert m.invoke_batch_callable(vector_add, {"x": [], "y": []}) == []


@pytest.mark.parametrize(
    "columns",
    [
        # A text column is one value per row, never one row per character:
        # "abc" used to be split into ["a", "b", "c"] and read as 3 rows.
        {"x": "abc"},
        {"x": b"abc"},
        # Non-sequence columns used to leak a raw TypeError from len().
        {"x": 1},
        {"x": None},
        {"x": [1], "y": 2},
        # A set has no row order, and a mapping is not a row sequence.
        {"x": {1, 2}},
        {"x": {"a": 1}},
    ],
)
def test_batch_refuses_non_list_columns(columns):
    with pytest.raises(m.InvalidBinding):
        m.invoke_batch_callable(lambda x, **_: list(x), columns)


def test_batch_accepts_tuple_columns():
    assert m.invoke_batch_callable(vector_add, {"x": (1, 3), "y": (2, None)}) == [
        3,
        3,
    ]
