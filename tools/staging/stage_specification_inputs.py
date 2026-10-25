"""Stage original unchanged compiler inputs and independent report truth."""

import argparse
import csv
import datetime
import json
import shutil
from pathlib import Path

from tools.paths import ROOT
from tools.staging import build_parquet_source_inputs

REPOSITORY = ROOT
CASES = (
    "negative-zero-division",
    "negative-integer-overflow",
    "adam-adlb-ordered-sum",
    "schema-window-functions",
    "schema-inheritance",
    "schema-lookup",
    "negative-formula-flag",
    "negative-row-aggregate",
    "negative-row-no-prior",
    "negative-source-missing-field",
    "negative-source-trivial-filter",
    "negative-paired-dates",
    "negative-not-missing-age",
    "negative-implausible-age",
    "negative-invalid-sex",
    "negative-sex-code",
    "negative-matches-bad-pattern",
)
CASE_PREDICATE_CASES = (
    "adam-adae-protocol-review",
    "adam-adfa-fever",
    "adam-adtte-pfs",
    "schema-expressions",
    "schema-predicates",
    "sdtm-ae-seriousness",
    "sdtm-mh-ongoing-conditions",
    "sdtm-tr-tumor-measurements",
    "sdtm-tu-tumor-identification",
    "sdtm-ds-reasons",
)
ROUNDING_CASES = (
    "schema-round-half-away-from-zero-ties",
    "negative-round-half-away-from-zero-non-numeric-source",
)
TEMPORAL_CASES = (
    "adam-adsl-duration",
    "adam-adsl-analysis-age",
    "negative-date-diff-endpoints",
    "negative-date-diff-units",
    "negative-study-day-datetime",
    "negative-epoch-day-non-date",
)
IMPUTATION_CASES = (
    "schema-birth-date",
    "sdtm-dm-age",
    "negative-impute-bad-month",
    "negative-impute-day-rule",
    "negative-impute-bad-day",
    "negative-date-conversion",
    "negative-impute-bad-source",
    "negative-precision-bad-source",
    "negative-impute-datetime-text",
    "negative-precision-datetime-text",
)
TEXT_CASES = (
    "schema-sentence-case",
    "negative-str-case-number",
    "negative-str-case-unknown",
)
PAD_CASES = ("negative-str-pad-width",)
FLAG_POSITIVE_CASES = (
    "schema-str-contains",
    "adam-adae-occurrence-flags",
    "adam-adex-dose-reduction",
    "adam-adlb-closest-visit",
    "adam-adsl-completion-flag",
    "sdtm-dm-death",
)
FLAG_NEGATIVE_CASES = (
    "negative-flag-condition",
    "negative-flag-missing-value",
    "negative-flag-payload",
    "negative-review-arithmetic",
    "negative-review-date-text",
    "negative-review-unknown-date",
)
FLAG_HELD_CASES = ("nested_case_mixed_declarations", "implicit_raw_ambiguous")
EXTRACTION_CASES = ("adam-adae-text-cleanup", "negative-str-uncaptured-group")
TEMPLATE_CASES = ("adam-adsl-site-parse", "negative-subject-reference")
# Archive aliases shorten installed paths without changing original fixture identities or bytes.
CASE_DIRECTORY_NAMES = {
    "negative-round-half-away-from-zero-non-numeric-source": "negative-rounding-source"
}
# Only archive directories are shortened; independent capture labels remain exact.
TEMPLATE_DIRECTORY_NAMES = {
    "type-before-later-invalid-declaration": "type-before-invalid",
    "unknown-before-later-invalid-declaration": "unknown-before-invalid",
}
# The authored fixtures of the application suite, and of the adapters suite,
# which keeps those its codec tests read (#3761).
FIXTURES = REPOSITORY / "rust/crates/yamaa-application/tests/fixtures"
ADAPTER_FIXTURES = REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures"


def fixture_rows(name):
    return json.loads((FIXTURES / (name + ".json")).read_text(encoding="utf-8"))


# Installed hosts expose these five issue columns. The adapter geometry
# (`source_span`, `operand_route`) that the same fixtures assert for the Rust
# boundary stays out of host-visible truth until a host column carries it
# (#2676). Projecting here keeps the fixture the one source of truth: a row
# missing a host field or carrying a field this projection does not know is
# refused rather than silently narrowed.
HOST_ISSUE_FIELDS = ("phase", "condition", "rule", "spec_paths", "context")
ADAPTER_GEOMETRY_FIELDS = ("source_span", "operand_route")


def host_issue_rows(rows):
    """Project authored issue rows onto the host-visible issue columns."""
    projected = []
    for row in rows:
        fields = set(row)
        missing = set(HOST_ISSUE_FIELDS) - fields
        if missing:
            raise ValueError(
                "issue row misses host fields: " + ", ".join(sorted(missing))
            )
        unknown = fields - set(HOST_ISSUE_FIELDS) - set(ADAPTER_GEOMETRY_FIELDS)
        if unknown:
            raise ValueError("unknown issue fields: " + ", ".join(sorted(unknown)))
        projected.append(
            {field: value for field, value in row.items() if field in HOST_ISSUE_FIELDS}
        )
    return projected


def host_truth(source):
    """Read authored case truth with every issue row projected for installed hosts."""
    truth = json.loads(source.read_text(encoding="ascii"))
    for case in truth["cases"].values():
        for group in ("issues", "check_issues"):
            case[group] = host_issue_rows(case[group])
    return truth


