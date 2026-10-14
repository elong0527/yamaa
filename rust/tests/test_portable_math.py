"""Public math results retain exact values, conversion handlers and saved bytes."""

import json
import struct
import tempfile
import unittest
from pathlib import Path

import yamaa


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
        return yamaa.domain(path)

    def test_public_values_preserve_bits_missingness_and_integer_promotion(self):
        fixture = (
            Path(__file__).parents[1]
            / "crates/yamaa-core/tests/fixtures/portable_math.tsv"
        )
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
                    self.assertEqual(result.issues["condition"].to_list(), [expected])
                    self.assertEqual(result.issues["phase"].to_list(), ["derivation"])
                    self.assertEqual(
                        result.issues["requirement"].to_list(),
                        ["REQ-0432" if function == "LN" else "REQ-0433"],
                    )
                    self.assertEqual(
                        result.issues["spec_paths"].to_list(),
                        [["columns.VALUE.derivation.compute"]],
                    )
                    context = json.loads(result.issues["context"][0])
                    self.assertEqual(context["expr"], expression)
                    self.assertEqual(context["keys"], [{"ID": 1}])
                    if function == "POWER":
                        self.assertEqual(struct.pack(">d", context["base"]).hex(), left)
                        self.assertEqual(
                            struct.pack(">d", context["exponent"]).hex(), right
                        )
                    continue
                self.assertTrue(result.issues.is_empty(), result.issues)
                expected = None if expected == "missing" else expected
                value = result.output["VALUE"][0]
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
                self.assertTrue(result.issues.is_empty(), result.issues)
                self.assertEqual(result.output["VALUE"][0], expected)
                observation = json.loads(result._native.observations())
                self.assertEqual(
                    [int(row["count"]) for row in observation["handler_counts"]],
                    [calls],
                )

    def test_exp_result_drives_predicates_rounding_verification_and_exact_csv(self):
        specification = self.specification("EXP(A)", [1.0])
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
                        "compute": {"expr": "ROUND_HALF_AWAY_FROM_ZERO(VALUE, 15)"}
                    },
                },
            ]
        )
        specification["output"]["columns"] = ["ID", "VALUE", "ABOVE", "ROUNDED"]
        result = self.build(specification)
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(result.output["ABOVE"][0], 1)
        self.assertEqual(
            struct.pack(">d", result.output["ROUNDED"][0]).hex(), "4005bf0a8b14576b"
        )
        self.assertTrue(result.save())
        self.assertEqual(
            (self.root / "result.csv").read_bytes(),
            b"ID,VALUE,ABOVE,ROUNDED\n1,2.7182818284590455,1,2.718281828459046\n",
        )
        specification["verifications"] = [
            {"assert": {"id": "exp-one-limit", "require": "VALUE <= 2.718281828459045"}}
        ]
        verified = self.build(specification)
        self.assertEqual(verified.issues["condition"].to_list(), ["assert_failed"])
        self.assertEqual(verified.issues["requirement"].to_list(), ["REQ-0384"])
        context = json.loads(verified.issues["context"][0])
        self.assertEqual(context["failure_count"], 1)
        self.assertEqual(context["verification_id"], "exp-one-limit")


if __name__ == "__main__":
    unittest.main()
