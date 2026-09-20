"""The Python conformance adapter runs real examples and judges them strictly.

Every mutation below changes what an example committed and expects the
comparison to fail. A test that passes after such a change would mean the
comparison normalizes something #101 requires it to keep.
"""

from __future__ import annotations

import json
import shutil
from pathlib import Path

import pytest
import yaml

from yamaa.adapters.conformance import (
    REPORT_VERSION,
    ConformanceError,
    ExampleReport,
    HandlerObservation,
    compare_example,
    execute_example,
    expected_kind,
    main,
    write_report,
)

REPOSITORY_ROOT = Path(__file__).parents[3]
SCHEMA_ROOT = REPOSITORY_ROOT / "yaml"
EXAMPLES = REPOSITORY_ROOT / "benchmarks"
POSITIVE = "sdtm-dm-basic"
NEGATIVE = "negative-column-type-unknown"
# A specification that calls a project function; with the project root it
# carries removed, the call is a logical one no implementation answers.
PORTABLE = "adam-adsl-bmi-function"

# What the engine reports for every handler path sdtm-dm-basic declares.
POSITIVE_HANDLERS = (
    HandlerObservation(
        spec_path="columns.SEX.derivation.mapping.missing",
        handler="missing",
        count=1,
    ),
    HandlerObservation(
        spec_path="columns.SEX.derivation.mapping.unmapped",
        handler="unmapped",
        count=1,
    ),
)


def run(example: Path, tmp_path: Path) -> ExampleReport:
    return execute_example(
        example,
        schema_root=SCHEMA_ROOT,
        output_dir=tmp_path / "artifacts" / example.name,
    )


def copy_example(name: str, tmp_path: Path) -> Path:
    """Copy one committed example so a test may mutate what it committed."""
    target = tmp_path / "examples" / name
    shutil.copytree(EXAMPLES / name, target)
    return target


def verdict_of(name: str, tmp_path: Path, example: Path | None = None):
    source = example if example is not None else EXAMPLES / name
    return compare_example(run(source, tmp_path), source)


def kinds(verdict) -> set[str]:
    return {finding.kind for finding in verdict.findings}


class TestSupportedPair:
    """The two covered examples execute and match what they committed."""

    def test_positive_example_matches_its_committed_artifact(
        self, tmp_path: Path
    ) -> None:
        report = run(EXAMPLES / POSITIVE, tmp_path)

        assert report.outcome == "success"
        assert report.runtime == "python"
        assert [item.name for item in report.artifacts] == ["dm"]
        assert compare_example(report, EXAMPLES / POSITIVE).passed

    def test_positive_report_records_order_types_and_handler_counts(
        self, tmp_path: Path
    ) -> None:
        artifact = run(EXAMPLES / POSITIVE, tmp_path).artifacts[0]

        assert artifact.columns == (
            "DOMAIN",
            "STUDYID",
            "USUBJID",
            "SUBJID",
            "SEX",
            "AGE",
            "ARM",
            "ACTARM",
            "ARMNRS",
        )
        assert artifact.types == (
            "str",
            "str",
            "str",
            "str",
            "str",
            "int",
            "str",
            "str",
            "str",
        )
        assert artifact.row_count == 4
        # A missing AGE renders as no characters at all (REQ-0731).
        assert artifact.records[3] == (
            "DM,STUDY01,003,003,U,,,,Not assigned to treatment arm"
        )

    def test_positive_run_reports_every_declared_handler_path(
        self, tmp_path: Path
    ) -> None:
        report = run(EXAMPLES / POSITIVE, tmp_path)

        assert report.handler_counts == POSITIVE_HANDLERS
        assert compare_example(
            report,
            EXAMPLES / POSITIVE,
            expected_handler_counts=POSITIVE_HANDLERS,
        ).passed

    def test_negative_example_matches_its_committed_contract(
        self, tmp_path: Path
    ) -> None:
        report = run(EXAMPLES / NEGATIVE, tmp_path)

        assert report.outcome == "failure"
        assert report.diagnostics[0].phase == "validation"
        assert report.diagnostics[0].condition == "value_not_permitted"
        assert report.diagnostics[0].spec_paths == ("columns.AVAL.type",)
        assert report.diagnostics[0].requirement == "REQ-0012"
        assert compare_example(report, EXAMPLES / NEGATIVE).passed

    def test_expected_kind_reads_the_committed_artifact(self) -> None:
        assert expected_kind(EXAMPLES / POSITIVE) == "positive"
        assert expected_kind(EXAMPLES / NEGATIVE) == "negative"


