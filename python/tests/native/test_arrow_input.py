"""In-process Arrow producers bypass source serialization in both host layouts."""

import datetime
import json

import polars as pl
import pyarrow as pa
import pytest

# The native extension is optional for the test suite (see
# test_package.py), but every test in this module exercises the compiled
# `_prepare_document` entry point, so there is no pure-Python fallback:
# skip the whole module instead of erroring on import during collection.
pytest.importorskip(
    "yamaa._native", reason="the compiled native extension is not installed"
)

from yamaa._native import _prepare_document


def prepared(kinds=None, inputs=None, base=None, expressions=None):
    kinds = kinds or {"ID": "str", "VALUE": "int"}
    document = {
        "schema_version": "1.0",
        "domain": "ARROW",
        "keys": ["ID"],
        "input": inputs or {"SRC": "unused.parquet"},
        "output": {"path": "unused.csv", "columns": list(kinds)},
        "columns": [
            {
                "name": name,
                "type": kind,
                "label": name,
                "derivation": (expressions or {}).get(name, f"SRC.{name}"),
            }
            for name, kind in kinds.items()
        ],
    }
    if base is not None:
        document["base"] = base

    def no_io(*_args):
        raise AssertionError("in-memory execution entered file IO")

    return _prepare_document(
        "memory.yaml", json.dumps(document).encode(), no_io, no_io, no_io
    )


def output(reply):
    ipc, outcome = reply
    assert json.loads(outcome)["outcome"]["status"] == "success"
    return pa.ipc.open_stream(ipc).read_all().to_pydict()


@pytest.mark.parametrize("host", ["pyarrow", "polars", "batch", "reader"])
def test_host_tables_chunks_slices_and_exact_i64(host):
    values = {"ID": ["discard", "a", "b"], "VALUE": [0, -(2**63), 2**63 - 1]}
    if host == "polars":
        source = pl.DataFrame(values).slice(1)
    else:
        source = pa.table(values).slice(1)
        if host == "batch":
            source = source.to_batches()[0]
        elif host == "reader":
            source = source.to_reader(max_chunksize=1)
    assert output(prepared().execute_arrow(source)) == {
        "ID": ["a", "b"],
        "VALUE": [-(2**63), 2**63 - 1],
    }


@pytest.mark.parametrize("kind", [pa.string(), pa.large_string(), pa.string_view()])
def test_string_layouts_keep_unicode_nul_null_and_empty_policy(kind):
    source = pa.table(
        {
            "ID": ["a", "b", "c", "d"],
            "VALUE": pa.array(
                ["\u03b2\0\U0001f980", None, "", "long text stored in a buffer"], kind
            ),
        }
    )
    result = output(prepared({"ID": "str", "VALUE": "str"}).execute_arrow(source))
    assert result["VALUE"] == [
        "\u03b2\0\U0001f980",
        None,
        None,
        "long text stored in a buffer",
    ]


def test_schema_only_empty_and_nonfinite_values():
    run = prepared({"ID": "str", "VALUE": "float"})
    source = pa.table(
        {"ID": ["a", "b", "c"], "VALUE": [-0.0, float("inf"), float("nan")]}
    )
    values = output(run.execute_arrow(source))["VALUE"]
    assert values == [-0.0, None, None]
    assert str(values[0]) == "-0.0"
    empty = pa.RecordBatchReader.from_batches(source.schema, [])
    assert output(run.execute_arrow(empty)) == {"ID": [], "VALUE": []}


def test_timestamp_units_preserve_whole_seconds_and_reject_fractional_values():
    run = prepared({"ID": "str", "VALUE": "datetime"})
    # Engine timestamps are civil values without a timezone.
    value = datetime.datetime(1969, 12, 31, 23, 59, 59)  # noqa: DTZ001
    source = pa.table({"ID": ["a"], "VALUE": pa.array([value], pa.timestamp("us"))})
    assert output(run.execute_arrow(source))["VALUE"] == [
        {"value": value, "precision": 1}
    ]
    fractional = source.set_column(1, "VALUE", pa.array([1], pa.timestamp("us")))
    with pytest.raises(ValueError, match="value"):
        run.execute_arrow(fractional)
    assert output(run.execute_arrow(source))["VALUE"][0]["value"] == value


