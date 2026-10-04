"""Installed optional facade/native compatibility for known temporal results."""

import datetime as dt
import gc
import json
import unittest
from pathlib import Path

import yamaa
import yamaa_native
from installed_functions import parameter, request
from yamaa.adapters.native_functions import invoke_function
from yamaa.functions.invocation import BoundFunction
from yamaa.functions.models import FunctionBinding, FunctionContract
from yamaa.models.values import DateTimeValue, DateValue, ValueResult


def invoke(req, callback):
    """Inspect the real installed native outcome, never a mocked decoder."""
    return json.loads(invoke_function(json.dumps(req), callback))["outcome"]


class InstalledTemporalResults(unittest.TestCase):
    """Preserve fields and collected precision across one real callback boundary."""

    def test_known_models_and_precision(self):
        """Use independent expected text/precision, including both calendar limits."""
        self.assertIn("site-packages", str(Path(yamaa.__file__).resolve()))
        self.assertIn("site-packages", str(Path(yamaa_native.__file__).resolve()))
        cases = [
            (
                DateValue(year=1, month=1, day=1, collected_precision="year"),
                "date",
                "0001-01-01",
                "year",
            ),
            (
                DateValue(year=9999, month=12, day=31, collected_precision="month"),
                "date",
                "9999-12-31",
                "month",
            ),
            (DateValue(year=2024, month=2, day=29), "date", "2024-02-29", "day"),
            (
                DateTimeValue(
                    year=1,
                    month=1,
                    day=1,
                    hour=0,
                    minute=0,
                    second=0,
                    collected_precision="day",
                ),
                "datetime",
                "0001-01-01T00:00:00",
                "day",
            ),
            (
                DateTimeValue(
                    year=9999, month=12, day=31, hour=23, minute=59, second=59
                ),
                "datetime",
                "9999-12-31T23:59:59",
                "second",
            ),
        ]
        for value, kind, text, precision in cases:
            with self.subTest(kind=kind, precision=precision):
                effects = []

                def callback(_effects=effects, _value=value):
                    _effects.append(1)
                    return _value

                actual = invoke(request(returns=kind), callback)
                del callback
                gc.collect()
                self.assertEqual(effects, [1])
                contract = FunctionContract(
                    contract_version="1",
                    implementation_version="2",
                    description="Independent temporal result truth",
                    params=[],
                    returns=kind,
                    binding=FunctionBinding(call="artifact.example"),
                    conformance="unused.yaml",
                )
                reference = BoundFunction(
                    "example", contract, lambda _value=value: _value
                ).invoke({})
                self.assertIsInstance(reference, ValueResult)
                self.assertEqual(reference.value.to_text(), text)
                self.assertEqual(reference.value.collected_precision, precision)
                self.assertEqual(
                    actual,
                    {
                        "status": "value",
                        "value": {kind: {"text": text, "precision": precision}},
                    },
                )
                wrong = invoke(request(returns="str"), lambda _value=value: _value)
                self.assertEqual(
                    wrong["diagnostic"]["condition"], "invalid_function_result"
                )
                self.assertEqual(wrong["diagnostic"]["requirement"], "REQ-0702")
                self.assertEqual(wrong["diagnostic"]["context"]["actual"], kind)
                self.assertEqual(wrong["diagnostic"]["context"]["expected"], "str")

    def test_builtin_arguments_and_results(self):
        """The new result bridge must not move the argument precision boundary."""
        for kind, text, precision, expected in [
            ("date", "2024-02-29", "month", dt.date(2024, 2, 29)),
            (
                "datetime",
                "2024-02-29T12:34:56",
                "day",
                dt.datetime(2024, 2, 29, 12, 34, 56),  # noqa: DTZ001 - zone-free contract
            ),
        ]:
            req = request(
                [parameter(kind)],
                [
                    {
                        "name": "x",
                        "value": {kind: {"text": text, "precision": precision}},
                    }
                ],
                returns=kind,
            )

            def echo(host_x, _expected=expected):
                self.assertIs(type(host_x), type(_expected))
                self.assertEqual(host_x, _expected)
                return host_x

            self.assertEqual(
                invoke(req, echo)["value"],
                {
                    kind: {
                        "text": text,
                        "precision": "day" if kind == "date" else "second",
                    }
                },
            )

    def test_subclasses_and_invalid_models(self):
        """Known subtypes retain reference acceptance; forged models are validated."""

        class SubDate(DateValue):
            pass

        class SubDateTime(DateTimeValue):
            pass

        for value, kind, precision in [
            (
                SubDate(year=2000, month=2, day=29, collected_precision="year"),
                "date",
                "year",
            ),
            (
                SubDateTime(
                    year=2000,
                    month=2,
                    day=29,
                    hour=1,
                    minute=2,
                    second=3,
                    collected_precision="day",
                ),
                "datetime",
                "day",
            ),
        ]:
            actual = invoke(request(returns=kind), lambda _value=value: _value)
            self.assertEqual(actual["value"][kind]["precision"], precision)
        for value in [
            DateValue.model_construct(year=2024, month=2),
            DateValue.model_construct(year=True, month=1, day=1),
            DateValue.model_construct(year=2023, month=2, day=29),
            DateValue.model_construct(year=2024.0, month=1, day=1),
            DateValue.model_construct(
                year=2024, month=1, day=1, collected_precision="second"
            ),
            DateTimeValue.model_construct(
                year=2024, month=1, day=1, hour=0, minute=0, second=60
            ),
            DateTimeValue.model_construct(
                year=2024, month=1, day=1, hour=0, minute=0, second=1.5
            ),
        ]:
            effects = []

            def bad(_effects=effects, _value=value):
                _effects.append(1)
                return _value

            outcome = invoke(request(returns="date"), bad)
            self.assertEqual(effects, [1])
            self.assertEqual(
                outcome["diagnostic"]["condition"], "invalid_function_result"
            )
            self.assertEqual(outcome["diagnostic"]["requirement"], "REQ-0702")
            self.assertEqual(
                outcome["diagnostic"]["context"]["reason"],
                "a returned temporal representation is invalid",
            )
        self.assertEqual(invoke(request(), lambda: 7)["value"], {"int": "7"})

    def test_unknown_objects_and_control_flow(self):
        """No duck typing, extra invocation, exception swallowing or control masking."""

        class Unknown:
            def __getattr__(self, name):
                raise AssertionError("unknown result attributes must not be read")

        outcome = invoke(request(), lambda: Unknown())
        self.assertEqual(outcome["diagnostic"]["requirement"], "REQ-0702")
        self.assertEqual(outcome["diagnostic"]["context"]["returned"], "Unknown")
        for error in [ValueError("boom"), KeyboardInterrupt(), SystemExit(8)]:
            effects = []

            def callback(_effects=effects, _error=error):
                _effects.append(1)
                raise _error

            if isinstance(error, Exception):
                outcome = invoke(request(), callback)
                self.assertEqual(outcome["diagnostic"]["requirement"], "REQ-0701")
                self.assertEqual(
                    outcome["diagnostic"]["context"]["host_error"], "ValueError"
                )
            else:
                with self.assertRaises(type(error)) as raised:
                    invoke(request(), callback)
                self.assertIs(raised.exception, error)
            self.assertEqual(effects, [1])
        # Signature admission and missing short-circuit happen before the facade.
        effects = []
        req = request(
            [parameter("date")],
            [{"name": "x", "value": {"missing": None}}],
            returns="date",
        )
        self.assertEqual(
            invoke(req, lambda **kwargs: effects.append(kwargs))["value"],
            {"missing": None},
        )
        self.assertEqual(effects, [])

    def test_known_model_accessor_failures(self):
        """Invalid known-model reads are result failures; control signals survive."""
        for error in [RuntimeError("broken field"), KeyboardInterrupt(), SystemExit(6)]:

            class BrokenDate(DateValue):
                def __getattribute__(self, name, _error=error):
                    if name == "year":
                        raise _error
                    return super().__getattribute__(name)

            value = BrokenDate.model_construct(year=2024, month=1, day=1)
            effects = []

            def callback(_value=value, _effects=effects):
                _effects.append(1)
                return _value

            if isinstance(error, Exception):
                outcome = invoke(request(returns="date"), callback)
                self.assertEqual(outcome["diagnostic"]["requirement"], "REQ-0702")
            else:
                with self.assertRaises(type(error)) as raised:
                    invoke(request(returns="date"), callback)
                self.assertIs(raised.exception, error)
            self.assertEqual(effects, [1])

    def test_closed_native_carrier(self):
        """Direct factories cannot construct arbitrary values or mutate held dates."""
        factory = yamaa_native._temporal_result
        value = factory("date", (2024, 2, 29), "month")
        with self.assertRaises(TypeError):
            type(value)()
        with self.assertRaises((AttributeError, TypeError)):
            value.year = 1
        with self.assertRaises(TypeError):
            type("Subclass", (type(value),), {})
        del factory
        gc.collect()
        result = json.loads(
            yamaa_native.invoke_function(
                json.dumps(request(returns="date")), lambda: value
            )
        )
        self.assertEqual(
            result["outcome"]["value"],
            {"date": {"text": "2024-02-29", "precision": "month"}},
        )
        for args in [
            (None, None, None),
            ("int", (1,), "day"),
            ("date", [2024, 2, 29], "day"),
            ("date", (True, 1, 1), "day"),
            ("date", (2024, 2, 29, 1), "day"),
            ("date", (2**100, 1, 1), "day"),
            ("date", (0, 1, 1), "day"),
            ("datetime", (2024, 2, 29, 24, 0, 0), "second"),
            ("date", (2024, 2, 29), "\ud800"),
        ]:
            carrier = yamaa_native._temporal_result(*args)
            outcome = json.loads(
                yamaa_native.invoke_function(
                    json.dumps(request(returns="date")),
                    lambda _carrier=carrier: _carrier,
                )
            )["outcome"]
            self.assertEqual(outcome["diagnostic"]["requirement"], "REQ-0702")


if __name__ == "__main__":
    unittest.main()
