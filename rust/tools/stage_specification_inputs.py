"""Stage original unchanged compiler inputs and independent report truth."""

import argparse
import csv
import json
import shutil
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[2]
CASES = ("negative-zero-division", "negative-integer-overflow", "adam-adlb-ordered-sum", "schema-window-functions", "schema-inheritance", "schema-lookup")
# The current specification schema closure, not other standalone schema roots.
# Missing/new includes fail shared bundle admission; no runtime parser is used here.
SCHEMA_MODULES = (
    "schema.yaml",
    "schema_shared.yaml",
    "schema_derivation.yaml",
    "schema_verification.yaml",
    "schema_metadata.yaml",
    "schema_function.yaml",
    "schema_expression_core.yaml",
    "schema_expression_aggregate.yaml",
    "schema_expression_numeric.yaml",
    "schema_expression_str.yaml",
    "schema_expression_date.yaml",
    "schema_expression_mapping.yaml",
    "schema_expression_window.yaml",
    "schema_expression_odm.yaml",
)


def stage_inheritance_replay(destination: Path):
    """Translate existing graph inputs to raw JSON/YAML, preserving independent outcomes."""
    fixtures = REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures"
    with (fixtures / "inheritance_traversal.tsv").open(encoding="ascii") as stream:
        cases = list(csv.DictReader(stream, delimiter="\t"))
    with (fixtures / "inheritance_sources.tsv").open(encoding="ascii") as stream:
        sources = list(csv.DictReader(stream, delimiter="\t"))

    def decoded(tree):
        def node(index):
            value = tree["nodes"][index]
            if value["kind"] == "text":
                return value["value"]
            if value["kind"] == "sequence":
                return [node(i) for i in value["items"]]
            if value["kind"] == "mapping":
                return {node(k): node(v) for k, v in value["entries"]}
            raise ValueError("unexpected inheritance input scalar")
        return json.dumps(node(tree["root"]), separators=(",", ":"))

    fields = ["case", "entry", "entry_yaml", "operation", "declaring", "written", "identity", "display_path", "source_yaml", "expected"]
    with (destination / "inheritance-replay.tsv").open("w", newline="", encoding="ascii") as stream:
        writer = csv.DictWriter(stream, fieldnames=fields, delimiter="\t", lineterminator="\n", quoting=csv.QUOTE_NONE, quotechar=None)
        writer.writeheader()
        for case in cases:
            if case["case"] == "diamond_alias":
                continue  # The complete original document qualifies successful composition.
            request = json.loads(case["request"])
            expected = json.loads(case["expected"])
            expected["protocol"] = "specification/prototype"
            events = [event for event in sources if event["case"] == case["case"]]
            for event in events or [None]:
                record = dict.fromkeys(fields, "")
                record.update(case=case["case"], entry=request["entry"]["identity"], entry_yaml=decoded(request["document"]), expected=json.dumps(expected, sort_keys=True, separators=(",", ":")))
                if event is not None:
                    ask = json.loads(event["request"])
                    answer = json.loads(event["reply"])["outcome"]
                    record["operation"] = ask["operation"]
                    if ask["operation"] == "canonicalize":
                        record.update(declaring=ask["declaring"], written=ask["path"], identity=answer.get("identity", ""), display_path=answer.get("display_path", ""))
                    else:
                        record.update(identity=ask["identity"], display_path=ask["display_path"])
                        if answer["status"] == "document":
                            record["source_yaml"] = decoded(answer["document"])
                writer.writerow(record)


def stage(destination: Path):
    """Keep original files separate from independently authored expected reports."""
    destination.mkdir(parents=True, exist_ok=False)
    (destination / "schema").mkdir()
    stage_inheritance_replay(destination)
    for name in SCHEMA_MODULES:
        shutil.copy2(REPOSITORY / "yaml" / name, destination / "schema" / name)
    for name in CASES:
        shutil.copytree(REPOSITORY / "benchmarks" / name, destination / "cases" / name)
    shutil.copytree(
        REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/specifications",
        destination / "expected",
    )

    # R compares complete JSON without adding a JSON-library runtime dependency.
    for path in (destination / "expected").glob("*.json"):
        path.write_text(
            json.dumps(
                json.loads(path.read_text()), sort_keys=True, separators=(",", ":")
            ),
            encoding="utf-8",
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    stage(parser.parse_args().destination)
