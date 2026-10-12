"""Stage original unchanged compiler inputs and independent report truth."""

import argparse
import csv
import json
import datetime
import shutil
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[2]
CASES = ("negative-zero-division", "negative-integer-overflow", "adam-adlb-ordered-sum", "schema-window-functions", "schema-inheritance", "schema-lookup", "negative-formula-flag", "negative-row-aggregate", "negative-row-no-prior", "negative-source-missing-field", "negative-source-trivial-filter", "negative-paired-dates", "negative-not-missing-age", "negative-implausible-age", "negative-invalid-sex", "negative-sex-code", "negative-matches-bad-pattern")
ROUNDING_CASES = ("schema-round-half-away-from-zero-ties", "negative-round-half-away-from-zero-non-numeric-source")
TEMPORAL_CASES = ("adam-adsl-duration", "adam-adsl-analysis-age", "negative-date-diff-endpoints", "negative-date-diff-units", "negative-study-day-datetime", "negative-epoch-day-non-date")
IMPUTATION_CASES = ("schema-birth-date", "sdtm-dm-age", "negative-impute-bad-month", "negative-impute-unreachable-month", "negative-impute-missing-month", "negative-impute-day-rule", "negative-impute-bad-day", "negative-date-conversion", "negative-impute-bad-source", "negative-precision-bad-source", "negative-datetime-impute-bad-source", "negative-datetime-precision-bad-source")
TEXT_CASES = ("schema-sentence-case", "negative-str-case-number", "negative-str-case-unknown")
PAD_CASES = ("negative-str-pad-width",)
FLAG_POSITIVE_CASES = ("schema-str-contains", "adam-adae-occurrence-flags", "adam-adex-dose-reduction", "adam-adlb-closest-visit", "adam-adsl-completion-flag", "sdtm-dm-death")
FLAG_NEGATIVE_CASES = ("negative-flag-condition", "negative-flag-missing-value", "negative-flag-payload", "negative-review-arithmetic", "negative-review-date-text", "negative-review-unknown-date")
FLAG_HELD_CASES = ("nested_case_mixed_declarations", "implicit_raw_ambiguous")
EXTRACTION_CASES = ("adam-adae-text-cleanup", "negative-str-uncaptured-group")
TEMPLATE_CASES = ("adam-adsl-site-parse", "negative-subject-reference")
# Archive aliases shorten installed paths without changing original fixture identities or bytes.
CASE_DIRECTORY_NAMES = {"negative-round-half-away-from-zero-non-numeric-source": "negative-rounding-source"}
# Only archive directories are shortened; independent capture labels remain exact.
TEMPLATE_DIRECTORY_NAMES = {
    "type-before-later-invalid-declaration": "type-before-invalid",
    "unknown-before-later-invalid-declaration": "unknown-before-invalid",
}
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


def stage_text_extract_truth(destination: Path):
    """Stage originals and separately captured complete reference extraction cases."""
    source = REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/text_extract_reference.json"
    shutil.copy2(source, destination / "text-extract-reference.json")
    capture = json.loads(source.read_text())
    with (destination / "text-extract-handlers.tsv").open("w", newline="", encoding="ascii") as stream:
        writer = csv.writer(stream, delimiter="\t", lineterminator="\n", quoting=csv.QUOTE_NONE, quotechar=None)
        writer.writerow(("case", "handlers"))
        for name, value in capture.items():
            if "handler_counts" in value:
                writer.writerow((name, json.dumps(value["handler_counts"], sort_keys=True, separators=(",", ":"))))
    root = destination / "text-extract-cases"
    root.mkdir()
    with (destination / "text-extract-diagnostics.tsv").open("w", newline="", encoding="ascii") as stream:
        writer = csv.writer(stream, delimiter="\t", lineterminator="\n", quoting=csv.QUOTE_NONE, quotechar=None)
        writer.writerow(("case", "phase", "condition", "requirement", "spec_path", "context"))
        for name, value in capture.items():
            if "specification" not in value or name == "wide-group":
                continue
            case = root / name
            case.mkdir()
            (case / "spec.yaml").write_text(json.dumps(value["specification"], ensure_ascii=True) + "\n", encoding="ascii", newline="\n")
            (case / "input.csv").write_text(value["input"], encoding="utf-8", newline="\n")
            if "csv" in value:
                (case / "expected.csv").write_text(value["csv"], encoding="utf-8", newline="\n")
            for finding in value.get("findings", []):
                writer.writerow((name, finding["phase"], finding["condition"], finding["requirement"], finding["spec_paths"][0], json.dumps(finding["context"], sort_keys=True, separators=(",", ":"))))