class TestArtifactMutations:
    """A changed golden artifact fails; nothing here is normalized away."""

    def test_a_changed_cell_fails(self, tmp_path: Path) -> None:
        example = copy_example(POSITIVE, tmp_path)
        golden = example / "expected/dm.csv"
        golden.write_bytes(golden.read_bytes().replace(b",34,", b",35,"))

        verdict = verdict_of(POSITIVE, tmp_path, example)

        assert not verdict.passed
        assert "artifact.record" in kinds(verdict)

    def test_a_reordered_header_fails(self, tmp_path: Path) -> None:
        example = copy_example(POSITIVE, tmp_path)
        golden = example / "expected/dm.csv"
        records = golden.read_text(encoding="utf-8").split("\n")
        records[0] = "STUDYID,DOMAIN,USUBJID,SUBJID,SEX,AGE,ARM,ACTARM"
        golden.write_text("\n".join(records), encoding="utf-8")

        verdict = verdict_of(POSITIVE, tmp_path, example)

        assert not verdict.passed
        assert "artifact.columns" in kinds(verdict)

    def test_reordered_rows_fail(self, tmp_path: Path) -> None:
        example = copy_example(POSITIVE, tmp_path)
        golden = example / "expected/dm.csv"
        records = golden.read_text(encoding="utf-8").split("\n")
        records[1], records[2] = records[2], records[1]
        golden.write_text("\n".join(records), encoding="utf-8")

        verdict = verdict_of(POSITIVE, tmp_path, example)

        assert not verdict.passed
        assert "artifact.record" in kinds(verdict)

    def test_a_dropped_row_fails(self, tmp_path: Path) -> None:
        example = copy_example(POSITIVE, tmp_path)
        golden = example / "expected/dm.csv"
        records = golden.read_text(encoding="utf-8").split("\n")
        golden.write_text("\n".join(records[:-2] + [""]), encoding="utf-8")

        verdict = verdict_of(POSITIVE, tmp_path, example)

        assert not verdict.passed
        assert "artifact.row_count" in kinds(verdict)

    def test_a_quoted_empty_field_is_not_a_missing_value(self, tmp_path: Path) -> None:
        example = copy_example(POSITIVE, tmp_path)
        golden = example / "expected/dm.csv"
        golden.write_bytes(golden.read_bytes().replace(b",U,,", b',U,"",'))

        verdict = verdict_of(POSITIVE, tmp_path, example)

        assert not verdict.passed
        assert "artifact.record" in kinds(verdict)

    def test_a_renamed_golden_artifact_fails(self, tmp_path: Path) -> None:
        example = copy_example(POSITIVE, tmp_path)
        golden = example / "expected/dm.csv"
        golden.rename(golden.with_name("other.csv"))

        verdict = verdict_of(POSITIVE, tmp_path, example)

        assert not verdict.passed
        assert kinds(verdict) == {"artifact.missing", "artifact.unexpected"}


