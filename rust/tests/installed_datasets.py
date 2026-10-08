"""Installed dataset/1 execution over independent Arrow and observation truth."""

import csv
import gc
import json
import unittest
from pathlib import Path

from yamaa import _native as yamaa_native

ROOT = Path(__file__).parent / "datasets"


class InstalledDatasets(unittest.TestCase):
    """Exercise copied native buffers and failed outcomes outside the checkout."""

    def test_shared_dataset_cases(self):
        """Both installed archive forms must replay all independent native outcomes."""
        self.assertIn("site-packages", str(Path(yamaa_native.__file__).resolve()))
        cases = json.loads((ROOT / "expected.json").read_text())
        with (ROOT / "expected.tsv").open() as stream:
            tabular = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(cases), len(tabular))
        for case, tab in zip(cases, tabular, strict=True):
            with self.subTest(case=case["case"]):
                self.assertEqual(json.loads(tab["request"]), case["request"])
                self.assertEqual(json.loads(tab["expected"]), case["expected"])
                self.assertEqual(
                    json.loads(tab["snapshot"]) if tab["snapshot"] else None,
                    case["snapshot"],
                )
                names = (
                    tab.get("secondary", "").split(";")
                    if tab.get("secondary", "-") != "-"
                    else []
                )
                self.assertEqual(names, case.get("secondary", []))
                secondary = [(ROOT / name).read_bytes() for name in names]
                source = (ROOT / case["input"]).read_bytes()
                table, outcome = (
                    yamaa_native.execute_dataset_sources(
                        json.dumps(case["request"]), source, secondary
                    )
                    if secondary
                    else yamaa_native.execute_dataset(
                        json.dumps(case["request"]), source
                    )
                )
                del source, secondary
                gc.collect()
                self.assertEqual(json.loads(outcome), case["expected"])
                if case["snapshot"] is None:
                    self.assertIsNone(table)
                else:
                    self.assertIs(type(table), bytes)
                    self.assertEqual(
                        json.loads(yamaa_native.table_snapshot(table)), case["snapshot"]
                    )
                    self.assertEqual(yamaa_native.table_round_trip(table), table)
        self.assertFalse(yamaa_native.engine_info()["execution_supported"])

    def test_secondary_source_input_shape_and_recovery(self):
        """Secondary buffers are bounded bytes lists and cannot be silently omitted."""
        case = next(
            case
            for case in json.loads((ROOT / "expected.json").read_text())
            if case["case"] == "lookup_values"
        )
        request = json.dumps(case["request"])
        source = (ROOT / case["input"]).read_bytes()
        secondary = [(ROOT / name).read_bytes() for name in case["secondary"]]
        with self.assertRaises(ValueError):
            yamaa_native.execute_dataset(request, source)
        for invalid in [(), ["bytes"], [None]]:
            with self.subTest(invalid=invalid), self.assertRaises(TypeError):
                yamaa_native.execute_dataset_sources(request, source, invalid)
        with self.assertRaisesRegex(ValueError, "too many"):
            yamaa_native.execute_dataset_sources(request, source, [b""] * 8)
        table, outcome = yamaa_native.execute_dataset_sources(
            request, source, secondary
        )
        del source, secondary
        gc.collect()
        self.assertEqual(json.loads(outcome), case["expected"])
        self.assertEqual(
            json.loads(yamaa_native.table_snapshot(table)), case["snapshot"]
        )

    def test_admission_precedes_ipc_and_recovers(self):
        """Invalid typed requests cannot decode bad IPC or poison subsequent execution."""
        request = json.loads((ROOT / "adlb-plan.json").read_text())
        request["columns"][0]["expression"] = {"source": 999}
        with self.assertRaisesRegex(ValueError, "invalid bound dataset plan"):
            yamaa_native.execute_dataset(json.dumps(request), b"invalid IPC")
        with self.assertRaisesRegex(ValueError, "resource limit"):
            yamaa_native.execute_dataset(" " * 1048577, b"")
        with self.assertRaises(TypeError):
            yamaa_native.execute_dataset({}, b"")
        request = (ROOT / "adlb-plan.json").read_text()
        table, outcome = yamaa_native.execute_dataset(
            request, (ROOT / "adlb.arrow").read_bytes()
        )
        self.assertIs(type(table), bytes)
        self.assertEqual(json.loads(outcome)["outcome"]["status"], "success")