def stage_text_template_truth(destination: Path):
    """Stage originals and separately captured complete reference template cases."""
    source = REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/text_template_reference.json"
    shutil.copy2(source, destination / "text-template-reference.json")
    capture = json.loads(source.read_text())
    with (destination / "text-template-handlers.tsv").open("w", newline="", encoding="ascii") as stream:
        writer = csv.writer(stream, delimiter="\t", lineterminator="\n", quoting=csv.QUOTE_NONE, quotechar=None)
        writer.writerow(("case", "handlers"))
        for name, value in capture.items():
            if "handler_counts" in value:
                writer.writerow((name, json.dumps(value["handler_counts"], sort_keys=True, separators=(",", ":"))))
    with (destination / "text-template-directories.tsv").open("w", newline="", encoding="ascii") as stream:
        writer = csv.writer(stream, delimiter="\t", lineterminator="\n", quoting=csv.QUOTE_NONE, quotechar=None)
        writer.writerow(("case", "directory"))
        for name, value in capture.items():
            if "specification" in value:
                writer.writerow((name, TEMPLATE_DIRECTORY_NAMES.get(name, name)))
    root = destination / "text-template-cases"
    root.mkdir()
    with (destination / "text-template-diagnostics.tsv").open("w", newline="", encoding="ascii") as stream:
        writer = csv.writer(stream, delimiter="\t", lineterminator="\n", quoting=csv.QUOTE_NONE, quotechar=None)
        writer.writerow(("case", "phase", "condition", "requirement", "spec_path", "context"))
        for name, value in capture.items():
            if "specification" not in value:
                continue
            case = root / TEMPLATE_DIRECTORY_NAMES.get(name, name)
            case.mkdir()
            (case / "spec.yaml").write_text(json.dumps(value["specification"], ensure_ascii=True) + "\n", encoding="ascii", newline="\n")
            (case / "input.csv").write_text(value["input"], encoding="utf-8", newline="\n")
            if "csv" in value:
                (case / "expected.csv").write_text(value["csv"], encoding="utf-8", newline="\n")
            for finding in value.get("findings", []):
                writer.writerow((name, finding["phase"], finding["condition"], finding["requirement"], finding["spec_paths"][0], json.dumps(finding["context"], sort_keys=True, separators=(",", ":"))))