class TestDiagnosticMutations:
    """A changed error contract fails on every compared dimension."""

    @pytest.mark.parametrize(
        ("field", "value", "kind"),
        [
            ("phase", "execution", "diagnostic.phase"),
            ("condition", "value_out_of_range", "diagnostic.condition"),
            ("spec_paths", ["columns.LBSEQ.type"], "diagnostic.spec_paths"),
            ("requirement", "REQ-0013", "diagnostic.requirement"),
        ],
    )
    def test_a_changed_contract_field_fails(
        self,
        tmp_path: Path,
        field: str,
        value: object,
        kind: str,
    ) -> None:
        example = copy_example(NEGATIVE, tmp_path)
        _write_contract(example / "expected/error.yaml", {field: value})

        verdict = verdict_of(NEGATIVE, tmp_path, example)

        assert not verdict.passed
        assert kind in kinds(verdict)

    def test_a_changed_context_value_fails(self, tmp_path: Path) -> None:
        example = copy_example(NEGATIVE, tmp_path)
        _write_contract(example / "expected/error.yaml", {"context": {"value": "text"}})

        verdict = verdict_of(NEGATIVE, tmp_path, example)

        assert not verdict.passed
        assert "diagnostic.context" in kinds(verdict)

    def test_a_context_key_the_run_never_carried_fails(self, tmp_path: Path) -> None:
        example = copy_example(NEGATIVE, tmp_path)
        _write_contract(
            example / "expected/error.yaml", {"context": {"column": "AVAL"}}
        )

        verdict = verdict_of(NEGATIVE, tmp_path, example)

        assert not verdict.passed
        assert "diagnostic.context" in kinds(verdict)


class TestHandlerCountMutations:
    """A handler count that drifts from what a caller requires fails."""

    def test_a_drifted_count_fails(self, tmp_path: Path) -> None:
        required = list(POSITIVE_HANDLERS)
        required[0] = required[0].model_copy(update={"count": 2})

        verdict = compare_example(
            run(EXAMPLES / POSITIVE, tmp_path),
            EXAMPLES / POSITIVE,
            expected_handler_counts=required,
        )

        assert not verdict.passed
        assert "handler.count" in kinds(verdict)

    def test_an_unreported_handler_path_fails(self, tmp_path: Path) -> None:
        required = [
            *POSITIVE_HANDLERS,
            HandlerObservation(
                spec_path="columns.ACTARM.derivation.source.missing",
                handler="missing",
                count=0,
            ),
        ]

        verdict = compare_example(
            run(EXAMPLES / POSITIVE, tmp_path),
            EXAMPLES / POSITIVE,
            expected_handler_counts=required,
        )

        assert not verdict.passed
        assert "handler.missing" in kinds(verdict)

    def test_a_handler_path_no_one_required_fails(self, tmp_path: Path) -> None:
        verdict = compare_example(
            run(EXAMPLES / POSITIVE, tmp_path),
            EXAMPLES / POSITIVE,
            expected_handler_counts=POSITIVE_HANDLERS[:-1],
        )

        assert not verdict.passed
        assert "handler.unexpected" in kinds(verdict)


def _write_contract(path: Path, changes: dict) -> None:
    """Rewrite one committed error contract with the supplied changes."""
    with open(path, encoding="utf-8") as handle:
        document = yaml.safe_load(handle)
    for key, value in changes.items():
        if isinstance(value, dict):
            document[key] = {**document.get(key, {}), **value}
        else:
            document[key] = value
    with open(path, "w", encoding="utf-8") as handle:
        yaml.safe_dump(document, handle, sort_keys=False)