def write_host_truth(truth, destination):
    """Write projected truth in the fixtures' own layout: two-space indent, ASCII."""
    destination.write_text(
        json.dumps(truth, indent=2, ensure_ascii=True) + "\n",
        encoding="ascii",
    )


def stage_json_fixtures(destination: Path):
    """Copy authored JSON directly; installed readers own no intermediate format."""
    for directory in (FIXTURES, ADAPTER_FIXTURES):
        for source in directory.glob("*.json"):
            shutil.copy2(source, destination / source.name)


def stage_text_extract_truth(destination: Path):
    """Stage originals and separately captured complete reference extraction cases."""
    source = (
        REPOSITORY
        / "rust/crates/yamaa-application/tests/fixtures/text_extract_reference.json"
    )
    shutil.copy2(source, destination / "text-extract-reference.json")
    capture = json.loads(source.read_text())
    with (destination / "text-extract-handlers.tsv").open(
        "w", newline="", encoding="ascii"
    ) as stream:
        writer = csv.writer(
            stream,
            delimiter="\t",
            lineterminator="\n",
            quoting=csv.QUOTE_NONE,
            quotechar=None,
        )
        writer.writerow(("case", "handlers"))
        for name, value in capture.items():
            if "handler_counts" in value:
                writer.writerow(
                    (
                        name,
                        json.dumps(
                            value["handler_counts"],
                            sort_keys=True,
                            separators=(",", ":"),
                        ),
                    )
                )
    root = destination / "text-extract-cases"
    root.mkdir()
    with (destination / "text-extract-diagnostics.tsv").open(
        "w", newline="", encoding="ascii"
    ) as stream:
        writer = csv.writer(
            stream,
            delimiter="\t",
            lineterminator="\n",
            quoting=csv.QUOTE_NONE,
            quotechar=None,
        )
        writer.writerow(("case", "phase", "condition", "rule", "spec_path", "context"))
        for name, value in capture.items():
            if "specification" not in value or name == "wide-group":
                continue
            case = root / name
            case.mkdir()
            (case / "spec.yaml").write_text(
                json.dumps(value["specification"], ensure_ascii=True) + "\n",
                encoding="ascii",
                newline="\n",
            )
            (case / "input.csv").write_text(
                value["input"], encoding="utf-8", newline="\n"
            )
            if "csv" in value:
                (case / "expected.csv").write_text(
                    value["csv"], encoding="utf-8", newline="\n"
                )
            for finding in value.get("findings", []):
                writer.writerow(
                    (
                        name,
                        finding["phase"],
                        finding["condition"],
                        finding["rule"],
                        finding["spec_paths"][0],
                        json.dumps(
                            finding["context"], sort_keys=True, separators=(",", ":")
                        ),
                    )
                )


def stage_text_template_truth(destination: Path):
    """Stage originals and separately captured complete reference template cases."""
    source = (
        REPOSITORY
        / "rust/crates/yamaa-application/tests/fixtures/text_template_reference.json"
    )
    shutil.copy2(source, destination / "text-template-reference.json")
    capture = json.loads(source.read_text())
    with (destination / "text-template-handlers.tsv").open(
        "w", newline="", encoding="ascii"
    ) as stream:
        writer = csv.writer(
            stream,
            delimiter="\t",
            lineterminator="\n",
            quoting=csv.QUOTE_NONE,
            quotechar=None,
        )
        writer.writerow(("case", "handlers"))
        for name, value in capture.items():
            if "handler_counts" in value:
                writer.writerow(
                    (
                        name,
                        json.dumps(
                            value["handler_counts"],
                            sort_keys=True,
                            separators=(",", ":"),
                        ),
                    )
                )
    with (destination / "text-template-directories.tsv").open(
        "w", newline="", encoding="ascii"
    ) as stream:
        writer = csv.writer(
            stream,
            delimiter="\t",
            lineterminator="\n",
            quoting=csv.QUOTE_NONE,
            quotechar=None,
        )
        writer.writerow(("case", "directory"))
        for name, value in capture.items():
            if "specification" in value:
                writer.writerow((name, TEMPLATE_DIRECTORY_NAMES.get(name, name)))
    root = destination / "text-template-cases"
    root.mkdir()
    with (destination / "text-template-diagnostics.tsv").open(
        "w", newline="", encoding="ascii"
    ) as stream:
        writer = csv.writer(
            stream,
            delimiter="\t",
            lineterminator="\n",
            quoting=csv.QUOTE_NONE,
            quotechar=None,
        )
        writer.writerow(("case", "phase", "condition", "rule", "spec_path", "context"))
        for name, value in capture.items():
            if "specification" not in value:
                continue
            case = root / TEMPLATE_DIRECTORY_NAMES.get(name, name)
            case.mkdir()
            (case / "spec.yaml").write_text(
                json.dumps(value["specification"], ensure_ascii=True) + "\n",
                encoding="ascii",
                newline="\n",
            )
            (case / "input.csv").write_text(
                value["input"], encoding="utf-8", newline="\n"
            )
            if "csv" in value:
                (case / "expected.csv").write_text(
                    value["csv"], encoding="utf-8", newline="\n"
                )
            for finding in value.get("findings", []):
                writer.writerow(
                    (
                        name,
                        finding["phase"],
                        finding["condition"],
                        finding["rule"],
                        finding["spec_paths"][0],
                        json.dumps(
                            finding["context"], sort_keys=True, separators=(",", ":")
                        ),
                    )
                )