class InstalledDatasetCallbacks(unittest.TestCase):
    """Compare explicit dataset callback effects and complete outcomes to authored truth."""

    def test_shared_callback_cases(self):
        """Installed hosts share expected tables, failures, argument order and effects."""
        cases = json.loads((ROOT / "callbacks.json").read_text())
        with (ROOT / "callbacks.tsv").open() as stream:
            tabular = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(cases), len(tabular))
        for case, tab in zip(cases, tabular, strict=True):
            with self.subTest(case=case["case"]):
                for field in ("request", "expected", "snapshot"):
                    self.assertEqual(json.loads(tab[field]), case[field])
                for _ in range(2):
                    trace = []

                    def make_callback(slot, mode=case["mode"], trace=trace):
                        """Snapshot one slot while leaving its host effects observable."""

                        def callback(**arguments):
                            self.assertEqual(
                                list(arguments),
                                ["lhs", "rhs", "scale"]
                                if len(arguments) == 3
                                else ["value"],
                            )
                            values = list(arguments.values())
                            trace.append(f"{slot}:" + ",".join(map(str, values)))
                            if mode == "raise_second" and len(trace) == 2:

                                class FixtureError(Exception):
                                    """Portable independent failure class for both hosts."""

                                raise FixtureError("after first effect")
                            if mode == "boolean":
                                return True
                            if mode == "bad_text":
                                return "bad"
                            return values[0] if len(values) == 1 else sum(values)

                        return callback

                    callbacks = [
                        make_callback(slot)
                        for slot in range(len(case["request"]["functions"]))
                    ]
                    table, outcome = yamaa_native.execute_dataset_functions(
                        json.dumps(case["request"]),
                        (ROOT / case["input"]).read_bytes(),
                        [],
                        callbacks,
                    )
                    self.assertEqual(json.loads(outcome), case["expected"])
                    self.assertEqual(";".join(trace), case["trace"])
                    if case["snapshot"] is None:
                        self.assertIsNone(table)
                    else:
                        self.assertEqual(
                            json.loads(yamaa_native.table_snapshot(table)),
                            case["snapshot"],
                        )

    def test_binding_admission_interruptions_and_reuse(self):
        """Bad authority precedes IPC and original control signals survive native return."""
        case = json.loads((ROOT / "callbacks.json").read_text())[0]
        request = json.dumps(case["request"])
        source = (ROOT / case["input"]).read_bytes()
        for callbacks in ([], [None], [lambda **kw: 1, lambda **kw: 1]):
            with self.assertRaises((ValueError, TypeError)):
                yamaa_native.execute_dataset_functions(
                    request, b"bad IPC", [], callbacks
                )
        with self.assertRaisesRegex(ValueError, "bindings"):
            yamaa_native.execute_dataset(request, b"bad IPC")
        invalid_name = json.loads(request)
        invalid_name["functions"][0]["parameters"][0]["host_name"] = "if"
        with self.assertRaisesRegex(ValueError, "host"):
            yamaa_native.execute_dataset_functions(
                json.dumps(invalid_name), b"bad IPC", [], [lambda **kw: 1]
            )
        calls = []
        interrupted = KeyboardInterrupt("stop this dataset")

        def stop(**arguments):
            """One host effect precedes a non-normal exception."""
            calls.append(arguments)
            raise interrupted

        with self.assertRaises(KeyboardInterrupt) as raised:
            yamaa_native.execute_dataset_functions(request, source, [], [stop])
        self.assertIs(raised.exception, interrupted)
        self.assertEqual(len(calls), 1)
        table, outcome = yamaa_native.execute_dataset_functions(
            request, source, [], [lambda **kw: sum(kw.values())]
        )
        self.assertEqual(json.loads(outcome), case["expected"])
        self.assertEqual(
            json.loads(yamaa_native.table_snapshot(table)), case["snapshot"]
        )

    def test_scalar_precision_and_bits_at_dataset_callback_boundary(self):
        """Typed literals preserve float bits and lose temporal precision only at encoding."""
        import copy
        import datetime as dt
        import struct

        original = json.loads((ROOT / "callbacks.json").read_text())[0]
        cases = [
            ("float", {"float": "8000000000000000"}, {"float": "8000000000000000"}),
            (
                "date",
                {"date": {"text": "2024-05-01", "precision": "month"}},
                {"date": {"text": "2024-05-01", "precision": "day"}},
            ),
            (
                "datetime",
                {"datetime": {"text": "2024-05-01T00:00:00", "precision": "day"}},
                {"datetime": {"text": "2024-05-01T00:00:00", "precision": "second"}},
            ),
        ]
        for kind, literal, expected in cases:
            with self.subTest(kind=kind):
                request = copy.deepcopy(original["request"])
                signature = request["functions"][0]
                signature["parameters"] = [
                    {
                        "name": "value",
                        "host_name": "value",
                        "type": kind,
                        "accepts_missing": False,
                        "presence": {"required": None},
                    }
                ]
                signature["returns"] = kind
                request["output"][1]["kind"] = kind
                request["columns"][0]["expression"]["function"]["arguments"] = [
                    {"name": "value", "input": {"literal": literal}}
                ]
                seen = []

                def identity(value, seen=seen):
                    """Return the exact host scalar received by this independent callback."""
                    seen.append(value)
                    return value

                table, outcome = yamaa_native.execute_dataset_functions(
                    json.dumps(request),
                    (ROOT / original["input"]).read_bytes(),
                    [],
                    [identity],
                )
                self.assertEqual(json.loads(outcome), original["expected"])
                rows = json.loads(yamaa_native.table_snapshot(table))["rows"]
                self.assertEqual([row[1] for row in rows], [expected] * 3)
                self.assertEqual(len(seen), 3)
                if kind == "float":
                    self.assertEqual(
                        struct.pack(">d", seen[0]).hex(), "8000000000000000"
                    )
                elif kind == "date":
                    self.assertIs(type(seen[0]), dt.date)
                    self.assertEqual(seen[0], dt.date(2024, 5, 1))
                else:
                    self.assertIs(type(seen[0]), dt.datetime)
                    self.assertIsNone(seen[0].tzinfo)
                    self.assertEqual(seen[0].isoformat(), "2024-05-01T00:00:00")

    def test_callbacks_are_snapshotted_before_effects(self):
        """Mutation of the caller's list cannot replace a later bound callable."""
        case = next(
            c
            for c in json.loads((ROOT / "callbacks.json").read_text())
            if c["case"] == "dependent_column"
        )
        callbacks = []
        called = []

        def first(**values):
            """Replace the external collection after the native registry was captured."""
            callbacks.clear()
            return sum(values.values())

        def second(value):
            """Retain the original later-column binding throughout this run."""
            called.append(value)
            return value

        callbacks.extend([first, second])
        table, outcome = yamaa_native.execute_dataset_functions(
            json.dumps(case["request"]),
            (ROOT / case["input"]).read_bytes(),
            [],
            callbacks,
        )
        self.assertEqual(called, [35, 95])
        self.assertEqual(json.loads(outcome), case["expected"])
        self.assertEqual(
            json.loads(yamaa_native.table_snapshot(table)), case["snapshot"]
        )


if __name__ == "__main__":
    unittest.main()
