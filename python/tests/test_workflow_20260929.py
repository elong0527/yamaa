"""Producing-specification workflows (storage/ingestion REQ-0520 onward).

`sdtm-dm-race-ethnicity` pins a consumer reading its producer's artifact in
the same run. These pin the rest: the producer's column types reach the
consumer, nothing is published, a shared producer runs once, and a cycle,
a path the producer does not publish to, or an incomplete contract fails.
"""

import os

import pytest
import yaml

from yamaa import YamaaError, derive
from yamaa import engine as _engine

SRC = "USUBJID,AGE\nS1,30\nS2,41\n"


def write(tmp_path, files):
    for name, content in files.items():
        path = os.path.join(str(tmp_path), name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            if isinstance(content, str):
                f.write(content)
            else:
                yaml.safe_dump(content, f, sort_keys=False)


def producer(output="dm.csv", label="Age", inputs=None):
    return {
        "schema_version": "1.0",
        "domain": "DM",
        "keys": ["USUBJID"],
        "input": inputs or {"SRC": {"path": "input/src.csv", "types": {"AGE": "int"}}},
        "output": {"path": output, "columns": ["USUBJID", "AGE"]},
        "columns": [
            {
                "name": "USUBJID",
                "type": "str",
                "label": "Subject",
                "derivation": "SRC.USUBJID",
            },
            {"name": "AGE", "type": "int", "label": label, "derivation": "SRC.AGE"},
        ],
    }


def consumer(inputs):
    return {
        "schema_version": "1.0",
        "domain": "ADSL",
        "keys": ["USUBJID"],
        "input": inputs,
        "base": "DM",
        "output": {"path": "adsl.csv", "columns": ["USUBJID", "OLD"]},
        "columns": [
            {"name": "USUBJID", "type": "str", "derivation": "DM.USUBJID"},
            # A number compares with 35 only because AGE arrives typed `int`
            # from spec_dm.yaml; as text the comparison would fail.
            {
                "name": "OLD",
                "type": "str",
                "derivation": {
                    "flag": {
                        "condition": "DM.AGE > 35",
                        "false_value": "N",
                        "missing": None,
                    }
                },
            },
        ],
    }


DM = {"DM": {"path": "dm.csv", "schema": "spec_dm.yaml"}}


def run(tmp_path, files, entry="spec_adsl.yaml"):
    write(tmp_path, {"input/src.csv": SRC, **files})
    return derive(os.path.join(str(tmp_path), entry))


def failure(tmp_path, files):
    with pytest.raises(YamaaError) as ei:
        run(tmp_path, files)
    return ei.value


def test_the_consumer_reads_the_producer_artifact_under_its_types(tmp_path):
    out = run(tmp_path, {"spec_dm.yaml": producer(), "spec_adsl.yaml": consumer(DM)})
    assert out == "USUBJID,OLD\nS1,N\nS2,Y\n"
    # REQ-0786: the rendered bytes are the snapshot; nothing is published.
    assert not os.path.exists(os.path.join(str(tmp_path), "dm.csv"))


def test_a_stored_file_at_the_path_is_not_what_the_consumer_reads(tmp_path):
    # REQ-0521: the producer completes first and its artifact is the one read.
    out = run(
        tmp_path,
        {
            "dm.csv": "USUBJID,AGE\nS9,1\n",
            "spec_dm.yaml": producer(),
            "spec_adsl.yaml": consumer(DM),
        },
    )
    assert out == "USUBJID,OLD\nS1,N\nS2,Y\n"


def test_a_producer_two_datasets_read_runs_once(tmp_path, monkeypatch):
    runs = []
    real = _engine.Engine.run

    def counted(self):
        runs.append(os.path.basename(self.spec_path))
        return real(self)

    monkeypatch.setattr(_engine.Engine, "run", counted)
    spec = consumer({**DM, "DM2": {"path": "dm.csv", "schema": "spec_dm.yaml"}})
    spec["intermediates"] = [{"id": "LK", "dataset": "DM2", "key": ["USUBJID"]}]
    spec["output"]["columns"].append("AGE3")
    spec["columns"].append({"name": "AGE3", "type": "int", "derivation": "LK.AGE"})
    out = run(tmp_path, {"spec_dm.yaml": producer(), "spec_adsl.yaml": spec})
    assert out == "USUBJID,OLD,AGE3\nS1,N,30\nS2,Y,41\n"
    assert runs == ["spec_dm.yaml", "spec_adsl.yaml"]


def test_a_producer_reading_its_consumer_is_a_cycle(tmp_path):
    # REQ-0521/REQ-0534: a link cannot name a specification above it.
    looped = producer(
        inputs={
            "SRC": {"path": "input/src.csv", "types": {"AGE": "int"}},
            "BACK": {"path": "adsl.csv", "schema": "spec_adsl.yaml"},
        }
    )
    e = failure(tmp_path, {"spec_dm.yaml": looped, "spec_adsl.yaml": consumer(DM)})
    assert (e.phase, e.condition, e.requirement) == (
        "validation",
        "dependency_cycle",
        "REQ-0534",
    )
    assert e.spec_paths == ["input.BACK.schema"]


def test_the_path_must_name_where_the_producer_publishes(tmp_path):
    # REQ-1247: `path` names the producer's `output.path` location.
    e = failure(
        tmp_path,
        {"spec_dm.yaml": producer(output="out/dm.csv"), "spec_adsl.yaml": consumer(DM)},
    )
    assert (e.condition, e.requirement) == ("producer_contract_mismatch", "REQ-1247")
    assert e.spec_paths == ["input.DM.path", "input.DM.schema"]


def test_a_stored_field_needs_its_producer_label(tmp_path):
    # REQ-0522: each selected column supplies the field's type and label.
    e = failure(
        tmp_path, {"spec_dm.yaml": producer(label=""), "spec_adsl.yaml": consumer(DM)}
    )
    assert (e.condition, e.requirement) == ("producer_contract_mismatch", "REQ-0534")
    assert e.spec_paths == ["input.DM.schema.columns.AGE.label"]


def test_a_missing_producer_fails_at_schema(tmp_path):
    # REQ-0520: `schema` resolves like `path`.
    e = failure(tmp_path, {"spec_adsl.yaml": consumer(DM)})
    assert (e.condition, e.spec_paths) == ("resource_path_missing", ["input.DM.schema"])
