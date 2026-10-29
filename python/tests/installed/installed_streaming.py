"""Installed streamed runs (#3697): with stream=True a specification's run
produces its output as a reader pulls its Arrow stream, one batch per pull,
and its other members give its result once that stream has ended.

The truth is the shared `streaming` family, authored from the rules: the rows
and saved CSV of a study of every column type, the batches of a 70,000-record
study under the engine's 65,536-record batches, and the error texts of a
cancelled streamed run and of its refusals. A streamed output also equals the
whole-table output of the same specification. The end states are #3697's
mid-stream row: a reader released after its first batch, an interrupt before
the first pull or between batches, and a project function's interrupt each
cancel the run, which then has no result and publishes nothing. Through
another library's reader, the interrupt ends the stream with that reader's
error, prefixed as the Arrow crate's C stream export prefixes it.
"""

import _thread
import datetime as dt
import gc
import operator
import pickle
import signal
import tempfile
import threading
import unittest
from pathlib import Path
from unittest.mock import patch

import pyarrow as pa
from host_fixtures import function_study
from host_fixtures import load as load_truth

import yamaa
from yamaa import _locked_functions as host

TRUTH = load_truth(__file__, "streaming")
LARGE = TRUTH["large"]
FUNCTION_STUDY_CSV = load_truth(__file__, "interface")["lifetime"]["function_study_csv"]
# The Arrow crate's C stream export writes an external error with this prefix.
EXPORTED = "External error: "


def expected_rows():
    """The truth's rows as Python values: dates and datetimes as `datetime`
    values, numbers as floats in the float column."""

    def value(column, cell):
        if cell is None or column in ("ID", "NAME"):
            return cell
        if column == "D":
            return dt.date.fromisoformat(cell)
        if column == "T":
            return dt.datetime.fromisoformat(cell)
        return float(cell)

    return [
        {column: value(column, cell) for column, cell in row.items()}
        for row in TRUTH["rows"]
    ]


def interrupting(resolve):
    """A resolver whose functions raise the host's interrupt when called."""

    def resolved(*arguments):
        resolve(*arguments)

        def callback(**_):
            raise KeyboardInterrupt

        return callback

    return resolved