def stage_temporal_imputation_truth(destination: Path):
    """Retain independently stated original failures and complete reference captures."""
    cases = [
        (
            "negative-impute-bad-month",
            "validation",
            "month_out_of_range",
            "TIME-10",
            "columns.ASTDT.derivation.impute.month",
            {"month": 15},
        ),
        (
            "negative-impute-day-rule",
            "validation",
            "value_not_permitted",
            "TIME-10",
            "columns.ASTDT.derivation.impute.day",
            {"value": "mid", "permitted": ["first", "last"]},
        ),
        (
            "negative-impute-bad-day",
            "impute",
            "invalid_calendar_date",
            "TIME-10",
            "columns.AENDT.derivation.impute",
            {
                "value": "2023-04",
                "completed": "2023-04-31",
                "keys": [
                    {"STUDYID": "YAMAA-01", "USUBJID": "YAMAA-01-101", "AESEQ": 2}
                ],
            },
        ),
        (
            "negative-date-conversion",
            "validation",
            "incompatible_input_type",
            "TIME-2",
            "columns.ASTDT2.derivation.to_date.source",
            {
                "source": "ASTDT",
                "expected": "datetime, ISO date text, or ISO 8601 datetime text",
                "actual": "date",
            },
        ),
    ]
    capture = json.loads(
        (
            REPOSITORY
            / "rust/crates/yamaa-application/tests/fixtures/temporal_imputation_reference.json"
        ).read_text()
    )
    for name, findings in capture.items():
        assert len(findings) == 1
        finding = findings[0]
        assert len(finding["spec_paths"]) == 1
        cases.append(
            (
                name,
                finding["phase"],
                finding["condition"],
                finding["rule"],
                finding["spec_paths"][0],
                finding["context"],
            )
        )
    assert {case[0] for case in cases} == set(IMPUTATION_CASES) - {
        "schema-birth-date",
        "sdtm-dm-age",
    }
    with (destination / "temporal-imputation-diagnostics.tsv").open(
        "w", newline="", encoding="ascii"
    ) as stream:
        writer = csv.writer(
            stream,
            delimiter="\t",
            lineterminator="\n",
            quoting=csv.QUOTE_NONE,
            quotechar=None,
        )
        writer.writerow(("case", "phase", "condition", "rule", "spec_path", "context"))
        for name, phase, condition, rule, path, context in cases:
            writer.writerow(
                (
                    name,
                    phase,
                    condition,
                    rule,
                    path,
                    json.dumps(context, sort_keys=True, separators=(",", ":")),
                )
            )


def stage_text_case_truth(destination: Path):
    """Keep complete legacy captures and stage their shared normative facts."""
    original = (
        REPOSITORY
        / "rust/crates/yamaa-application/tests/fixtures/text_case_reference.json"
    )
    shutil.copy2(original, destination / "text-case-reference.json")
    capture = json.loads(original.read_text())
    with (destination / "text-case-diagnostics.tsv").open(
        "w", newline="", encoding="ascii"
    ) as stream:
        writer = csv.writer(
            stream,
            delimiter="\t",
            lineterminator="\n",
            quoting=csv.QUOTE_NONE,
            quotechar=None,
        )
        writer.writerow(("case", "phase", "condition", "rule", "spec_path", "context"))
        for name in TEXT_CASES[1:]:
            (finding,) = capture[name]
            context = dict(finding["context"])
            if name == "negative-str-case-unknown":
                # Loader source/entry provenance is retained above, separately
                # from native schema diagnostics' normative context.
                source, entry = context.pop("source"), context.pop("entry")
                assert (
                    source
                    == entry
                    == "/private/tmp/yamaa-root-text-operators/benchmarks/negative-str-case-unknown/spec.yaml"
                )
            writer.writerow(
                (
                    name,
                    finding["phase"],
                    finding["condition"],
                    finding["rule"],
                    finding["spec_paths"][0],
                    json.dumps(context, sort_keys=True, separators=(",", ":")),
                )
            )


def stage_text_pad_truth(destination: Path):
    """Stage untouched independent captures and complete shared normative findings."""
    original = (
        REPOSITORY
        / "rust/crates/yamaa-application/tests/fixtures/text_pad_reference.json"
    )
    shutil.copy2(original, destination / "text-pad-reference.json")
    capture = json.loads(original.read_text(encoding="utf-8"))
    # Each successful case's complete handler ledger, as the report writes it.
    with (destination / "text-pad-handlers.tsv").open(
        "w", newline="", encoding="ascii"
    ) as stream:
        writer = csv.writer(
            stream,
            delimiter="\t",
            lineterminator="\n",
            quoting=csv.QUOTE_NONE,
            quotechar=None,
        )
        writer.writerow(("case", "handlers"))
        for name, case in capture.items():
            if "handlers" in case:
                writer.writerow(
                    (
                        name,
                        json.dumps(
                            case["handlers"], sort_keys=True, separators=(",", ":")
                        ),
                    )
                )
    root = destination / "text-pad-cases"
    root.mkdir()
    with (destination / "text-pad-findings.tsv").open(
        "w", newline="", encoding="ascii"
    ) as stream:
        writer = csv.writer(
            stream,
            delimiter="\t",
            lineterminator="\n",
            quoting=csv.QUOTE_NONE,
            quotechar=None,
        )
        writer.writerow(
            (
                "case",
                "index",
                "phase",
                "condition",
                "rule",
                "spec_paths",
                "context",
            )
        )
        for name, case in capture.items():
            directory = root / name
            directory.mkdir()
            (directory / "spec.yaml").write_text(
                case["specification"], encoding="utf-8", newline="\n"
            )
            (directory / "input.csv").write_text(
                case["input"], encoding="utf-8", newline="\n"
            )
            if "csv" in case:
                (directory / "expected.csv").write_text(
                    case["csv"], encoding="utf-8", newline="\n"
                )
            for index, finding in enumerate(case.get("findings", [])):
                context = dict(finding["context"])
                if name == "width-noninteger":
                    # The capture names the file its loader read, and a run names
                    # its own; the complete loader evidence is copied above.
                    for field in ("source", "entry"):
                        assert (
                            context.pop(field)
                            == "/private/tmp/yamaa-root-text-pad-audit/width-noninteger/spec.yaml"
                        )
                writer.writerow(
                    (
                        name,
                        index,
                        finding["phase"],
                        finding["condition"],
                        finding["rule"],
                        json.dumps(finding["spec_paths"], separators=(",", ":")),
                        json.dumps(context, sort_keys=True, separators=(",", ":")),
                    )
                )


