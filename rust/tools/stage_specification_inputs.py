"""Stage original unchanged compiler inputs and independent report truth."""

import argparse
import csv
import json
import shutil
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[2]
CASES = ("negative-zero-division", "negative-integer-overflow", "adam-adlb-ordered-sum", "schema-window-functions", "schema-inheritance", "schema-lookup", "negative-formula-flag", "negative-row-aggregate", "negative-row-no-prior", "negative-source-missing-field", "negative-source-trivial-filter", "negative-paired-dates", "negative-not-missing-age", "negative-implausible-age", "negative-invalid-sex", "negative-sex-code", "negative-matches-bad-pattern")
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
            outcome = expected["outcome"]
            findings = []
            for finding in outcome["diagnostics"]:
                context = {}
                for item in finding["context"]:
                    reference = item["value"]
                    if reference["kind"] == "null":
                        context[item["name"]] = None
                    elif reference["kind"] in ("text", "text_list", "count"):
                        context[item["name"]] = reference["value"]
                    else:
                        raise ValueError("replay truth requires an explicit context value")
                context.update({name: outcome[name] for name in ("source", "entry", "parent") if name in outcome})
                findings.append(dict(phase="validation", condition=finding["condition"], requirement=finding["requirement"], spec_paths=[finding["path"]], context=context))
            expected["outcome"] = dict(status="invalid", diagnostics=findings)
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


def stage_decode_replay(destination: Path):
    """Add the run envelope to existing independent decoder failure truth."""
    fixture = REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/yaml_transport.tsv"
    with fixture.open(encoding="utf-8") as stream:
        records = list(csv.DictReader(stream, delimiter="\t"))
    with (destination / "decode-replay.tsv").open("w", newline="", encoding="ascii") as stream:
        writer = csv.DictWriter(stream, fieldnames=["id", "source_hex", "expected"], delimiter="\t", lineterminator="\n", quoting=csv.QUOTE_NONE, quotechar=None)
        writer.writeheader()
        for record in records:
            expected = json.loads(record["expected"])
            if expected["outcome"]["status"] != "invalid":
                continue
            expected["protocol"] = "specification/prototype"
            for finding in expected["outcome"]["diagnostics"]:
                finding.update(phase="validation", requirement=None)
                if finding["condition"] == "non_ascii_source":
                    finding["context"]["path"] = "source.yaml"
            writer.writerow(dict(record, expected=json.dumps(expected, sort_keys=True, separators=(",", ":"))))


def stage_issue_frame_truth(destination: Path):
    """Project committed report truth to literal R column expectations, never candidate output."""
    reports = {
        name: json.loads((destination / "expected" / (name + ".json")).read_text())["diagnostics"]
        for name in CASES
    }
    with (destination / "static-verification-checks.tsv").open(encoding="utf-8") as stream:
        for case in csv.DictReader(stream, delimiter="\t"):
            reports["static/" + case["case"]] = json.loads(case["expected"])

    def text(value):
        # JSON's surrogate-pair escapes are not R string escapes. Emit code points.
        quoted = json.dumps(value, ensure_ascii=False)
        return "".join(
            char if ord(char) < 128 else
            (f"\\u{ord(char):04x}" if ord(char) <= 0xffff else f"\\U{ord(char):08x}")
            for char in quoted
        )

    def strings(values):
        return "c(" + ",".join("NA_character_" if v is None else text(v) for v in values) + ")" if values else "character()"

    entries = []
    for name, diagnostics in reports.items():
        columns = []
        for field in ("phase", "condition", "requirement", "spec_paths", "context"):
            values = [row[field] for row in diagnostics]
            if field == "spec_paths":
                literal = "list(" + ",".join(strings(paths) for paths in values) + ")"
            else:
                if field == "context":
                    values = [json.dumps(v, ensure_ascii=False, sort_keys=True, separators=(",", ":")) if isinstance(v, dict) else v for v in values]
                literal = strings(values)
            columns.append(field + "=" + literal)
        entries.append(text(name) + "=list(" + ",".join(columns) + ")")
    (destination / "issue-frame-truth.R").write_text(
        "# Literal projection of committed expected diagnostics; no candidate execution.\nissue_frame_truth <- list(\n" + ",\n".join(entries) + "\n)\n",
        encoding="ascii",
    )


def stage(destination: Path):
    """Keep original files separate from independently authored expected reports."""
    destination.mkdir(parents=True, exist_ok=False)
    (destination / "schema").mkdir()
    stage_inheritance_replay(destination)
    stage_decode_replay(destination)
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/inheritance_preparation.tsv", destination / "inheritance-preparation.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/preflight.tsv", destination / "preflight.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/output_declarations.tsv", destination / "output-declarations.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/grammar_diagnostics.tsv", destination / "grammar-diagnostics.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/binding_diagnostics.tsv", destination / "binding-diagnostics.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/window_diagnostics.tsv", destination / "window-diagnostics.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/predicate_diagnostics.tsv", destination / "predicate-diagnostics.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/source_typing.tsv", destination / "source-typing.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/source_capture.tsv", destination / "source-capture.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/source_inspection.tsv", destination / "source-inspection.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/source_filters.tsv", destination / "source-filters.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/first_available.tsv", destination / "first-available.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/original_assertions.tsv", destination / "original-assertions.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/original_all_or_none.tsv", destination / "original-all-or-none.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/original_not_missing.tsv", destination / "original-not-missing.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/row_filters.tsv", destination / "row-filters.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/row_filter_admission.tsv", destination / "row-filter-admission.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/row_reductions.tsv", destination / "row-reductions.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/row_column_checks.tsv", destination / "row-column-checks.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/original_column_values.tsv", destination / "original-column-values.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/original_column_matches.tsv", destination / "original-column-matches.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/static_verification_checks.tsv", destination / "static-verification-checks.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/csv_profile_diagnostics.tsv", destination / "csv-profile-diagnostics.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/column_literals.tsv", destination / "column-literals.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/original_conversion_handlers.tsv", destination / "original-conversion-handlers.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/original_row_conversion_handlers.tsv", destination / "original-row-conversion-handlers.tsv")
    for name in SCHEMA_MODULES:
        shutil.copy2(REPOSITORY / "yaml" / name, destination / "schema" / name)
    for name in CASES:
        shutil.copytree(REPOSITORY / "benchmarks" / name, destination / "cases" / name)
    shutil.copytree(
        REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/specs",
        destination / "expected",
    )
    shutil.copytree(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/pq", destination / "pq")

    # R compares complete JSON without adding a JSON-library runtime dependency.
    for path in (destination / "expected").glob("*.json"):
        path.write_text(
            json.dumps(
                json.loads(path.read_text()), sort_keys=True, separators=(",", ":")
            ),
            encoding="utf-8",
        )
    stage_issue_frame_truth(destination)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    stage(parser.parse_args().destination)