class Streaming(unittest.TestCase):
    def setUp(self):
        work = tempfile.TemporaryDirectory(prefix="yamaa-streaming-")
        self.addCleanup(work.cleanup)
        self.root = Path(work.name)

    def study(self):
        """A new directory holding the study of every column type."""
        root = self.root / "study"
        root.mkdir()
        (root / "spec.yaml").write_text(TRUTH["specification_yaml"], encoding="utf-8")
        (root / "input.csv").write_bytes(TRUTH["input_csv"].encode("utf-8"))
        return root

    def large(self):
        """A new directory holding the 70,000-record study."""
        root = self.root / "large"
        root.mkdir()
        (root / "spec.yaml").write_text(LARGE["specification_yaml"], encoding="utf-8")
        prefix = LARGE["name_prefix"]
        records = "".join(
            f"{record},{prefix}{record}\n" for record in range(1, LARGE["records"] + 1)
        )
        # Bytes, not text mode, which on Windows ends each record in U+000D
        # U+000A: a test compares this input with the saved CSV, whose records
        # end in U+000A (CSV-1).
        (root / "input.csv").write_bytes(("ID,NAME\n" + records).encode("utf-8"))
        return root

    def test_a_streamed_output_holds_the_expected_rows_and_saves_the_expected_file(
        self,
    ):
        root = self.study()
        spec = yamaa.spec(root / "spec.yaml")
        whole = spec.run().output.to_arrow()
        run = spec.run(stream=True)
        streamed = pa.RecordBatchReader.from_stream(run.output).read_all()
        saved = [Path(path).name for path in run.save()]
        self.assertEqual(
            (
                streamed.to_pylist(),
                streamed.equals(whole),
                run.status,
                saved,
                (root / "output.csv").read_bytes(),
            ),
            (
                expected_rows(),
                True,
                "succeeded",
                ["output.csv"],
                TRUTH["output_csv"].encode("utf-8"),
            ),
        )

    def test_a_large_output_streams_in_the_engine_s_batches(self):
        run = yamaa.spec(self.large() / "spec.yaml").run(stream=True)
        batches = list(pa.RecordBatchReader.from_stream(run.output))
        ids = [value for batch in batches for value in batch.column("ID").to_pylist()]
        self.assertEqual(
            (
                [batch.num_rows for batch in batches],
                ids == list(range(1, LARGE["records"] + 1)),
                run.status,
            ),
            (LARGE["batches"], True, "succeeded"),
        )

    def test_an_unread_run_runs_whole_when_its_result_is_asked_for(self):
        run = yamaa.spec(self.study() / "spec.yaml").run(stream=True)
        status = run.status
        output = run.output
        self.assertEqual(
            (status, len(output), output.to_pylist(), pa.table(output).to_pylist()),
            ("succeeded", 5, expected_rows(), expected_rows()),
        )

    def test_pickling_an_unread_output_materializes_it_and_keeps_its_types(self):
        # Python serialization must drive the same whole-read stream transition.
        run = yamaa.spec(self.study() / "spec.yaml").run(stream=True)
        output = run.output
        restored = pickle.loads(pickle.dumps(output))
        self.assertEqual(
            (restored.to_pylist(), restored.to_arrow().schema, run.status),
            (expected_rows(), output.to_arrow().schema, "succeeded"),
        )

    def test_a_streamed_output_is_read_once(self):
        run = yamaa.spec(self.study() / "spec.yaml").run(stream=True)
        output = run.output
        pa.table(output)
        refusals = []
        for read in (
            lambda: len(output),
            output.to_pylist,
            lambda: pickle.dumps(output),
        ):
            with self.assertRaises(yamaa.StateError) as raised:
                read()
            refusals.append(str(raised.exception))
        with self.assertRaises(pa.ArrowInvalid) as raised:
            pa.table(output)
        refusals.append(str(raised.exception))
        self.assertEqual(
            (refusals, run.status),
            (
                [
                    TRUTH["stream_taken"],
                    TRUTH["stream_taken"],
                    TRUTH["stream_taken"],
                    EXPORTED + TRUTH["stream_taken"],
                ],
                "succeeded",
            ),
        )

    def test_a_released_reader_cancels_the_run_and_publishes_nothing(self):
        root = self.large()
        run = yamaa.spec(root / "spec.yaml").run(stream=True)
        reader = pa.RecordBatchReader.from_stream(run.output)
        first = reader.read_next_batch().num_rows
        del reader
        gc.collect()
        refusals = []
        for read in (lambda: run.status, lambda: run.output, run.save):
            with self.assertRaises(yamaa.StateError) as raised:
                read()
            refusals.append(str(raised.exception))
        self.assertEqual(
            (first, refusals, (root / "output.csv").exists()),
            (LARGE["batches"][0], [TRUTH["cancelled"]] * 3, False),
        )

    def test_a_reader_released_after_the_last_batch_leaves_the_run_its_result(self):
        root = self.large()
        run = yamaa.spec(root / "spec.yaml").run(stream=True)
        reader = pa.RecordBatchReader.from_stream(run.output)
        rows = [reader.read_next_batch().num_rows for _ in LARGE["batches"]]
        del reader
        gc.collect()
        saved = [Path(path).name for path in run.save()]
        written = (root / "output.csv").read_bytes()
        # The specification copies both columns in input order, so its CSV
        # writes the input's text again (CSV-2, CSV-3, VALUE-8). The two
        # megabyte files are compared as one Boolean, as the large output's
        # IDs are: a failure that held them both would make unittest diff
        # them, which CPython before 3.14 does in cubic time.
        self.assertEqual(
            (rows, run.status, saved, written == (root / "input.csv").read_bytes()),
            (LARGE["batches"], "succeeded", ["output.csv"], True),
        )

    def test_an_interrupt_before_the_first_pull_or_between_batches_cancels_the_run(
        self,
    ):
        previous = signal.signal(signal.SIGINT, signal.default_int_handler)
        self.addCleanup(signal.signal, signal.SIGINT, previous)
        root = self.large()
        for label, before in (("before the first pull", 0), ("between batches", 1)):
            with self.subTest(case=label):
                run = yamaa.spec(root / "spec.yaml").run(stream=True)
                reader = pa.RecordBatchReader.from_stream(run.output)
                for _ in range(before):
                    reader.read_next_batch()
                with self.assertRaises(pa.ArrowInvalid) as raised:
                    # interrupt_main trips SIGINT without running its
                    # handler; the pull, called from C with no Python frame
                    # between, polls the signal before it builds anything.
                    list(
                        map(
                            operator.call,
                            [_thread.interrupt_main, reader.read_next_batch],
                        )
                    )
                with self.assertRaises(yamaa.StateError) as refused:
                    run.status
                self.assertEqual(
                    (
                        str(raised.exception),
                        str(refused.exception),
                        (root / "output.csv").exists(),
                    ),
                    (EXPORTED + TRUTH["cancelled"], TRUTH["cancelled"], False),
                )

    def test_a_project_function_interrupt_cancels_the_stream(self):
        case = function_study(__file__, self.root / "functions")
        run = yamaa.project(case / "project.yaml").spec("TEST").run(stream=True)
        reader = pa.RecordBatchReader.from_stream(run.output)
        with (
            patch.object(
                host,
                "resolve_callable",
                side_effect=interrupting(host.resolve_callable),
            ),
            self.assertRaises(pa.ArrowInvalid) as raised,
        ):
            reader.read_next_batch()
        with self.assertRaises(yamaa.StateError) as refused:
            run.status
        self.assertEqual(
            (
                str(raised.exception),
                str(refused.exception),
                (case / "test.csv").exists(),
            ),
            (EXPORTED + TRUTH["cancelled"], TRUTH["cancelled"], False),
        )

    def test_a_run_that_calls_python_is_read_only_on_its_own_thread(self):
        case = function_study(__file__, self.root / "functions")
        run = yamaa.project(case / "project.yaml").spec("TEST").run(stream=True)
        reader = pa.RecordBatchReader.from_stream(run.output)
        refusals = []

        def read():
            try:
                reader.read_next_batch()
            except pa.ArrowInvalid as error:
                refusals.append(str(error))

        thread = threading.Thread(target=read)
        thread.start()
        thread.join()
        rows = pa.RecordBatchReader.from_stream(run.output).read_all().num_rows
        run.save()
        self.assertEqual(
            (refusals, rows, (case / "test.csv").read_text(encoding="utf-8")),
            ([EXPORTED + TRUTH["host_thread"]], 1, FUNCTION_STUDY_CSV),
        )


if __name__ == "__main__":
    unittest.main()