def stage_flag_truth(destination: Path):
    """Retain complete independent captures and emit literal installed R truth."""
    original = (
        REPOSITORY / "rust/crates/yamaa-application/tests/fixtures/flag_reference.json"
    )
    shutil.copy2(original, destination / "flag-reference.json")
    capture = json.loads(original.read_text(encoding="utf-8"))
    authored = destination / "flags"
    authored.mkdir()
    for name, case in capture["witnesses"].items():
        directory = authored / name
        directory.mkdir()
        (directory / "spec.yaml").write_text(
            case["specification"], encoding="utf-8", newline="\n"
        )
        for filename, text in case.get("inputs", {}).items():
            (directory / filename).write_text(text, encoding="utf-8", newline="\n")
        if "csv" in case:
            (directory / "expected.csv").write_text(
                case["csv"], encoding="utf-8", newline="\n"
            )

    def text(value):
        quoted = json.dumps(value, ensure_ascii=False)
        return "".join(
            char
            if ord(char) < 128
            else (
                f"\\u{ord(char):04x}" if ord(char) <= 0xFFFF else f"\\U{ord(char):08x}"
            )
            for char in quoted
        )

    def strings(values):
        return (
            "c("
            + ",".join("NA_character_" if v is None else text(v) for v in values)
            + ")"
            if values
            else "character()"
        )

    outputs = dict(
        zip(
            FLAG_POSITIVE_CASES,
            ("adae.csv", "adae.csv", "adex.csv", "adlb.csv", "adsl.csv", "dm.csv"),
        )
    )
    entries = []
    cohorts = [
        (name, capture["originals"][name], "cases")
        for name in FLAG_POSITIVE_CASES + FLAG_NEGATIVE_CASES
    ]
    cohorts += [
        (name, case, "flags")
        for name, case in capture["witnesses"].items()
        if name not in FLAG_HELD_CASES
    ]
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
        for field in ("phase", "condition", "rule", "spec_paths", "context"):
            values = [row[field] for row in findings]
            if field == "spec_paths":
                literal = "list(" + ",".join(strings(paths) for paths in values) + ")"
            else:
                if field == "context":
                    values = [
                        json.dumps(v, sort_keys=True, separators=(",", ":"))
                        for v in values
                    ]
                literal = strings(values)
            columns.append(field + "=" + literal)
        expected_csv = ""
        if case["result_type"] == "ExecutionSuccess":
            if directory == "cases":
                (expected,) = (REPOSITORY / "benchmarks" / name / "expected").glob(
                    "*.csv"
                )
                # Committed CSV fixes lexical serialization; the retained legacy
                # dataframe capture may use decimal/datetime display padding.
                expected_csv = "expected/" + expected.name
            else:
                expected_csv = "expected.csv"
        values = [
            "directory=" + text(directory),
            "output=" + text(outputs.get(name, "out.csv")),
            "expected_csv=" + text(expected_csv),
            "specification=" + text(case["specification"]),
            "schema_failure="
            + ("TRUE" if case["result_type"] == "SpecificationError" else "FALSE"),
            "handlers="
            + text(
                json.dumps(
                    case.get("handlers", []), sort_keys=True, separators=(",", ":")
                )
            ),
            "findings=list(" + ",".join(columns) + ")",
        ]
        entries.append(text(name) + "=list(" + ",".join(values) + ")")
    (destination / "flag-truth.R").write_text(
        "# Independent original CSV/findings/handlers; no candidate execution.\n"
        "# Held: generic case (NAME-3, FORMULA-9) and recursive report context PR1933.\n"
        "flag_truth <- list(\n" + ",\n".join(entries) + "\n)\n",
        encoding="ascii",
    )