def stage_temporal_imputation_truth(destination: Path):
    """Retain independently stated original failures and complete reference captures."""
    cases = [
        ("negative-impute-bad-month", "validation", "month_out_of_range", "REQ-0608", "columns.ASTDT.derivation.date_impute.month", {"month": 15}),
        ("negative-impute-unreachable-month", "validation", "month_not_permitted", "REQ-0592", "columns.ASTDT.derivation.date_impute.month", {"month": 6}),
        ("negative-impute-missing-month", "validation", "month_required", "REQ-0592", "columns.ASTDT.derivation.date_impute.month", {"minimum_source_precision": "year"}),
        ("negative-impute-day-rule", "validation", "value_not_permitted", "REQ-0609", "columns.ASTDT.derivation.date_impute.day", {"value": "mid", "permitted": ["first", "last"]}),
        ("negative-impute-bad-day", "impute", "invalid_calendar_date", "REQ-0608", "columns.AENDT.derivation.date_impute", {"value": "2023-04", "completed": "2023-04-31", "keys": [{"STUDYID": "YAMAA-01", "USUBJID": "YAMAA-01-101", "AESEQ": 2}]}),
        ("negative-date-conversion", "validation", "incompatible_input_type", "REQ-0607", "columns.ASTDT2.derivation.to_date.source", {"source": "ASTDT", "expected": "datetime, ISO date text, or ISO 8601 datetime text", "actual": "date"}),
    ]
    capture = json.loads((REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/temporal_imputation_reference.json").read_text())
    for name, findings in capture.items():
        assert len(findings) == 1
        finding = findings[0]
        assert len(finding["spec_paths"]) == 1
        cases.append((name, finding["phase"], finding["condition"], finding["requirement"], finding["spec_paths"][0], finding["context"]))
    assert {case[0] for case in cases} == set(IMPUTATION_CASES) - {"schema-birth-date", "sdtm-dm-age"}
    with (destination / "temporal-imputation-diagnostics.tsv").open("w", newline="", encoding="ascii") as stream:
        writer = csv.writer(stream, delimiter="\t", lineterminator="\n", quoting=csv.QUOTE_NONE, quotechar=None)
        writer.writerow(("case", "phase", "condition", "requirement", "spec_path", "context"))
        for name, phase, condition, requirement, path, context in cases:
            writer.writerow((name, phase, condition, requirement, path, json.dumps(context, sort_keys=True, separators=(",", ":"))))



def stage_text_case_truth(destination: Path):
    """Keep complete legacy captures and stage their shared normative facts."""
    original = REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/text_case_reference.json"
    shutil.copy2(original, destination / "text-case-reference.json")
    capture = json.loads(original.read_text())
    with (destination / "text-case-diagnostics.tsv").open("w", newline="", encoding="ascii") as stream:
        writer = csv.writer(stream, delimiter="\t", lineterminator="\n", quoting=csv.QUOTE_NONE, quotechar=None)
        writer.writerow(("case", "phase", "condition", "requirement", "spec_path", "context"))
        for name in TEXT_CASES[1:]:
            finding, = capture[name]
            context = dict(finding["context"])
            if name == "negative-str-case-unknown":
                # Loader source/entry provenance is retained above, separately
                # from native schema diagnostics' normative context.
                source, entry = context.pop("source"), context.pop("entry")
                assert source == entry == "/private/tmp/yamaa-root-text-operators/benchmarks/negative-str-case-unknown/spec.yaml"
            writer.writerow((name, finding["phase"], finding["condition"], finding["requirement"], finding["spec_paths"][0], json.dumps(context, sort_keys=True, separators=(",", ":"))))


def stage_text_pad_truth(destination: Path):
    """Stage untouched independent captures and complete shared normative findings."""
    original = REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/text_pad_reference.json"
    shutil.copy2(original, destination / "text-pad-reference.json")
    capture = json.loads(original.read_text(encoding="utf-8"))
    root = destination / "text-pad-cases"
    root.mkdir()
    with (destination / "text-pad-findings.tsv").open("w", newline="", encoding="ascii") as stream:
        writer = csv.writer(stream, delimiter="\t", lineterminator="\n", quoting=csv.QUOTE_NONE, quotechar=None)
        writer.writerow(("case", "index", "phase", "condition", "requirement", "spec_paths", "context"))
        for name, case in capture.items():
            directory = root / name
            directory.mkdir()
            (directory / "spec.yaml").write_text(case["specification"], encoding="utf-8", newline="\n")
            (directory / "input.csv").write_text(case["input"], encoding="utf-8", newline="\n")
            if "csv" in case:
                (directory / "expected.csv").write_text(case["csv"], encoding="utf-8", newline="\n")
            for index, finding in enumerate(case.get("findings", [])):
                context = dict(finding["context"])
                if name == "width-noninteger":
                    # This existing native schema provenance gap remains explicit;
                    # the complete unmodified loader evidence is copied above.
                    for field in ("source", "entry"):
                        assert context.pop(field) == "/private/tmp/yamaa-root-text-pad-audit/width-noninteger/spec.yaml"
                writer.writerow((name, index, finding["phase"], finding["condition"], finding["requirement"], json.dumps(finding["spec_paths"], separators=(",", ":")), json.dumps(context, sort_keys=True, separators=(",", ":"))))


def stage_flag_truth(destination: Path):
    """Retain complete independent captures and emit literal installed R truth."""
    original = REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/flag_reference.json"
    shutil.copy2(original, destination / "flag-reference.json")
    capture = json.loads(original.read_text(encoding="utf-8"))
    authored = destination / "flags"
    authored.mkdir()
    for name, case in capture["witnesses"].items():
        directory = authored / name
        directory.mkdir()
        (directory / "spec.yaml").write_text(case["specification"], encoding="utf-8", newline="\n")
        for filename, text in case.get("inputs", {}).items():
            (directory / filename).write_text(text, encoding="utf-8", newline="\n")
        if "csv" in case:
            (directory / "expected.csv").write_text(case["csv"], encoding="utf-8", newline="\n")

    def text(value):
        quoted = json.dumps(value, ensure_ascii=False)
        return "".join(char if ord(char) < 128 else
            (f"\\u{ord(char):04x}" if ord(char) <= 0xffff else f"\\U{ord(char):08x}") for char in quoted)

    def strings(values):
        return "c(" + ",".join("NA_character_" if v is None else text(v) for v in values) + ")" if values else "character()"

    outputs = dict(zip(FLAG_POSITIVE_CASES, ("adae.csv", "adae.csv", "adex.csv", "adlb.csv", "adsl.csv", "dm.csv")))
    entries = []
    cohorts = [(name, capture["originals"][name], "cases") for name in FLAG_POSITIVE_CASES + FLAG_NEGATIVE_CASES]
    cohorts += [(name, case, "flags") for name, case in capture["witnesses"].items() if name not in FLAG_HELD_CASES]
    assert len(cohorts) == 33
    for name, case, directory in cohorts:
        findings = json.loads(json.dumps(case["findings"]))
        if case["result_type"] == "SpecificationError":
            # Existing loader source/entry fields are retained unmodified in
            # flag-reference.json; this suite states the native provenance gap.
            for finding in findings:
                context = finding["context"]
                source, entry = context.pop("source"), context.pop("entry")
                assert source == entry and source.endswith("/" + name + "/spec.yaml")
        columns = []
        for field in ("phase", "condition", "requirement", "spec_paths", "context"):
            values = [row[field] for row in findings]
            if field == "spec_paths":
                literal = "list(" + ",".join(strings(paths) for paths in values) + ")"
            else:
                if field == "context":
                    values = [json.dumps(v, sort_keys=True, separators=(",", ":")) for v in values]
                literal = strings(values)
            columns.append(field + "=" + literal)
        expected_csv = ""
        if case["result_type"] == "ExecutionSuccess":
            if directory == "cases":
                expected, = (REPOSITORY / "benchmarks" / name / "expected").glob("*.csv")
                # Committed CSV fixes lexical serialization; the retained legacy
                # dataframe capture may use decimal/datetime display padding.
                expected_csv = "expected/" + expected.name
            else:
                expected_csv = "expected.csv"
        values = ["directory=" + text(directory), "output=" + text(outputs.get(name, "out.csv")),
            "expected_csv=" + text(expected_csv), "specification=" + text(case["specification"]),
            "schema_failure=" + ("TRUE" if case["result_type"] == "SpecificationError" else "FALSE"),
            "handlers=" + text(json.dumps(case.get("handlers", []), sort_keys=True, separators=(",", ":"))),
            "findings=list(" + ",".join(columns) + ")"]
        entries.append(text(name) + "=list(" + ",".join(values) + ")")
    (destination / "flag-truth.R").write_text(
        "# Independent original CSV/findings/handlers; no candidate execution.\n"
        "# Held: generic case REQ-0103/0189 and recursive report context PR1933.\n"
        "flag_truth <- list(\n" + ",\n".join(entries) + "\n)\n", encoding="ascii")

def stage_intermediate_key_truth(destination: Path):
    """Stage all unchanged scalar-key captures without evaluating the candidate."""
    original = REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/intermediate_key_reference.json"
    shutil.copy2(original, destination / "key-reference.json")
    capture = json.loads(original.read_text(encoding="utf-8"))
    assert len(capture) == 19
    root = destination / "keys"
    root.mkdir()

    def text(value):
        return json.dumps(value, ensure_ascii=True)

    def strings(values):
        return "c(" + ",".join("NA_character_" if value is None else text(value) for value in values) + ")" if values else "character()"

    entries = []
    for name, case in capture.items():
        directory = root / name
        directory.mkdir()
        (directory / "spec.yaml").write_text(case["specification"], encoding="utf-8", newline="\n")
        for filename, source in case["inputs"].items():
            (directory / filename).write_text(source, encoding="utf-8", newline="\n")
        if "csv" in case:
            (directory / "expected.csv").write_text(case["csv"], encoding="utf-8", newline="\n")
        columns = []
        for field in ("phase", "condition", "requirement", "spec_paths", "context"):
            values = [finding[field] for finding in case.get("findings", [])]
            if field == "spec_paths":
                literal = "list(" + ",".join(strings(paths) for paths in values) + ")"
            else:
                if field == "context":
                    values = [json.dumps(value, sort_keys=True, separators=(",", ":")) for value in values]
                literal = strings(values)
            columns.append(field + "=" + literal)
        entries.append(text(name) + "=list(issues=list(" + ",".join(columns) + "),success=" +
            ("TRUE" if "csv" in case else "FALSE") + ",handlers=" +
            text(json.dumps(case["handlers"], sort_keys=True, separators=(",", ":"))) + ")")
    (destination / "key-truth.R").write_text(
        "# All 19 unchanged independent scalar-key findings, CSV and handlers.\n"
        "key_truth <- list(\n" + ",\n".join(entries) + "\n)\n", encoding="ascii")


def stage_row_intermediate_truth(destination: Path):
    """Materialize unchanged originals and complete independently captured row truth."""
    original = REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/row_intermediate_reference.json"
    shutil.copy2(original, destination / "row-intermediate-reference.json")
    reference = json.loads(original.read_text(encoding="ascii"))
    root = destination / "row-cases"
    root.mkdir()
    cases = dict(reference["cases"], **reference["originals"])
    def text(value):
        return json.dumps(value, ensure_ascii=True)
    def strings(values):
        return "c(" + ",".join("NA_character_" if value is None else text(value) for value in values) + ")" if values else "character()"
    entries = []
    for name, case in cases.items():
        directory = root / name
        directory.mkdir()
        if name in reference["originals"]:
            # The full copied original keeps its source/expected fixtures unchanged.
            shutil.copytree(REPOSITORY / "benchmarks" / name, directory, dirs_exist_ok=True)
            assert (directory / "spec.yaml").read_bytes() == case["specification"].encode()
            for filename, source in case["inputs"].items():
                assert (directory / "input" / filename).read_bytes() == source.encode()
        else:
            (directory / "spec.yaml").write_text(case["specification"], encoding="utf-8", newline="\n")
            for filename, source in case["inputs"].items():
                (directory / filename).write_text(source, encoding="utf-8", newline="\n")
        findings = case["findings"]
        success = "csv" in case
        if name == "adam-adlb-end-of-treatment":
            # Keep the historical capture; only intermediate derivations and
            # verifications remain unsupported after same-input admission.
            findings = list(reference["eot_native_gates"])
            assert len(findings) == 5
            assert findings.pop() == {"phase":"planning", "condition":"unsupported_operation", "requirement":None,
                                      "spec_paths":["columns.EOTFL.derivation.flag"], "context":{"operation":"flag"}}
            assert findings.pop() == {"phase":"planning", "condition":"unsupported_operation", "requirement":None,
                                      "spec_paths":["columns.ENDPOINT.derivation.source"], "context":{"operation":"secondary_source_expression"}}
            assert findings.pop() == {"phase":"planning", "condition":"unsupported_operation", "requirement":None,
                                      "spec_paths":["intermediates[1]"], "context":{"operation":"intermediate_scope"}}
            success = False
        if success:
            (directory / "expected.csv").write_text(case["csv"], encoding="utf-8", newline="\n")
            if name in reference["originals"]:
                assert case["csv"] == case["expected_csv"]
        (directory / "findings.json").write_text(json.dumps(findings, ensure_ascii=True, sort_keys=True) + "\n", encoding="ascii")
        fields = []
        for field in ("phase", "condition", "requirement", "spec_paths", "context"):
            values = [finding[field] for finding in findings]
            if field == "spec_paths":
                literal = "list(" + ",".join(strings(paths) for paths in values) + ")"
            else:
                if field == "context":
                    values = [json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":")) for value in values]
                literal = strings(values)
            fields.append(field + "=" + literal)
        ledger = case.get("handler_counts", [])
        output = "adlb.csv" if name in reference["originals"] else "result.csv"
        entries.append(text(name) + "=list(issues=list(" + ",".join(fields) + "),success=" + ("TRUE" if success else "FALSE") + ",output=" + text(output) + ",ledger=" + text(json.dumps(ledger, sort_keys=True, separators=(",", ":"))) + ")")
    (destination / "row-intermediate-truth.R").write_text("# Complete independent row-context truth; EOT derivations and verifications remain gated.\nrow_intermediate_truth <- list(\n" + ",\n".join(entries) + "\n)\n", encoding="ascii")

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


