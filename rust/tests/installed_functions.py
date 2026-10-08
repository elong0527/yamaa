"""Installed native callback contracts; run from outside the source checkout."""

import csv
import datetime as dt
import gc
import json
import math
import struct
import threading
import unittest
from pathlib import Path

from yamaa import _native as yamaa_native


def scalar(token):
    """Decode fixture input spelling into the independent scalar wire vocabulary."""
    if token == "missing":
        return {"missing": None}
    kind, text = token.split(":", 1)
    if kind == "float":
        return {kind: struct.pack(">d", float(text)).hex()}
    if kind == "bool":
        return {kind: text == "true"}
    if kind in ("date", "datetime"):
        return {
            kind: {"text": text, "precision": "day" if kind == "date" else "second"}
        }
    return {kind: text}


def request(parameters=None, arguments=None, returns="int", may_missing=False):
    """Build an explicit capability request with caller-supplied identity labels."""
    return {
        "protocol": "function/1",
        "identity": {
            "name": "example",
            "contract_version": "1",
            "implementation_version": "2",
            "call": "artifact.example",
        },
        "parameters": parameters or [],
        "arguments": arguments or [],
        "returns": returns,
        "may_return_missing": may_missing,
    }


def parameter(kind="int", name="x", accepts_missing=False):
    """Declare one required parameter with an explicit, different host name."""
    return {
        "name": name,
        "host_name": "host_" + name,
        "type": kind,
        "accepts_missing": accepts_missing,
        "presence": {"required": None},
    }


def invoke(req, callback):
    """Call the installed extension and decode its owned JSON outcome."""
    return json.loads(yamaa_native.invoke_function(json.dumps(req), callback))[
        "outcome"
    ]


def encode_host(value):
    """Observe exact callback types and bits without reference-engine imports."""
    if value is None:
        return "missing"
    if type(value) is bool:
        return "bool:" + str(value).lower()
    if type(value) is int:
        return f"int:{value}"
    if type(value) is float:
        return "float:" + struct.pack(">d", value).hex()
    if isinstance(value, str):
        return "str:" + value
    if type(value) is dt.datetime:
        return "datetime:" + value.isoformat(timespec="seconds")
    if type(value) is dt.date:
        return "date:" + value.isoformat()
    raise AssertionError(type(value))


def host_value(token):
    """Produce a real host result, leaving normalization to the native adapter."""
    if token == "missing":
        return None
    kind, text = token.split(":", 1)
    if kind == "int":
        return int(text)
    if kind == "float":
        return float(text)
    if kind == "bool":
        return text == "true"
    if kind == "date":
        return dt.date.fromisoformat(text)
    if kind == "datetime":
        return dt.datetime.fromisoformat(text)
    return text


def encode_outcome(test, outcome):
    """Assert failure identity/metadata, then compare independent compact truth."""
    if outcome["status"] == "value":
        kind, value = next(iter(outcome["value"].items()))
        if kind == "missing":
            test.assertIsNone(value)
            return "missing"
        if kind in ("date", "datetime"):
            test.assertEqual(value["precision"], "day" if kind == "date" else "second")
            value = value["text"]
        return f"{kind}:{value}"
    diagnostic = outcome["diagnostic"]
    context = diagnostic["context"]
    test.assertEqual(diagnostic["phase"], "derivation")
    test.assertIsNone(diagnostic["applicable_handler"])
    for key, value in [
        ("function", "example"),
        ("contract_version", "1"),
        ("implementation_version", "2"),
    ]:
        test.assertEqual(context[key], value)
    condition = diagnostic["condition"]
    if condition == "invalid_function_argument":
        test.assertEqual(diagnostic["requirement"], "REQ-0700")
        if "unknown" in context:
            return "error:unknown:" + ",".join(context["unknown"])
        if "expected" in context:
            return f"error:argument:{context['parameter']}:{context['expected']}:{context['actual']}"
        test.assertEqual(context["reason"], "a required argument was not supplied")
        return "error:missing:" + context["parameter"]
    if condition == "function_call_failed":
        test.assertEqual(diagnostic["requirement"], "REQ-0701")
        test.assertEqual(context["call"], "artifact.example")
        return f"error:raised:{context['host_error']}:{context['host_message']}"
    test.assertEqual(condition, "invalid_function_result")
    test.assertEqual(diagnostic["requirement"], "REQ-0702")
    if "expected" in context:
        return f"error:result:{context['expected']}:{context['actual']}"
    reason = context["reason"]
    if reason == "a binding returned a Boolean":
        test.assertEqual(context["returned"], "bool")
        return "error:boolean"
    if reason == "an invoked binding returned an undeclared missing":
        return "error:undeclared-missing"
    if reason == "a returned integer exceeds 64 bits":
        return "error:invalid:bigint"
    test.assertEqual(reason, "a binding returned a value of no scalar type")
    return "error:invalid:" + context["returned"]


