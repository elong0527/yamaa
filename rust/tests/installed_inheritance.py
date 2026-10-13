"""Installed typed inheritance ports preserve originals, effects and isolation."""

import json
import os
import platform
import threading
import unittest
from pathlib import Path

from yamaa import _native as yamaa_native

ROOT = Path(__file__).with_name("specification-original")


def modules():
    root = ROOT / "schema"
    names = ["schema.yaml"] + sorted(p.name for p in root.glob("*.yaml") if p.name != "schema.yaml")
    return [(name, (root / name).read_bytes()) for name in names]


class Boundary(unittest.TestCase):
    def setUp(self):
        self.assertIn("site-packages", str(Path(yamaa_native.__file__).resolve()))
        self.case = ROOT / "cases" / "schema-inheritance"
        self.path = self.case / "spec_study.yaml"
        self.entry = str(self.path.resolve())
        self.source = self.path.read_bytes()

    def prepare(self, explicit, canonicalize, capture, rebase, source=None):
        arguments = (self.entry, self.source if source is None else source, canonicalize, capture, rebase)
        if explicit:
            return yamaa_native._prepare_inherited_specification(modules(), 0, *arguments)
        return yamaa_native._prepare_document(*arguments)

    def ports(self, effect=None):
        calls = []

        def record(operation, arguments):
            calls.append((operation, arguments))
            if effect is not None:
                effect(operation, arguments)

        def canonicalize(declaring, written):
            record("canonicalize", (declaring, written))
            candidate = Path(declaring).parent / written
            return (str(candidate.resolve()), str(candidate)) if candidate.is_file() else None

        def capture(identity, display_path, maximum):
            record("capture", (identity, display_path, maximum))
            with Path(identity).open("rb") as stream:
                content = stream.read(maximum + 1)
            self.assertLessEqual(len(content), maximum)
            return content

        def rebase(layer, entry, written, maximum):
            record("rebase", (layer, entry, written, maximum))
            if Path(written).is_absolute() or Path(layer).parent == Path(entry).parent:
                return written
            return Path(os.path.relpath(os.path.normpath(os.path.join(Path(layer).parent, written)), Path(entry).parent)).as_posix()

        return (canonicalize, capture, rebase), calls

    def assert_original(self, specification):
        self.assertEqual(specification.source(), ("LB", "input/lb.csv"))
        reads, publications = [], []

        def capture(dataset, path, maximum):
            reads.append((dataset, path))
            content = (self.case / path).read_bytes()
            self.assertLessEqual(len(content), maximum)
            return content, True

        def publish(path, content):
            publications.append(path)
            self.assertEqual(path, "adlb.csv")
            self.assertEqual(content, (self.case / "expected/adlb.csv").read_bytes())

        metadata = (platform.python_version(), yamaa_native._CORE_VERSION, "schema-inheritance", "spec_study.yaml", ".")
        expected = json.loads((ROOT / "expected/schema-inheritance.json").read_text())
        expected.update(runtime="python", runtime_version=metadata[0], engine_version=metadata[1])
        self.assertEqual(json.loads(specification.report(capture, publish, metadata)), expected)
        self.assertEqual(publications, ["adlb.csv"])
        self.assertEqual(reads, [("LB", "input/lb.csv")])

    def test_retired_callbacks_and_schema_probes_are_absent(self):
        for name in ("traverse_inheritance", "_compile_schema", "interpret_schema", "decode_yaml"):
            self.assertFalse(hasattr(yamaa_native, name), name)

    def test_complete_original_reports_and_typed_source_effects(self):
        for explicit in (False, True):
            with self.subTest(explicit=explicit):
                ports, calls = self.ports()
                specification = self.prepare(explicit, *ports)
                self.assert_original(specification)
                self.assertEqual([Path(args[1]).name for op, args in calls if op == "canonicalize"], ["spec_organization.yaml", "spec_compound.yaml", "spec_organization.yaml"])
                self.assertEqual([Path(args[0]).name for op, args in calls if op == "capture"], ["spec_organization.yaml", "spec_compound.yaml"])
                remaining = 16_777_216 - len(self.source)
                for op, args in calls:
                    if op == "capture":
                        self.assertEqual(args[2], remaining)
                        remaining -= Path(args[0]).stat().st_size

    def test_original_exceptions_and_interrupts_stop_at_each_typed_port(self):
        for explicit in (False, True):
            for operation in ("canonicalize", "capture", "rebase"):
                for failure in (ValueError("original"), KeyboardInterrupt("original"), SystemExit(17)):
                    with self.subTest(explicit=explicit, operation=operation, failure=type(failure)):
                        def effect(op, arguments):
                            if op == operation:
                                raise failure
                        ports, calls = self.ports(effect)
                        with self.assertRaises(type(failure)) as caught:
                            self.prepare(explicit, *ports)
                        self.assertIs(caught.exception, failure)
                        self.assertEqual(calls[-1][0], operation)
                        self.assertEqual(sum(op == operation for op, _ in calls), 1)

    def test_same_thread_reentrant_typed_attempts_have_independent_state(self):
        caller = threading.get_ident()
        nested = []
        source = b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns: [{name: ID, type: str, derivation: SRC.ID}]\n"
        def effect(operation, arguments):
            self.assertEqual(threading.get_ident(), caller)
            def no_io(*_):
                self.fail("nested source authority")
            nested.append(yamaa_native._prepare_document("nested.yaml", source, no_io, no_io, no_io))
            self.assertEqual(nested[-1].source(), ("SRC", "input.csv"))
        for explicit in (False, True):
            ports, calls = self.ports(effect)
            specification = self.prepare(explicit, *ports)
            self.assert_original(specification)
            self.assertEqual(len(nested), len(calls) if not explicit else 2 * len(calls))

    def test_typed_shape_and_capture_limits_do_not_poison_later_attempts(self):
        for explicit in (False, True):
            for operation, reply, exception, reason in (
                ("canonicalize", 17, TypeError, "tuple"),
                ("capture", "not bytes", TypeError, "bytes"),
                ("capture", lambda maximum: b" " * (maximum + 1), ValueError, "capture limit"),
                ("rebase", 17, TypeError, "str"),
            ):
                with self.subTest(explicit=explicit, operation=operation, reason=reason):
                    ports, calls = self.ports()
                    index = ("canonicalize", "capture", "rebase").index(operation)
                    def invalid(*arguments):
                        calls.append((operation, arguments))
                        return reply(arguments[-1]) if callable(reply) else reply
                    ports = list(ports)
                    ports[index] = invalid
                    with self.assertRaisesRegex(exception, reason):
                        self.prepare(explicit, *ports)
                    self.assertEqual(calls[-1][0], operation)
                    healthy, _ = self.ports()
                    self.assert_original(self.prepare(explicit, *healthy))


if __name__ == "__main__":
    unittest.main()