def stage_intermediate_key_truth(destination: Path):
    """Stage all unchanged scalar-key captures without evaluating the candidate."""
    original = (
        REPOSITORY
        / "rust/crates/yamaa-application/tests/fixtures/intermediate_key_reference.json"
    )
    shutil.copy2(original, destination / "key-reference.json")
    capture = json.loads(original.read_text(encoding="utf-8"))
    assert len(capture) == 19
    root = destination / "keys"
    root.mkdir()

    def text(value):
        return json.dumps(value, ensure_ascii=True)

    def strings(values):
        return (
            "c("
            + ",".join(
                "NA_character_" if value is None else text(value) for value in values
            )
            + ")"
            if values
            else "character()"
        )

    entries = []
    for name, case in capture.items():
        directory = root / name
        directory.mkdir()
        (directory / "spec.yaml").write_text(
            case["specification"], encoding="utf-8", newline="\n"
        )
        for filename, source in case["inputs"].items():
            (directory / filename).write_text(source, encoding="utf-8", newline="\n")
        if "csv" in case:
            (directory / "expected.csv").write_text(
                case["csv"], encoding="utf-8", newline="\n"
            )
        columns = []
        for field in ("phase", "condition", "rule", "spec_paths", "context"):
            values = [finding[field] for finding in case.get("findings", [])]
            if field == "spec_paths":
                literal = "list(" + ",".join(strings(paths) for paths in values) + ")"
            else:
                if field == "context":
                    values = [
                        json.dumps(value, sort_keys=True, separators=(",", ":"))
                        for value in values
                    ]
                literal = strings(values)
            columns.append(field + "=" + literal)
        entries.append(
            text(name)
            + "=list(issues=list("
            + ",".join(columns)
            + "),success="
            + ("TRUE" if "csv" in case else "FALSE")
            + ",handlers="
            + text(json.dumps(case["handlers"], sort_keys=True, separators=(",", ":")))
            + ")"
        )
    (destination / "key-truth.R").write_text(
        "# All 19 unchanged independent scalar-key findings, CSV and handlers.\n"
        "key_truth <- list(\n" + ",\n".join(entries) + "\n)\n",
        encoding="ascii",
    )


def stage_row_intermediate_truth(destination: Path):
    """Materialize unchanged originals and complete independently captured row truth."""
    original = (
        REPOSITORY
        / "rust/crates/yamaa-application/tests/fixtures/row_intermediate_reference.json"
    )
    shutil.copy2(original, destination / "row-intermediate-reference.json")
    reference = json.loads(original.read_text(encoding="ascii"))
    root = destination / "row-cases"
    root.mkdir()
    cases = dict(reference["cases"], **reference["originals"])

    def text(value):
        return json.dumps(value, ensure_ascii=True)

    def strings(values):
        return (
            "c("
            + ",".join(
                "NA_character_" if value is None else text(value) for value in values
            )
            + ")"
            if values
            else "character()"
        )

    entries = []
    for name, case in cases.items():
        directory = root / name
        directory.mkdir()
        if name in reference["originals"]:
            # The full copied original keeps its source/expected fixtures unchanged.
            shutil.copytree(
                REPOSITORY / "benchmarks" / name, directory, dirs_exist_ok=True
            )
            assert (directory / "spec.yaml").read_bytes() == case[
                "specification"
            ].encode()
            for filename, source in case["inputs"].items():
                assert (directory / "input" / filename).read_bytes() == source.encode()
        else:
            (directory / "spec.yaml").write_text(
                case["specification"], encoding="utf-8", newline="\n"
            )
            for filename, source in case["inputs"].items():
                (directory / filename).write_text(
                    source, encoding="utf-8", newline="\n"
                )
        findings = case["findings"]
        success = "csv" in case
        if success:
            saved_csv = case.get("expected_csv", case["csv"])
            (directory / "expected.csv").write_text(
                saved_csv, encoding="utf-8", newline="\n"
            )
        (directory / "findings.json").write_text(
            json.dumps(findings, ensure_ascii=True, sort_keys=True) + "\n",
            encoding="ascii",
        )
        fields = []
        for field in ("phase", "condition", "rule", "spec_paths", "context"):
            values = [finding[field] for finding in findings]
            if field == "spec_paths":
                literal = "list(" + ",".join(strings(paths) for paths in values) + ")"
            else:
                if field == "context":
                    values = [
                        json.dumps(
                            value,
                            ensure_ascii=True,
                            sort_keys=True,
                            separators=(",", ":"),
                        )
                        for value in values
                    ]
                literal = strings(values)
            fields.append(field + "=" + literal)
        ledger = case.get("handler_counts", [])
        output = "adlb.csv" if name in reference["originals"] else "result.csv"
        entries.append(
            text(name)
            + "=list(issues=list("
            + ",".join(fields)
            + "),success="
            + ("TRUE" if success else "FALSE")
            + ",output="
            + text(output)
            + ",ledger="
            + text(json.dumps(ledger, sort_keys=True, separators=(",", ":")))
            + ")"
        )
    (destination / "row-intermediate-truth.R").write_text(
        "# Complete independent row-context truth, including EOT derivations and verifications.\nrow_intermediate_truth <- list(\n"
        + ",\n".join(entries)
        + "\n)\n",
        encoding="ascii",
    )


