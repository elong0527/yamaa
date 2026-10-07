"""Installed original-YAML execution with reference semantic imports forbidden."""

import builtins
import json
import os
import platform
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import yamaa_native

ROOT = Path(__file__).with_name("specification-original")
CASES = ("negative-zero-division", "negative-integer-overflow", "adam-adlb-ordered-sum")


def modules():
    names = ["schema.yaml"] + sorted(
        p.name for p in (ROOT / "schema").glob("*.yaml") if p.name != "schema.yaml"
    )
    return [(name, (ROOT / "schema" / name).read_bytes()) for name in names]


def prepare(name):
    return yamaa_native._prepare_specification(
        modules(), 0, "spec.yaml", (ROOT / "cases" / name / "spec.yaml").read_bytes()
    )


class OriginalSpecifications(unittest.TestCase):
    def setUp(self):
        original = builtins.__import__

        def reject(name, *args, **kwargs):
            if name.split(".")[0] in {"yamaa", "pydantic", "yaml", "yaml12", "pyarrow"}:
                raise AssertionError("reference semantic import: " + name)
            return original(name, *args, **kwargs)

        guard = patch("builtins.__import__", side_effect=reject)
        guard.start()
        self.addCleanup(guard.stop)

    def test_complete_original_reports_and_cached_capture(self):
        for name in CASES:
            with self.subTest(name=name):
                specification = prepare(name)
                self.assertEqual(specification.source(), ("LB", "input/lb.csv"))
                state = {"requests": 0, "reads": 0, "bytes": None}

                def capture(dataset, path, maximum, *, state=state, case_name=name):
                    self.assertEqual((dataset, path), ("LB", "input/lb.csv"))
                    state["requests"] += 1
                    created = state["bytes"] is None
                    if created:
                        with (ROOT / "cases" / case_name / path).open("rb") as stream:
                            state["bytes"] = stream.read(maximum + 1)
                        self.assertLessEqual(len(state["bytes"]), maximum)
                        state["reads"] += 1
                    return state["bytes"], created

                metadata = (
                    platform.python_version(),
                    yamaa_native.engine_info()["core_version"],
                    name,
                    "spec.yaml",
                    ".",
                )
                expected = json.loads(
                    (ROOT / "expected" / (name + ".json")).read_text()
                )
                expected.update(
                    runtime="python",
                    runtime_version=metadata[0],
                    engine_version=metadata[1],
                )
                published = []
                with tempfile.TemporaryDirectory() as directory:

                    def publish(path, content, *, name=name, published=published):
                        self.assertEqual(name, "adam-adlb-ordered-sum")
                        self.assertEqual(path, "adlb.csv")
                        self.assertEqual(
                            content,
                            (ROOT / "cases" / name / "expected" / path).read_bytes(),
                        )
                        pending = Path(directory) / "candidate.csv"
                        pending.write_bytes(content)
                        os.replace(pending, Path(directory) / path)
                        published.append((Path(directory) / path).read_bytes())

                    for created in (1, 0):
                        expected["source_reads"][0]["snapshots_created"] = created
                        actual = json.loads(
                            specification.report(capture, publish, metadata)
                        )
                        self.assertEqual(actual, expected)
                self.assertEqual(
                    len(published), 2 if name == "adam-adlb-ordered-sum" else 0
                )
                self.assertEqual((state["requests"], state["reads"]), (2, 1))

    def test_original_host_errors_and_interruptions_survive_native_return(self):
        specification = prepare(CASES[0])
        metadata = (
            platform.python_version(),
            yamaa_native.engine_info()["core_version"],
            CASES[0],
            "spec.yaml",
            ".",
        )
        for failure in (
            OSError("retained source failure"),
            KeyboardInterrupt("retained interrupt"),
        ):
            calls = []

            def capture(*args, calls=calls, failure=failure):
                calls.append(args)
                raise failure

            with self.assertRaises(type(failure)) as raised:
                specification.failure_report(capture, metadata)
            self.assertIs(raised.exception, failure)
            self.assertEqual(len(calls), 1)

    def test_publication_retains_original_errors_and_interruptions(self):
        name = "adam-adlb-ordered-sum"
        specification = prepare(name)
        metadata = (
            platform.python_version(),
            yamaa_native.engine_info()["core_version"],
            name,
            "spec.yaml",
            ".",
        )
        content = (ROOT / "cases" / name / "input/lb.csv").read_bytes()
        for failure in (
            OSError("retained publication failure"),
            KeyboardInterrupt("retained publication interrupt"),
        ):
            calls = []

            def publish(*args, calls=calls, failure=failure):
                calls.append(args)
                raise failure

            with self.assertRaises(type(failure)) as raised:
                specification.report(lambda *_: (content, True), publish, metadata)
            self.assertIs(raised.exception, failure)
            self.assertEqual(len(calls), 1)

    def test_unsupported_preparation_has_no_source_effect(self):
        source = (ROOT / "cases" / CASES[0] / "spec.yaml").read_bytes()
        source = source.replace(b"100 * (AVAL - BASE) / BASE", b"LN(AVAL)")
        with self.assertRaises(ValueError) as raised:
            yamaa_native._prepare_specification(modules(), 0, "spec.yaml", source)
        self.assertEqual(
            json.loads(str(raised.exception))["outcome"]["status"], "unsupported"
        )


if __name__ == "__main__":
    unittest.main()