def test_date_values_nulls_and_canonical_collected_precision():
    run = prepared({"ID": "str", "VALUE": "date"})
    dates = pa.array([datetime.date(2024, 1, 1), None], pa.date32())
    source = pa.table({"ID": ["a", "b"], "VALUE": dates})
    assert output(run.execute_arrow(source))["VALUE"] == [
        {"value": datetime.date(2024, 1, 1), "precision": 2},
        None,
    ]
    fields = [
        pa.field("value", pa.date32(), nullable=False),
        pa.field("precision", pa.uint8(), nullable=False),
    ]
    canonical = pa.StructArray.from_arrays(
        [pa.array([datetime.date(2024, 1, 1)], pa.date32()), pa.array([0], pa.uint8())],
        fields=fields,
    )
    assert output(run.execute_arrow(pa.table({"ID": ["a"], "VALUE": canonical})))[
        "VALUE"
    ] == [{"value": datetime.date(2024, 1, 1), "precision": 0}]


def test_semantic_failure_withholds_output_and_handle_remains_reusable():
    run = prepared()
    failed = pa.table({"ID": pa.array([None], pa.string()), "VALUE": [1]})
    ipc, outcome = run.execute_arrow(failed)
    assert ipc is None
    assert json.loads(outcome)["outcome"]["status"] == "failure"
    assert json.loads(outcome)["outcome"]["phase"] == "output"
    assert output(run.execute_arrow(pa.table({"ID": ["a"], "VALUE": [1]}))) == {
        "ID": ["a"],
        "VALUE": [1],
    }


def test_multi_input_order_can_put_the_driver_last():
    run = prepared(
        inputs={"OTHER": "other.parquet", "SRC": "source.parquet"},
        base="SRC",
        expressions={"VALUE": "OTHER.VALUE"},
    )
    other = pa.table({"ID": ["a", "b"], "VALUE": [7, 9]})
    source = pl.DataFrame({"ID": ["b", "a"]})
    assert output(run.execute_arrow_sources([other, source])) == {
        "ID": ["b", "a"],
        "VALUE": [9, 7],
    }


def test_count_refusal_precedes_export_and_export_errors_keep_identity():
    run = prepared()
    failure = RuntimeError("opaque producer error")

    class Refuse:
        def __arrow_c_stream__(self):
            raise failure

    with pytest.raises(ValueError, match="source_count"):
        run.execute_arrow_sources([Refuse(), Refuse()])
    with pytest.raises(RuntimeError) as actual:
        run.execute_arrow(Refuse())
    assert actual.value is failure


def test_consumed_capsule_cannot_release_or_import_twice():
    run = prepared()
    source = pa.table({"ID": ["a"], "VALUE": [1]})
    capsule = source.__arrow_c_stream__()

    class SharedCapsule:
        def __arrow_c_stream__(self):
            return capsule

    assert output(run.execute_arrow(SharedCapsule())) == {"ID": ["a"], "VALUE": [1]}
    with pytest.raises(ValueError, match="released"):
        run.execute_arrow(SharedCapsule())
    assert output(run.execute_arrow(source)) == {"ID": ["a"], "VALUE": [1]}


@pytest.mark.parametrize(
    "kind", [pa.bool_(), pa.uint64(), pa.list_(pa.int64()), pa.timestamp("s", "UTC")]
)
def test_unsupported_schema_does_not_fetch_batches(kind):
    run = prepared()

    def forbid():
        raise AssertionError("unsupported schema entered batch producer")
        yield

    reader = pa.RecordBatchReader.from_batches(
        pa.schema([("ID", pa.string()), ("VALUE", kind)]), forbid()
    )
    with pytest.raises(ValueError, match="schema"):
        run.execute_arrow(reader)


def test_exporter_must_return_a_named_capsule():
    class Invalid:
        def __arrow_c_stream__(self):
            return object()

    with pytest.raises(TypeError, match="capsule"):
        prepared().execute_arrow(Invalid())