def stage_public_output_truth(destination: Path):
    """Literal typed R expectations from committed original report observations."""
    def text(value):
        quoted = json.dumps(value, ensure_ascii=False)
        return "".join(char if ord(char) < 128 else
                       (f"\\u{ord(char):04x}" if ord(char) <= 0xffff else f"\\U{ord(char):08x}")
                       for char in quoted)
    entries = []
    for name in CASES:
        report = json.loads((destination / "expected" / (name + ".json")).read_text())
        if not report["artifacts"]:
            entries.append(text(name) + "=NULL")
            continue
        artifact = report["artifacts"][0]
        table = next(table for table in report["tables"] if table["stage"] == "derived")
        columns = []
        for column_name, kind in zip(artifact["columns"], artifact["types"]):
            position = table["columns"].index(column_name)
            cells = []
            for row in table["rows"]:
                scalar = row[position]
                if scalar["type"] == "missing":
                    cells.append("NA_character_" if kind in ("str", "int") else "NA_real_")
                elif kind in ("str", "int"):
                    cells.append(text(scalar["value"]))
                elif kind == "float":
                    raw = bytes.fromhex(scalar["value"])[::-1]
                    cells.append("readBin(as.raw(c(" + ",".join(str(byte) for byte in raw) + ")), 'double', n=1L, size=8L, endian='little')")
                elif kind == "date":
                    cells.append(str((datetime.date.fromisoformat(scalar["value"]) - datetime.date(1970, 1, 1)).days))
                elif kind == "datetime":
                    cells.append(str(int((datetime.datetime.fromisoformat(scalar["value"]) - datetime.datetime(1970, 1, 1)).total_seconds())))
                else:
                    raise ValueError("unclassified public output type")
            literal = "c(" + ",".join(cells) + ")" if cells else ("character()" if kind in ("str", "int") else "double()")
            if kind == "int":
                literal = "structure(" + literal + ",class='yamaa_int64_vector')"
            elif kind == "date":
                literal = "structure(as.double(" + literal + "),class='Date')"
            elif kind == "datetime":
                literal = "structure(as.double(" + literal + "),class=c('POSIXct','POSIXt'),tzone='UTC')"
            columns.append(text(column_name) + "=" + literal)
        entries.append(text(name) + "=list(" + ",".join(columns) + ")")
    (destination / "public-output-truth.R").write_text(
        "# Literal columns from committed reference report truth.\npublic_output_truth <- list(\n" + ",\n".join(entries) + "\n)\n", encoding="ascii")


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
    source_truth = REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/original_source_bindings.tsv"
    shutil.copy2(source_truth, destination / "original-source-bindings.tsv")
    source_cases = destination / "source-binding-cases"
    source_cases.mkdir()
    with source_truth.open(encoding="ascii") as stream:
        for row in csv.DictReader(stream, delimiter="\t"):
            case = source_cases / row["case"]
            case.mkdir()
            for name, content in json.loads(row["inputs_json"]).items():
                (case / (name.lower() + ".csv")).write_bytes(bytes.fromhex(content))
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/mapping_cut_admission.tsv", destination / "mapping-cut-admission.tsv")
    source_truth = REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/original_mapping_cut.tsv"
    shutil.copy2(source_truth, destination / "original-mapping-cut.tsv")
    source_cases = destination / "mapping-cut-cases"
    source_cases.mkdir()
    with source_truth.open(encoding="ascii") as stream:
        for row in csv.DictReader(stream, delimiter="\t"):
            # R CMD build removes directories ending in old, including ascii_fold.
            case = source_cases / (row["case"] + "-fixture")
            case.mkdir()
            for name, content in json.loads(row["inputs_json"]).items():
                (case / (name.lower() + ".csv")).write_bytes(bytes.fromhex(content))
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/source_filters.tsv", destination / "source-filters.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/first_available.tsv", destination / "first-available.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/original_assertions.tsv", destination / "original-assertions.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/original_all_or_none.tsv", destination / "original-all-or-none.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/original_not_missing.tsv", destination / "original-not-missing.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/row_filters.tsv", destination / "row-filters.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/row_filter_admission.tsv", destination / "row-filter-admission.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/row_reductions.tsv", destination / "row-reductions.tsv")
    shutil.copy2(REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/correlated_aggregates.tsv", destination / "correlated-aggregates.tsv")
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
    for name in (*CASES, *ROUNDING_CASES, *TEMPORAL_CASES, *IMPUTATION_CASES, *TEXT_CASES, *PAD_CASES, *EXTRACTION_CASES, *TEMPLATE_CASES, *FLAG_POSITIVE_CASES, *FLAG_NEGATIVE_CASES, "schema-verification-log"):
        shutil.copytree(REPOSITORY / "benchmarks" / name, destination / "cases" / CASE_DIRECTORY_NAMES.get(name, name))
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
    stage_public_output_truth(destination)
    stage_temporal_imputation_truth(destination)
    stage_text_case_truth(destination)
    stage_text_pad_truth(destination)
    stage_intermediate_key_truth(destination)
    stage_flag_truth(destination)
    stage_row_intermediate_truth(destination)
    stage_text_extract_truth(destination)
    stage_text_template_truth(destination)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    stage(parser.parse_args().destination)
