"""Public math results retain exact values, conversion handlers and saved bytes."""

import json
import struct
import tempfile
import unittest
from pathlib import Path

import yamaa

from tools.paths import WORKSPACE


def binary64(bits):
    return struct.unpack(">d", bytes.fromhex(bits))[0]


class PublicPortableMath(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory(prefix="yamaa-portable-math-")
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)

    def specification(self, expression, operands, target="float", handler=False):
        kinds = {"ID": "int"}
        values = ["1"]
        columns = [{"name": "ID", "type": "int", "derivation": "SRC.ID"}]
        for name, value in zip("AB", operands, strict=False):
            kinds[name] = "int" if type(value) is int else "float"
            values.append("" if value is None else repr(value))
            columns.append(
                {"name": name, "type": kinds[name], "derivation": "SRC." + name}
            )
        derivation = {"compute": {"expr": expression}}
        if handler:
            derivation = {"value": derivation, "unconvertible": 9}
        columns.append({"name": "VALUE", "type": target, "derivation": derivation})
        (self.root / "input.csv").write_text(
            ",".join(kinds) + "\n" + ",".join(values) + "\n", encoding="ascii"
        )
        return {
            "schema_version": "1.0",
            "domain": "MATH",
            "keys": ["ID"],
            "input": {"SRC": {"path": "input.csv", "types": kinds}},
            "columns": columns,
            "output": {"path": "result.csv", "columns": ["ID", "VALUE"]},
        }

    def build(self, specification):
        path = self.root / "spec.yaml"
        path.write_text(json.dumps(specification), encoding="ascii")
        return yamaa.spec(path).run()

    def test_public_values_preserve_bits_missingness_and_integer_promotion(self):
        fixture = WORKSPACE / "crates/yamaa-core/tests/fixtures/portable_math.tsv"
        rows = fixture.read_text().splitlines()
        self.assertEqual(len(rows), 58)
        for row in rows:
            name, function, left, right, expected = row.split()
            expression = function + ("(A, B)" if function == "POWER" else "(A)")
            operands = []
            for operand in (left, right) if function == "POWER" else (left,):
                operands.append(
                    None
                    if operand == "missing"
                    else int(operand[4:])
                    if operand.startswith("int:")
                    else binary64(operand)
                )
            with self.subTest(case=name):
                result = self.build(self.specification(expression, operands))
                if expected in {"ln_of_nonpositive", "invalid_power"}:
                    self.assertEqual(
                        result.issues.to_polars()["condition"].to_list(), [expected]
                    )
                    self.assertEqual(
                        result.issues.to_polars()["phase"].to_list(), ["derivation"]
                    )
                    self.assertEqual(
                        result.issues.to_polars()["rule"].to_list(),
                        ["FORMULA-15"],
                    )
                    self.assertEqual(
                        result.issues.to_polars()["spec_paths"].to_list(),
                        [["columns.VALUE.derivation.compute"]],
                    )
                    context = json.loads(result.issues.to_polars()["context"][0])
                    self.assertEqual(context["expr"], expression)
                    self.assertEqual(context["keys"], [{"ID": 1}])
                    if function == "POWER":
                        self.assertEqual(struct.pack(">d", context["base"]).hex(), left)
                        self.assertEqual(
                            struct.pack(">d", context["exponent"]).hex(), right
                        )
                    continue
                self.assertEqual(result.issues.num_rows, 0, result.issues)
                expected = None if expected == "missing" else expected
                value = result.output.to_polars()["VALUE"][0]
                self.assertEqual(
                    None if value is None else struct.pack(">d", value).hex(), expected
                )

    def test_integer_handlers_preserve_both_conversion_directions_and_counts(self):
        for operands, expected, calls in (
            (
                [binary64("40689ed31ed9c3bf"), binary64("401b2641709adeb1")],
                3739524560436070,
                0,
            ),
            ([binary64("3fcfffffffffffff"), -1.0], 9, 1),
        ):
            with self.subTest(operands=operands):
                result = self.build(
                    self.specification("POWER(A, B)", operands, "int", True)
                )
                self.assertEqual(result.issues.num_rows, 0, result.issues)
                self.assertEqual(result.output.to_polars()["VALUE"][0], expected)
                observation = json.loads(result._native.observations())
                self.assertEqual(
                    observation["handler_counts"],
                    [
                        {
                            "count": calls,
                            "handler": "unconvertible",
                            "spec_path": "columns.VALUE.derivation.unconvertible",
                        }
                    ],
                )

    def assert_downstream(self, specification, value_bits, above, rounded_bits, csv):
        specification["columns"].extend(
            [
                {
                    "name": "ABOVE",
                    "type": "int",
                    "derivation": {
                        "case": [
                            {
                                "when": "VALUE > 2.718281828459045",
                                "then": {"literal": 1},
                            },
                            {"otherwise": {"literal": 0}},
                        ]
                    },
                },
                {
                    "name": "ROUNDED",
                    "type": "float",
                    "derivation": {
                        "round_half_away_from_zero": {"source": "VALUE", "digits": 15}
                    },
                },
            ]
        )
        specification["output"]["columns"] = ["ID", "VALUE", "ABOVE", "ROUNDED"]
        result = self.build(specification)
        self.assertEqual(result.issues.num_rows, 0, result.issues)
        self.assertEqual(
            result.output.column_names, ["ID", "VALUE", "ABOVE", "ROUNDED"]
        )
        self.assertEqual(result.output.num_rows, 1)
        self.assertEqual(result.output.to_polars()["ID"][0], 1)
        self.assertEqual(
            struct.pack(">d", result.output.to_polars()["VALUE"][0]).hex(), value_bits
        )
        self.assertEqual(result.output.to_polars()["ABOVE"][0], above)
        self.assertEqual(
            struct.pack(">d", result.output.to_polars()["ROUNDED"][0]).hex(),
            rounded_bits,
        )
        self.assertEqual(
            result.save(), [(self.root / "result.csv").resolve().as_posix()]
        )
        self.assertEqual((self.root / "result.csv").read_bytes(), csv)
        specification["verifications"] = [
            {
                "assert": {
                    "id": "historical-exp-one-limit",
                    "require": "VALUE <= 2.718281828459045",
                }
            }
        ]
        verified = self.build(specification)
        self.assertEqual(
            verified.issues.to_pylist(),
            [
                {
                    "severity": "error",
                    "phase": "verification",
                    "condition": "assert_failed",
                    "rule": "CHECK-12",
                    "spec_paths": ["verifications[0].assert"],
                    "file": (self.root / "spec.yaml").resolve().as_posix(),
                    "line": 1,
                    "column": json.dumps(specification).index('"assert":') + 1,
                    "message": "the assert predicate is not TRUE in 1 of the rows",
                    "context": '{"failure_count":1,"keys":[{"ID":1}],'
                    '"verification_id":"historical-exp-one-limit"}',
                    "source_span": None,
                    "operand_route": None,
                }
            ]
            if above
            else [],
        )

    def test_exp_result_drives_predicates_rounding_verification_and_exact_csv(self):
        self.assert_downstream(
            self.specification("EXP(A)", [1.0]),
            "4005bf0a8b14576a",
            1,
            "4005bf0a8b14576b",
            b"ID,VALUE,ABOVE,ROUNDED\n1,2.7182818284590455,1,2.718281828459046\n",
        )

    def test_adjacent_declared_values_retain_both_downstream_controls(self):
        # These authored controls were already exercised through the current
        # public engine. Both values are source data, independent of EXP output.
        for value_bits, above, rounded_bits, csv in (
            (
                "4005bf0a8b145769",
                0,
                "4005bf0a8b145769",
                b"ID,VALUE,ABOVE,ROUNDED\n1,2.718281828459045,0,2.718281828459045\n",
            ),
            (
                "4005bf0a8b14576a",
                1,
                "4005bf0a8b14576b",
                b"ID,VALUE,ABOVE,ROUNDED\n1,2.7182818284590455,1,2.718281828459046\n",
            ),
        ):
            with self.subTest(input_bits=value_bits):
                self.assert_downstream(
                    self.specification("A", [binary64(value_bits)]),
                    value_bits,
                    above,
                    rounded_bits,
                    csv,
                )


if __name__ == "__main__":
    unittest.main()