class InstalledFunctionTests(unittest.TestCase):
    """Exercise actual native calls, not a JSON echo or an editable reference."""

    def test_shared_independent_truth(self):
        """All 42 reference/Rust expectations also pass through real Python calls."""
        self.assertIn("site-packages", str(Path(yamaa_native.__file__).resolve()))
        with (
            Path(__file__)
            .with_name("function_invocation.tsv")
            .open(encoding="utf-8", newline="") as stream
        ):
            cases = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(cases), 42)
        for case in cases:
            with self.subTest(case=case["id"]):
                params = []
                if case["parameters"] != "-":
                    for item in case["parameters"].split(";"):
                        name, kind, missing, default, host = item.split("/")
                        p = parameter(kind, name, missing == "true")
                        p["host_name"] = host
                        p["presence"] = (
                            {"required": None}
                            if default == "required"
                            else {"optional": scalar(default)}
                        )
                        params.append(p)
                args = []
                if case["arguments"] != "-":
                    for item in case["arguments"].split(";"):
                        name, token = item.split("=", 1)
                        args.append({"name": name, "value": scalar(token)})
                trace = []
                calls = []

                def callback(_calls=calls, _trace=trace, _case=case, **kwargs):
                    """Execute the authored callback action once and retain its trace."""
                    _calls.append(1)
                    _trace.extend(f"{k}={encode_host(v)}" for k, v in kwargs.items())
                    if not kwargs:
                        _trace.append("called")
                    action = _case["callback"]
                    if action == "raise":
                        raise ValueError("boom")
                    if action == "invalid:list":
                        return [1]
                    if action == "invalid:bigint":
                        return 2**63
                    if action.startswith("echo:"):
                        return kwargs[action[5:]]
                    return host_value(action)

                outcome = invoke(
                    request(
                        params, args, case["returns"], case["may_missing"] == "true"
                    ),
                    callback,
                )
                self.assertEqual(encode_outcome(self, outcome), case["expected"])
                self.assertEqual(len(calls), int(case["trace"] != "-"))
                self.assertEqual(";".join(trace) if trace else "-", case["trace"])

    def test_owned_text_retention_and_repeated_calls(self):
        """Retained host arguments/results survive request deletion and collection."""
        text = "a\0snow\u96ea\U0001f980"
        req = request(
            [parameter("str")], [{"name": "x", "value": {"str": text}}], "str"
        )
        held = []

        def callback(host_x):
            """Retain a Python scalar after the native argument list is dropped."""
            held.append(host_x)
            return host_x

        results = [invoke(req, callback) for _ in range(100)]
        del req
        gc.collect()
        self.assertEqual(held, [text] * 100)
        self.assertEqual(results, [{"status": "value", "value": {"str": text}}] * 100)

    def test_thread_and_reentrant_call_order(self):
        """Callbacks execute on the caller's thread and may make a nested native call."""
        trace = []
        caller = threading.get_ident()

        def outer():
            """Call another native invocation before completing the outer callback."""
            trace.append(("outer", threading.get_ident()))
            inner = invoke(
                request(), lambda: trace.append(("inner", threading.get_ident())) or 4
            )
            return int(inner["value"]["int"]) + 1

        self.assertEqual(invoke(request(), outer)["value"], {"int": "5"})
        self.assertEqual(trace, [("outer", caller), ("inner", caller)])
        worker_results = []

        def worker():
            """A Python-owned worker remains the owner of its own native callback."""
            identity = threading.get_ident()
            worker_results.append((identity, invoke(request(), threading.get_ident)))

        thread = threading.Thread(target=worker)
        thread.start()
        thread.join()
        identity, outcome = worker_results[0]
        self.assertEqual(outcome["value"], {"int": str(identity)})

    def test_temporal_encoding_drops_only_argument_precision(self):
        """Civil fields survive; host dates carry no collected-precision metadata."""
        for kind, text, precision, expected_precision, expected_type in [
            ("date", "0001-01-01", "year", "day", dt.date),
            ("datetime", "1969-12-31T23:59:59", "day", "second", dt.datetime),
            ("datetime", "9999-12-31T23:59:59", "second", "second", dt.datetime),
        ]:
            seen = []

            def callback(host_x, _seen=seen):
                """Record actual host type and return the host scalar unchanged."""
                _seen.append(type(host_x))
                return host_x

            req = request(
                [parameter(kind)],
                [
                    {
                        "name": "x",
                        "value": {kind: {"text": text, "precision": precision}},
                    }
                ],
                kind,
            )
            self.assertEqual(
                invoke(req, callback)["value"],
                {kind: {"text": text, "precision": expected_precision}},
            )
            self.assertEqual(seen, [expected_type])

    def test_result_rejection_and_nonfinite_normalization(self):
        """No vector/subclass coercion, fractional datetime or oversized int slips in."""

        class IntegerSubclass(int):
            """An arbitrary numeric subclass is not the exact host scalar type."""

        class TextSubclass(str):
            """Text subclasses remain compatible with the reference's str check."""

        for value in [
            [],
            {},
            object(),
            IntegerSubclass(1),
            2**63,
            -(2**63) - 1,
            "\ud800",
        ]:
            with self.subTest(value=type(value).__name__):
                self.assertEqual(
                    invoke(request(), lambda v=value: v)["diagnostic"]["condition"],
                    "invalid_function_result",
                )
        for value in [
            dt.datetime(2025, 1, 1, microsecond=1),  # noqa: DTZ001
            dt.datetime(2025, 1, 1, tzinfo=dt.timezone.utc),
        ]:
            result = invoke(request(returns="datetime"), lambda v=value: v)
            self.assertEqual(
                result["diagnostic"]["context"]["reason"],
                "a returned datetime carries a zone or a fraction of a second",
            )
        self.assertEqual(
            invoke(request(returns="str"), lambda: TextSubclass("text"))["value"],
            {"str": "text"},
        )
        for value in [math.nan, math.inf, -math.inf]:
            self.assertEqual(
                invoke(request(returns="float", may_missing=True), lambda v=value: v)[
                    "value"
                ],
                {"missing": None},
            )

    def test_exceptions_cancellation_and_bounded_details(self):
        """Primary cancellation propagates; bad exception rendering retains identity."""

        class Unprintable(Exception):
            """Exercise a secondary failure while converting an exception to text."""

            def __str__(self):
                """Fail deliberately instead of providing the original message."""
                raise ValueError("secondary")

        def fail(error):
            """Raise the exact supplied error instance from inside the callback."""
            raise error

        for error in [KeyboardInterrupt(), SystemExit(7)]:
            with self.assertRaises(type(error)) as caught:
                invoke(request(), lambda e=error: fail(e))
            self.assertIs(caught.exception, error)
        result = invoke(request(), lambda: fail(Unprintable()))
        self.assertEqual(result["diagnostic"]["context"]["host_error"], "Unprintable")
        self.assertEqual(
            result["diagnostic"]["context"]["host_message"],
            "<exception message unavailable>",
        )
        result = invoke(request(), lambda: fail(ValueError("\u96ea" * 10000)))
        context = result["diagnostic"]["context"]
        self.assertEqual(context["host_message"], "\u96ea" * 2730)
        self.assertTrue(context["host_details_truncated"])
        long_class = type("E" * 10000, (Exception,), {})
        context = invoke(request(), lambda: fail(long_class()))["diagnostic"]["context"]
        self.assertEqual(context["host_error"], "E" * 8192)
        self.assertTrue(context["host_details_truncated"])
        self.assertEqual(invoke(request(), lambda: 9)["value"], {"int": "9"})

    def test_rejected_requests_never_call_and_failed_outputs_never_retry(self):
        """Admission fails before effects; oversized return failures retain one effect."""
        calls = []

        def callback(**kwargs):
            """Count attempts independently of whether serialization succeeds."""
            calls.append(kwargs)
            return "x" * (1048576 + 1)

        for name in ["for", "bad-name", "snow\u96ea", "1x", "x\0"]:
            p = parameter()
            p["host_name"] = name
            with self.assertRaisesRegex(ValueError, "host argument name"):
                invoke(request([p], [{"name": "x", "value": {"int": "1"}}]), callback)
        for text in ["{}", "[]", "bad", "x" * 1048577]:
            with self.assertRaises(ValueError):
                yamaa_native.invoke_function(text, callback)
        for value in [None, 1, [], {}]:
            with self.assertRaises(TypeError):
                yamaa_native.invoke_function(json.dumps(request()), value)
        self.assertEqual(calls, [])
        with self.assertRaisesRegex(ValueError, "output exceeds"):
            invoke(request(returns="str"), callback)
        self.assertEqual(calls, [{}])
        output = invoke(request(returns="str"), lambda: "\0" * 1048576)
        self.assertEqual(output["value"], {"str": "\0" * 1048576})


if __name__ == "__main__":
    unittest.main()