def stage_issue_frame_truth(destination: Path):
    """Project committed report truth to literal R column expectations, never candidate output."""
    reports = {
        name: json.loads((destination / "expected" / (name + ".json")).read_text())[
            "diagnostics"
        ]
        for name in CASES
    }
    for case in json.loads(
        (destination / "static_verification_checks.json").read_text(encoding="utf-8")
    ):
        reports["static/" + case["case"]] = case["expected"]

    def text(value):
        # JSON's surrogate-pair escapes are not R string escapes. Emit code points.
        quoted = json.dumps(value, ensure_ascii=False)
        return "".join(
            char
            if ord(char) < 128
            else (
                f"\\u{ord(char):04x}" if ord(char) <= 0xFFFF else f"\\U{ord(char):08x}"
            )
            for char in quoted
        )

    def strings(values):
        return (
            "c("
            + ",".join("NA_character_" if v is None else text(v) for v in values)
            + ")"
            if values
            else "character()"
        )

    entries = []
    for name, diagnostics in reports.items():
        columns = []
        for field in ("phase", "condition", "rule", "spec_paths", "context"):
            values = [row[field] for row in diagnostics]
            if field == "spec_paths":
                literal = "list(" + ",".join(strings(paths) for paths in values) + ")"
            else:
                if field == "context":
                    values = [
                        json.dumps(
                            v, ensure_ascii=False, sort_keys=True, separators=(",", ":")
                        )
                        if isinstance(v, dict)
                        else v
                        for v in values
                    ]
                literal = strings(values)
            columns.append(field + "=" + literal)
        entries.append(text(name) + "=list(" + ",".join(columns) + ")")
    (destination / "issue-frame-truth.R").write_text(
        "# Literal projection of committed expected diagnostics; no candidate execution.\nissue_frame_truth <- list(\n"
        + ",\n".join(entries)
        + "\n)\n",
        encoding="ascii",
    )


def stage_public_output_truth(destination: Path):
    """Literal plain R columns from committed original report observations.

    Text is character, `int` and `float` are doubles, dates are `Date` and
    datetimes UTC `POSIXct`, as the public R output holds them.
    """

    def text(value):
        quoted = json.dumps(value, ensure_ascii=False)
        return "".join(
            char
            if ord(char) < 128
            else (
                f"\\u{ord(char):04x}" if ord(char) <= 0xFFFF else f"\\U{ord(char):08x}"
            )
            for char in quoted
        )

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
                    cells.append("NA_character_" if kind == "str" else "NA_real_")
                elif kind == "str":
                    cells.append(text(scalar["value"]))
                elif kind == "int":
                    # R returns integers as plain doubles; one beyond 2^53
                    # has no exact double, so the public R view raises.
                    value = int(scalar["value"])
                    if abs(value) > 2**53:
                        raise ValueError(
                            "an integer beyond 2^53 has no exact R double: "
                            + scalar["value"]
                        )
                    cells.append(str(value))
                elif kind == "float":
                    raw = bytes.fromhex(scalar["value"])[::-1]
                    cells.append(
                        "readBin(as.raw(c("
                        + ",".join(str(byte) for byte in raw)
                        + ")), 'double', n=1L, size=8L, endian='little')"
                    )
                elif kind == "date":
                    cells.append(
                        str(
                            (
                                datetime.date.fromisoformat(scalar["value"])
                                - datetime.date(1970, 1, 1)
                            ).days
                        )
                    )
                elif kind == "datetime":
                    cells.append(
                        str(
                            int(
                                (
                                    datetime.datetime.fromisoformat(scalar["value"])
                                    - datetime.datetime(1970, 1, 1)
                                ).total_seconds()
                            )
                        )
                    )
                else:
                    raise ValueError("unclassified public output type")
            literal = (
                "c(" + ",".join(cells) + ")"
                if cells
                else ("character()" if kind == "str" else "double()")
            )
            if kind == "date":
                literal = "structure(as.double(" + literal + "),class='Date')"
            elif kind == "datetime":
                literal = (
                    "structure(as.double("
                    + literal
                    + "),class=c('POSIXct','POSIXt'),tzone='UTC')"
                )
            columns.append(text(column_name) + "=" + literal)
        entries.append(text(name) + "=list(" + ",".join(columns) + ")")
    (destination / "public-output-truth.R").write_text(
        "# Literal columns from committed reference report truth.\npublic_output_truth <- list(\n"
        + ",\n".join(entries)
        + "\n)\n",
        encoding="ascii",
    )


def stage_source_selection_truth(
    destination: Path, fixture="source_selection", label="source-selection"
):
    """Project authored typed cells into both installed hosts without executing them."""
    source = REPOSITORY / (
        "rust/crates/yamaa-application/tests/fixtures/" + fixture + ".json"
    )
    truth = host_truth(source)
    write_host_truth(truth, destination / (label + "-truth.json"))
    directories = destination / (label + "-cases")
    directories.mkdir()

    def text(value):
        return json.dumps(value, ensure_ascii=True)

    def vector(values):
        return "c(" + ",".join(values) + ")" if values else "character()"

    def issues(rows):
        fields = []
        for field in ("phase", "condition", "rule", "context"):
            values = [
                text(json.dumps(row[field], sort_keys=True, separators=(",", ":")))
                if field == "context"
                else text(row[field])
                for row in rows
            ]
            fields.append(field + "=" + vector(values))
        paths = [vector([text(path) for path in row["spec_paths"]]) for row in rows]
        fields.insert(3, "spec_paths=list(" + ",".join(paths) + ")")
        return "list(" + ",".join(fields) + ")"

    entries = []
    case_paths = {}
    for index, (name, case) in enumerate(truth["cases"].items(), start=1):
        case_paths[name] = f"{index:04d}"
        directory = directories / case_paths[name]
        directory.mkdir()
        (directory / "spec.yaml").write_text(
            case["specification"], encoding="ascii", newline="\n"
        )
        for path, contents in case["inputs"].items():
            (directory / path).write_text(contents, encoding="ascii", newline="\n")
        fields = [
            "directory=" + text(case_paths[name]),
            "issues=" + issues(case["issues"]),
            "check_issues=" + issues(case["check_issues"]),
            "handlers="
            + text(json.dumps(case["handlers"], sort_keys=True, separators=(",", ":"))),
        ]
        reads = [
            {
                "base_directory": ".",
                "condition": None,
                "outcome": "captured",
                "path": path,
                "snapshots_created": 1,
            }
            for path in case["source_reads"]
        ]
        fields.append(
            "source_reads="
            + text(json.dumps(reads, sort_keys=True, separators=(",", ":")))
        )
        if "table" in case:
            (directory / "expected.csv").write_text(
                case["csv"], encoding="ascii", newline="\n"
            )
            columns, kinds = [], []
            for index, (column, kind) in enumerate(case["table"]["columns"]):
                values = [
                    "NA_character_" if row[index] is None else text(row[index])
                    for row in case["table"]["rows"]
                ]
                columns.append(text(column) + "=" + vector(values))
                kinds.append(text(column) + "=" + text(kind))
            fields.extend(
                (
                    "columns=list(" + ",".join(columns) + ")",
                    "kinds=c(" + ",".join(kinds) + ")",
                )
            )
        entries.append(text(name) + "=list(" + ",".join(fields) + ")")
    (destination / (label + "-case-paths.json")).write_text(
        json.dumps(case_paths, indent=2) + "\n", encoding="ascii"
    )
    (destination / (label + "-truth.R")).write_text(
        "# Literal typed cells and complete findings from independent source-selection truth.\n"
        "source_selection_truth <- list(\n" + ",\n".join(entries) + "\n)\n",
        encoding="ascii",
    )


