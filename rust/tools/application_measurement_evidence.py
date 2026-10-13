"""Retain actual bounded owner measurements without parsing report cell maps."""
from __future__ import annotations
import gzip
import json
from pathlib import Path

PREFIX = '{"protocol":"held-observations/1","report":'
DELIMITER = ',"measurements":'


def retain_envelope(raw, report_path, measurements_path):
    if not isinstance(raw, str) or not raw.startswith(PREFIX) or not raw.endswith("}"):
        raise ValueError("invalid held observation envelope")
    split = raw.rfind(DELIMITER)
    if split < len(PREFIX):
        raise ValueError("missing original measurement evidence")
    metadata = raw[split + len(DELIMITER):-1]
    value = json.loads(metadata)
    if value.get("protocol") != "application-measurements/1":
        raise ValueError("invalid original measurement protocol")
    # Keep report cells as original encoded bytes. A bounded chunk avoids a
    # second whole report string/UTF-8 byte array during worker retention.
    for path, start, end in [(report_path, len(PREFIX), split),
                             (measurements_path, split + len(DELIMITER), len(raw)-1)]:
        opener = gzip.open if path.suffix == ".gz" else open
        options = {"compresslevel": 1} if path.suffix == ".gz" else {}
        with opener(path, "wb", **options) as target:
            for position in range(start, end, 1_048_576):
                target.write(raw[position:min(position + 1_048_576, end)].encode("utf-8"))


def uint(value, label):
    if type(value) is not int or value < 0 or value > 2**64 - 1:
        raise ValueError("invalid actual unsigned metric: " + label)
    return value


def validate(value, saves):
    if (value.get("protocol") != "application-measurements/1" or
        value.get("admitted") is not True or value.get("refusal") is not None or
        value.get("original_execution_finished") is not True or value.get("build_accepted") is not True):
        raise ValueError("original phase measurement was refused or incomplete")
    if value.get("clock") != {"provider":"std::time::Instant", "unit":"nanoseconds", "origin":"owner-local monotonic origin immediately before original application entry"}:
        raise ValueError("actual owner clock units or origin differ")
    if value.get("types") != {"offsets":"uint64 nanoseconds since owner origin", "function_counts":"uint64 actual calls", "resource_counts":"usize; required null preserves original overflow/absence"}:
        raise ValueError("actual measurement metric types differ")
    limits=value["retention_limits"]
    if limits != {"nodes":64,"identity_bytes":1_048_576,"stage_records":1024,"save_records":16}:
        raise ValueError("phase retention bounds differ")
    nodes=value["nodes"]
    indices=[]
    for node in nodes:
        index=uint(node["index"], "node")
        if index>=64 or index in indices or not isinstance(node["identity"],str):
            raise ValueError("invalid actual node identity")
        indices.append(index)
    if sum(len(node["identity"].encode("utf-8")) for node in nodes)>1_048_576:
        raise ValueError("retained actual node text exceeds bound")
    if len(value["stages"])>1024 or not value["stages"]:
        raise ValueError("actual stage ledger is absent or exceeds bound")
    durations=[]
    for stage in value["stages"]:
        if stage["node"] is not None and stage["node"] not in indices:
            raise ValueError("stage does not identify an actual node")
        if stage["phase"] not in {"preparation","build_service","activation","input","execution_admission","execution_derivation","output_keys","verification","output_preparation"}:
            raise ValueError("invalid typed actual phase")
        start=uint(stage["entered_ns"],"stage entry")
        end=uint(stage["completed_ns"],"stage exit")
        if end<start or stage["outcome"]!="succeeded":
            raise ValueError("actual stage did not complete successfully")
        durations.append({"node":stage["node"],"phase":stage["phase"],"duration_ns":end-start})
    required={"preparation","build_service","input","execution_admission","execution_derivation","output_keys","verification","output_preparation"}
    if not required.issubset({stage["phase"] for stage in value["stages"]}):
        raise ValueError("required actual application phases are missing")
    calls=value["function_calls"]
    if len(calls)>65*6:
        raise ValueError("actual function aggregates exceed bound")
    seen=set()
    for call in calls:
        identity=(call["node"],call["stage"])
        if call["stage"] not in {"activation","execution_admission","execution_derivation","output_keys","verification","execution_finished"} or identity in seen or (call["node"] is not None and call["node"] not in indices):
            raise ValueError("invalid actual function aggregate scope")
        seen.add(identity)
        entered=uint(call["entered"],"function entries")
        succeeded=uint(call["succeeded"],"function returns")
        failed=uint(call["failed"],"function failures")
        if entered!=succeeded+failed or failed:
            raise ValueError("actual function return ledger is incomplete")
    for stage in value["stages"]:
        if stage["phase"] in {"activation","execution_admission","execution_derivation","output_keys","verification"} and (stage["node"],stage["phase"]) not in seen:
            raise ValueError("actual entered function scope has no count aggregate")
    if value["classified_refusals"]:
        raise ValueError("accepted execution retains an actual resource refusal")
    if uint(value["save_attempts"],"Save attempts")!=saves or len(value["saves"])!=saves:
        raise ValueError("actual Save prefix differs from retained boundary")
    previous_exit=max(stage["completed_ns"] for stage in value["stages"])
    for number, save in enumerate(value["saves"],1):
        if uint(save["attempt"],"Save identity")!=number or save["outcome"]!="succeeded":
            raise ValueError("actual held Save did not succeed")
        start=uint(save["entered_ns"],"Save entry")
        end=uint(save["completed_ns"],"Save exit")
        if end<start or start<previous_exit: raise ValueError("actual Save monotonic offsets differ")
        previous_exit=end
    return durations


def validate_repetition(case, child, row, read_json):
    previous=None
    phases=None
    for label, saves in (("build",0),("save_1",1),("save_2",2)):
        value=read_json(child/row["measurements"][label])
        durations=validate(value,saves)
        immutable={key:value[key] for key in ("clock","nodes","stages","function_calls","classified_refusals")}
        if previous is not None and immutable!=previous:
            raise ValueError("reading held observations fabricated original events")
        previous=immutable
        phases=durations
    if case["family"]=="producer":
        actual=[Path(node["identity"]).name for node in value["nodes"]]
        if actual!=case["expected_nodes"]:
            raise ValueError("actual phase nodes differ from independently declared graph")
    elif value["nodes"]:
        raise ValueError("ordinary execution fabricated producer nodes")
    if case["family"]=="callback":
        counts={call["stage"]:call["entered"] for call in value["function_calls"] if call["node"] is None}
        if counts.get("activation")!=case["expected_callbacks"]["test_invocations"] or counts.get("execution_derivation")!=case["expected_callbacks"]["derivation_invocations"]:
            raise ValueError("actual activation/derivation calls differ from independently declared missing inputs")
    observation=row.get("callback_observation")
    if observation is not None and observation["count"]!=sum(call["entered"] for call in value["function_calls"]):
        raise ValueError("actual engine function entries differ from delegated callback trace")
    return {"stages":phases,"function_calls":value["function_calls"],"saves":value["saves"]}