class TestExpectedArtifactsAreOutOfReach:
    """The executor cannot read the answer it is being judged against."""

    def test_a_run_without_expected_reports_exactly_the_same_thing(
        self, tmp_path: Path
    ) -> None:
        """Deleting `expected/` changes nothing the executor reports.

        If the engine ever read a committed artifact, removing it would
        change the report; a report identical without it is the evidence
        that nothing under `expected/` was consulted.
        """
        complete = copy_example(POSITIVE, tmp_path / "with")
        stripped = copy_example(POSITIVE, tmp_path / "without")
        shutil.rmtree(stripped / "expected")

        with_expected = run(complete, tmp_path / "with")
        without_expected = run(stripped, tmp_path / "without")

        assert without_expected == with_expected

    def test_a_negative_run_without_its_contract_reports_the_same_failure(
        self, tmp_path: Path
    ) -> None:
        complete = copy_example(NEGATIVE, tmp_path / "with")
        stripped = copy_example(NEGATIVE, tmp_path / "without")
        shutil.rmtree(stripped / "expected")

        with_contract = run(complete, tmp_path / "with")
        without_contract = run(stripped, tmp_path / "without")

        assert without_contract == with_contract
        assert without_contract.outcome == "failure"

    def test_only_the_comparison_half_names_the_committed_answer(self) -> None:
        """`expected/` is named only after execution is over.

        The two tests above prove a run ignores the directory. This one
        keeps the halves apart in the source, so a later edit cannot
        quietly hand the executor the answer it is being judged against.
        """
        from yamaa.adapters import conformance

        source = Path(conformance.__file__).read_text(encoding="utf-8")
        _constants, rest = source.split("class _FrozenModel", 1)
        execution, comparison = rest.split("def _finding(", 1)

        assert "EXPECTED_DIR" not in execution
        assert "ERROR_CONTRACT" not in execution
        assert "EXPECTED_DIR" in comparison
        assert "ERROR_CONTRACT" in comparison

    def test_a_run_writing_into_its_own_example_is_refused(
        self, tmp_path: Path
    ) -> None:
        example = copy_example(POSITIVE, tmp_path)

        with pytest.raises(ConformanceError):
            execute_example(
                example,
                schema_root=SCHEMA_ROOT,
                output_dir=example / "expected",
            )

    def test_a_run_leaves_the_example_directory_untouched(self, tmp_path: Path) -> None:
        example = copy_example(POSITIVE, tmp_path)
        before = sorted(path.name for path in example.rglob("*"))

        run(example, tmp_path)

        assert sorted(path.name for path in example.rglob("*")) == before