def stage_completed_lookup_assertion_truth(destination: Path):
    """Project authored typed cells into both installed hosts without executing them."""
    fixture, label = "completed_lookup_assertions", "completed-lookup-assertions"
    source = REPOSITORY / (
        "rust/crates/yamaa-application/tests/fixtures/" + fixture + ".json"
    )
    truth = host_truth(source)
    write_host_truth(truth, destination / (label + "-truth.json"))
    directories = destination / (label + "-cases")
    directories.mkdir()

    def text(value):
        return json.dumps(value, ensure_ascii=True)

    def vector(values):
        return "c(" + ",".join(values) + ")" if values else "character()"

    def issues(rows):
        fields = []
        for field in ("phase", "condition", "rule", "context"):
            values = [
                text(json.dumps(row[field], sort_keys=True, separators=(",", ":")))
                if field == "context"
                else text(row[field])
                for row in rows
            ]
            fields.append(field + "=" + vector(values))
        paths = [vector([text(path) for path in row["spec_paths"]]) for row in rows]
        fields.insert(3, "spec_paths=list(" + ",".join(paths) + ")")
        return "list(" + ",".join(fields) + ")"

    entries = []
    case_paths = {}
    for index, (name, case) in enumerate(truth["cases"].items(), start=1):
        case_paths[name] = f"{index:04d}"
        directory = directories / case_paths[name]
        directory.mkdir()
        (directory / "spec.yaml").write_text(
            case["specification"], encoding="ascii", newline="\n"
        )
        for path, contents in case["inputs"].items():
            (directory / path).write_text(contents, encoding="ascii", newline="\n")
        fields = [
            "directory=" + text(case_paths[name]),
            "issues=" + issues(case["issues"]),
            "check_issues=" + issues(case["check_issues"]),
            "handlers="
            + text(json.dumps(case["handlers"], sort_keys=True, separators=(",", ":"))),
        ]
        if "verifications" in case:
            records = [
                {**record, "specification": "__SPECIFICATION__"}
                for record in case["verifications"]
            ]
            fields.append(
                "verifications="
                + text(json.dumps(records, sort_keys=True, separators=(",", ":")))
            )
        reads = [
            {
                "base_directory": ".",
                "condition": None,
                "outcome": "captured",
                "path": path,
                "snapshots_created": 1,
            }
            for path in case["source_reads"]
        ]
        fields.append(
            "source_reads="
            + text(json.dumps(reads, sort_keys=True, separators=(",", ":")))
        )
        if "table" in case:
            (directory / "expected.csv").write_text(
                case["csv"], encoding="ascii", newline="\n"
            )
            columns, kinds = [], []
            for index, (column, kind) in enumerate(case["table"]["columns"]):
                values = [
                    "NA_character_" if row[index] is None else text(row[index])
                    for row in case["table"]["rows"]
                ]
                columns.append(text(column) + "=" + vector(values))
                kinds.append(text(column) + "=" + text(kind))
            fields.extend(
                (
                    "columns=list(" + ",".join(columns) + ")",
                    "kinds=c(" + ",".join(kinds) + ")",
                )
            )
        entries.append(text(name) + "=list(" + ",".join(fields) + ")")
    (destination / (label + "-case-paths.json")).write_text(
        json.dumps(case_paths, indent=2) + "\n", encoding="ascii"
    )
    (destination / (label + "-truth.R")).write_text(
        "# Literal typed cells and complete findings from independent completed-row assertion truth.\n"
        "completed_lookup_assertion_truth <- list(\n" + ",\n".join(entries) + "\n)\n",
        encoding="ascii",
    )


