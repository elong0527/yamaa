"""Installed source-port identity, exception, thread and reentrancy contracts."""

import csv
import json
import threading
import unittest
from pathlib import Path

from yamaa import _native as yamaa_native
from yamaa.adapters._native_schema_wire import encode_tree

SCHEMA = json.dumps(
    {
        "protocol": "schema/1",
        "schema": {
            "entry": 0,
            "root_class": "root_class",
            "modules": [
                {
                    "name": "schema.yaml",
                    "document": encode_tree(
                        {
                            "version": "1.0",
                            "root_class": [
                                {"schema_version": {"type": "str"}},
                                {"parents": {"type": ["str", "list[str]"]}},
                            ],
                        }
                    ),
                }
            ],
        },
    }
)


def request(parents):
    return json.dumps(
        {
            "protocol": "inheritance/1",
            "entry": {"identity": "/entry", "display_path": "/entry"},
            "document": encode_tree({"schema_version": "1.0", "parents": parents}),
        }
    )


def reply(outcome):
    return json.dumps({"protocol": "inheritance/1", "outcome": outcome})


class Boundary(unittest.TestCase):
    def setUp(self):
        self.assertIn("site-packages", str(Path(yamaa_native.__file__).resolve()))
        self.snapshot, result = yamaa_native._compile_schema(SCHEMA)
        self.assertEqual(json.loads(result)["outcome"]["status"], "compiled")

    def test_complete_independent_graph_outcomes_and_source_traces(self):
        root = Path(__file__).parent
        with (root / "inheritance_traversal.tsv").open(encoding="ascii") as stream:
            cases = list(csv.DictReader(stream, delimiter="\t"))
        with (root / "inheritance_sources.tsv").open(encoding="ascii") as stream:
            sources = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(cases), 8)
        for case in cases:
            for prepared in (False, True):
                with self.subTest(case=case["case"], prepared=prepared):
                    expected = [s for s in sources if s["case"] == case["case"]]
                    trace = iter(expected)
                    calls = []

                    def callback(message, maximum, trace=trace, calls=calls):
                        event = next(trace)
                        self.assertEqual(message, event["request"])
                        calls.append(message)
                        self.assertLessEqual(len(event["reply"].encode()), maximum)
                        return event["reply"]

                    if prepared:
                        snapshot, _ = yamaa_native._compile_schema(case["schema"])
                        result = snapshot.traverse_inheritance(
                            case["request"], callback
                        )
                    else:
                        result = yamaa_native.traverse_inheritance(
                            case["schema"], case["request"], callback
                        )
                    self.assertEqual(result, case["expected"])
                    self.assertEqual(calls, [s["request"] for s in expected])

    def test_original_host_exceptions_and_control_flow_survive(self):
        for prepared in (False, True):
            for failure in (
                ValueError("original"),
                KeyboardInterrupt("original"),
                SystemExit(17),
            ):
                with self.subTest(prepared=prepared, failure=type(failure)):

                    def callback(*_, failure=failure):
                        raise failure

                    with self.assertRaises(type(failure)) as caught:
                        if prepared:
                            self.snapshot.traverse_inheritance(request(["a"]), callback)
                        else:
                            yamaa_native.traverse_inheritance(
                                SCHEMA, request(["a"]), callback
                            )
                    self.assertIs(caught.exception, failure)

    def test_same_thread_reentrant_snapshot_has_isolated_graph_state(self):
        caller = threading.get_ident()
        calls = []

        def callback(message, maximum):
            self.assertEqual(threading.get_ident(), caller)
            message = json.loads(message)
            calls.append(message["operation"])
            nested = self.snapshot.traverse_inheritance(
                request([]), lambda *_: self.fail("nested IO")
            )
            self.assertEqual(json.loads(nested)["outcome"]["status"], "traversed")
            if message["operation"] == "canonicalize":
                return reply(
                    {"status": "resolved", "identity": "/a", "display_path": "/a"}
                )
            return reply(
                {
                    "status": "document",
                    "document": encode_tree({"schema_version": "1.0"}),
                }
            )

        result = self.snapshot.traverse_inheritance(request(["a"]), callback)
        self.assertEqual(calls, ["canonicalize", "read"])
        self.assertEqual(
            [x["identity"] for x in json.loads(result)["outcome"]["layers"]],
            ["/a", "/entry"],
        )

    def test_reply_limit_and_wrong_return_do_not_poison_snapshot(self):
        with self.assertRaisesRegex(ValueError, "reply exceeds byte limit"):
            self.snapshot.traverse_inheritance(
                request(["a"]), lambda _, maximum: " " * (maximum + 1)
            )
        with self.assertRaisesRegex(TypeError, "return JSON text"):
            self.snapshot.traverse_inheritance(request(["a"]), lambda *_: 17)
        result = self.snapshot.traverse_inheritance(
            request([]), lambda *_: self.fail("unexpected IO")
        )
        self.assertEqual(json.loads(result)["outcome"]["status"], "traversed")


if __name__ == "__main__":
    unittest.main()