class TestUnsupportedAndMissingPrerequisites:
    """Neither an unsupported run nor a broken one passes for an example."""

    def test_an_unsupported_positive_example_fails_rather_than_skips(
        self, tmp_path: Path
    ) -> None:
        # With no project root to select, the call is a logical one no
        # implementation answers, which is unsupported rather than a result.
        # A positive example must still execute, so it fails here.
        example = copy_example(PORTABLE, tmp_path)
        shutil.rmtree(example / "python")
        (example / "environment.yaml").unlink()

        report = run(example, tmp_path)
        verdict = compare_example(report, example)

        assert report.outcome == "unsupported"
        assert report.unsupported[0].operation == "function"
        assert not verdict.passed
        assert kinds(verdict) == {"outcome"}

    def test_a_selected_project_root_executes_the_call(self, tmp_path: Path) -> None:
        # The same example with its `python/` root in place: this runner
        # selects the root for the language it speaks and runs it.
        report = run(EXAMPLES / PORTABLE, tmp_path)

        assert report.outcome == "success", report
        assert compare_example(report, EXAMPLES / PORTABLE).passed

    @pytest.mark.parametrize("outcome", ["unsupported", "error", "success"])
    def test_only_a_semantic_failure_satisfies_a_negative_example(
        self, outcome: str
    ) -> None:
        """An engine that cannot run an example has not reproduced its error."""
        report = ExampleReport(
            runtime_version="0.0.0",
            example=NEGATIVE,
            outcome=outcome,
        )

        verdict = compare_example(report, EXAMPLES / NEGATIVE)

        assert not verdict.passed
        assert kinds(verdict) == {"outcome"}

    def test_an_example_without_a_specification_is_refused(
        self, tmp_path: Path
    ) -> None:
        missing = tmp_path / "examples" / "no-such-example"
        missing.mkdir(parents=True)

        with pytest.raises(ConformanceError):
            run(missing, tmp_path)

    def test_an_engine_crash_becomes_a_visible_error_outcome(
        self,
        tmp_path: Path,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        from yamaa.adapters import conformance

        def explode(*args: object, **kwargs: object) -> None:
            raise MemoryError("out of memory")

        monkeypatch.setattr(conformance, "plan_workflow", explode)
        report = run(EXAMPLES / POSITIVE, tmp_path)

        assert report.outcome == "error"
        assert report.artifacts == ()
        assert not compare_example(report, EXAMPLES / POSITIVE).passed


class TestDocumentedCommand:
    """One command runs the covered pair and writes a report for each."""

    def test_the_documented_command_reports_and_passes_both(
        self,
        tmp_path: Path,
        capsys: pytest.CaptureFixture[str],
    ) -> None:
        code = main(
            [
                POSITIVE,
                NEGATIVE,
                "--run-dir",
                str(tmp_path / "run"),
                "--examples-root",
                str(EXAMPLES),
            ]
        )

        assert code == 0
        reports = sorted((tmp_path / "run" / "reports").glob("*.json"))
        assert [path.name for path in reports] == [
            f"{NEGATIVE}.python.json",
            f"{POSITIVE}.python.json",
        ]
        for path in reports:
            restored = ExampleReport.model_validate_json(
                path.read_text(encoding="utf-8")
            )
            assert restored.runtime == "python"
            assert restored.report_version == REPORT_VERSION
        assert "pass" in capsys.readouterr().out

    def test_the_command_publishes_artifacts_under_the_run_directory(
        self,
        tmp_path: Path,
        capsys: pytest.CaptureFixture[str],
    ) -> None:
        main(
            [
                POSITIVE,
                "--run-dir",
                str(tmp_path / "run"),
                "--examples-root",
                str(EXAMPLES),
            ]
        )
        capsys.readouterr()

        published = tmp_path / "run" / "artifacts" / POSITIVE / "dm.csv"
        assert (
            published.read_bytes()
            == (EXAMPLES / POSITIVE / "expected/dm.csv").read_bytes()
        )

    def test_the_command_fails_when_an_example_drifts(
        self,
        tmp_path: Path,
        capsys: pytest.CaptureFixture[str],
    ) -> None:
        example = copy_example(POSITIVE, tmp_path)
        golden = example / "expected/dm.csv"
        golden.write_bytes(golden.read_bytes().replace(b",34,", b",35,"))

        code = main(
            [
                POSITIVE,
                "--run-dir",
                str(tmp_path / "run"),
                "--examples-root",
                str(example.parent),
            ]
        )

        assert code == 1
        assert "FAIL" in capsys.readouterr().out

    def test_reports_can_be_written_without_judging_them(
        self,
        tmp_path: Path,
        capsys: pytest.CaptureFixture[str],
    ) -> None:
        code = main(
            [
                NEGATIVE,
                "--run-dir",
                str(tmp_path / "run"),
                "--examples-root",
                str(EXAMPLES),
                "--no-compare",
            ]
        )
        capsys.readouterr()

        assert code == 0
        assert (tmp_path / "run" / "reports" / f"{NEGATIVE}.python.json").is_file()

    def test_a_written_report_reads_back_as_the_report_it_was(
        self, tmp_path: Path
    ) -> None:
        """A runner collecting these files gets the observations back whole."""
        report = run(EXAMPLES / POSITIVE, tmp_path)

        path = write_report(report, tmp_path / "reports")
        restored = ExampleReport.model_validate_json(path.read_text(encoding="utf-8"))

        assert restored == report
        assert json.loads(path.read_text(encoding="utf-8"))["runtime"] == "python"

    def test_one_report_is_written_the_same_way_twice(self, tmp_path: Path) -> None:
        report = run(EXAMPLES / POSITIVE, tmp_path)

        first = write_report(report, tmp_path / "one").read_bytes()
        second = write_report(report, tmp_path / "two").read_bytes()

        assert first == second