def stage(destination: Path):
    """Keep original files separate from independently authored expected reports."""
    destination.mkdir(parents=True, exist_ok=False)
    stage_completed_lookup_assertion_truth(destination)
    stage_json_fixtures(destination)
    stage_source_selection_truth(destination)
    stage_source_selection_truth(
        destination, "source_filter_scope", "source-filter-scope"
    )
    stage_source_selection_truth(
        destination, "source_filter_mixed", "source-filter-mixed"
    )
    shutil.copy2(
        REPOSITORY / "rust/crates/yamaa-application/tests/fixtures/preflight.tsv",
        destination / "preflight.tsv",
    )
    shutil.copy2(
        REPOSITORY
        / "rust/crates/yamaa-application/tests/fixtures/output_declarations.tsv",
        destination / "output-declarations.tsv",
    )
    shutil.copy2(
        REPOSITORY
        / "rust/crates/yamaa-application/tests/fixtures/grammar_diagnostics.tsv",
        destination / "grammar-diagnostics.tsv",
    )
    shutil.copy2(
        REPOSITORY
        / "rust/crates/yamaa-application/tests/fixtures/binding_diagnostics.tsv",
        destination / "binding-diagnostics.tsv",
    )
    shutil.copy2(
        REPOSITORY
        / "rust/crates/yamaa-application/tests/fixtures/window_diagnostics.tsv",
        destination / "window-diagnostics.tsv",
    )
    shutil.copy2(
        REPOSITORY
        / "rust/crates/yamaa-application/tests/fixtures/predicate_diagnostics.tsv",
        destination / "predicate-diagnostics.tsv",
    )
    shutil.copy2(
        REPOSITORY / "rust/crates/yamaa-application/tests/fixtures/source_capture.tsv",
        destination / "source-capture.tsv",
    )
    shutil.copy2(
        REPOSITORY
        / "rust/crates/yamaa-application/tests/fixtures/source_inspection.tsv",
        destination / "source-inspection.tsv",
    )
    shutil.copy2(
        REPOSITORY
        / "rust/crates/yamaa-application/tests/fixtures/case_predicates.json",
        destination / "case-predicate-truth.json",
    )
    truth = json.loads((destination / "case-predicate-truth.json").read_text())
    dispositions_path = (
        REPOSITORY
        / "rust/crates/yamaa-application/tests/fixtures/case_predicate_dispositions.json"
    )
    shutil.copy2(dispositions_path, destination / "case-predicate-dispositions.json")
    dispositions = json.loads(dispositions_path.read_text())["cases"]
    entries = [
        json.dumps(name)
        + "=list("
        + ",".join(
            field + "=" + json.dumps(value) for field, value in disposition.items()
        )
        + ")"
        for name, disposition in dispositions.items()
    ]
    (destination / "case-predicate-dispositions.R").write_text(
        "# Explicit correction metadata; historical CSV is never replaced.\n"
        "case_predicate_dispositions <- list(" + ",".join(entries) + ")\n",
        encoding="ascii",
    )
    history = destination / "case-predicate-history"
    history.mkdir()
    private_cases = destination / "case-predicate-cases"
    private_cases.mkdir()
    with (destination / "case-predicate-diagnostics.tsv").open(
        "w", newline="", encoding="ascii"
    ) as stream:
        writer = csv.writer(
            stream,
            delimiter="\t",
            lineterminator="\n",
            quoting=csv.QUOTE_NONE,
            quotechar=None,
        )
        writer.writerow(("case", "phase", "condition", "rule", "spec_path", "context"))
        for name, case in truth["cases"].items():
            if name in dispositions:
                assert case["csv"] == dispositions[name]["historical_csv"]
            directory = (history if name in dispositions else private_cases) / name
            directory.mkdir()
            (directory / "spec.yaml").write_text(
                case["specification"], encoding="ascii", newline="\n"
            )
            for filename, contents in case["inputs"].items():
                (directory / filename).write_text(
                    contents, encoding="ascii", newline="\n"
                )
            if "csv" in case:
                (directory / "expected.csv").write_text(
                    case["csv"], encoding="ascii", newline="\n"
                )
            for finding in case.get("findings", []):
                writer.writerow(
                    (
                        name,
                        finding["phase"],
                        finding["condition"],
                        finding["rule"],
                        finding["spec_paths"][0],
                        json.dumps(
                            finding["context"], sort_keys=True, separators=(",", ":")
                        ),
                    )
                )
    source_cases = destination / "source-binding-cases"
    source_cases.mkdir()
    for row in fixture_rows("original_source_bindings"):
        case = source_cases / row["case"]
        case.mkdir()
        for name, content in row["inputs"].items():
            (case / (name.lower() + ".csv")).write_bytes(content.encode("utf-8"))
    source_cases = destination / "mapping-cut-cases"
    source_cases.mkdir()
    for row in fixture_rows("original_mapping_cut"):
        # R CMD build removes directories ending in old, including ascii_fold.
        case = source_cases / (row["case"] + "-fixture")
        case.mkdir()
        for name, content in row["inputs"].items():
            (case / (name.lower() + ".csv")).write_bytes(content.encode("utf-8"))
    for name in (
        *CASES,
        *CASE_PREDICATE_CASES,
        *ROUNDING_CASES,
        *TEMPORAL_CASES,
        *IMPUTATION_CASES,
        *TEXT_CASES,
        *PAD_CASES,
        *EXTRACTION_CASES,
        *TEMPLATE_CASES,
        *FLAG_POSITIVE_CASES,
        *FLAG_NEGATIVE_CASES,
        "schema-verification-log",
    ):
        shutil.copytree(
            REPOSITORY / "benchmarks" / name,
            destination / "cases" / CASE_DIRECTORY_NAMES.get(name, name),
        )
    shutil.copytree(
        REPOSITORY / "rust/crates/yamaa-application/tests/fixtures/specs",
        destination / "expected",
    )
    build_parquet_source_inputs.build(destination / "pq")

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
