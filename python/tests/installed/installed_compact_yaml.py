"""Public controls for the installed YAML parser: compact flow verifications
(#2196) and raw NUL refusal (#3125).

The cases and their expected results are the `compact-yaml` family of the
shared host truth, which the R `compact_yaml` suite reads too.
"""

import tempfile
import unittest
from pathlib import Path

from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "compact-yaml")


class CompactYaml(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="yamaa-compact-yaml-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / "input.csv").write_bytes(TRUTH["input_csv"].encode("ascii"))
        self.spec = self.root / "spec.yaml"

    def write(self, verification, specification=TRUTH["specification_yaml"]):
        self.spec.write_bytes((specification + verification + "\n").encode("ascii"))

    def test_block_compact_and_explicit_verifications_have_equal_output_bytes(self):
        expected = TRUTH["equivalent"]
        for input_form, specification in (
            ("a block input", TRUTH["specification_yaml"]),
            ("a flow input", TRUTH["flow_input_yaml"]),
        ):
            for case in expected["cases"]:
                with self.subTest(case=f"{case['name']} after {input_form}"):
                    self.write(case["verification"], specification)
                    self.assertEqual(yamaa.spec(self.spec).check().issues.num_rows, 0)
                    built = yamaa.spec(self.spec).run()
                    self.assertEqual(built.issues.num_rows, 0)
                    self.assertEqual(
                        built.output.to_polars().to_dict(as_series=False),
                        expected["output"],
                    )
                    built.save()
                    self.assertEqual(
                        (self.root / "out.csv").read_bytes(),
                        expected["output_csv"].encode("ascii"),
                    )

    def test_both_bounds_in_a_compact_pair_are_executed(self):
        for case in TRUTH["failed_bounds"]:
            with self.subTest(case=case["name"]):
                self.write(case["verification"])
                self.assertEqual(yamaa.spec(self.spec).check().issues.num_rows, 0)
                built = yamaa.spec(self.spec).run()
                self.assertIsNone(built.output)
                self.assertEqual(
                    built.issues.to_polars()["condition"].to_list(),
                    case["conditions"],
                )

    def test_malformed_compact_closers_remain_invalid_yaml(self):
        for case in TRUTH["malformed"]:
            self.write(case["verification"])
            specification = yamaa.spec(self.spec)
            for operation, result in (
                ("check", specification.check()),
                ("run", specification.run()),
            ):
                with self.subTest(case=f"{operation} of {case['name']}"):
                    self.assertEqual(
                        result.issues.to_polars()["condition"].to_list(),
                        case["conditions"],
                    )

    def test_raw_nul_is_invalid_yaml_at_the_nul(self):
        # A raw NUL used to end the specification silently: the verification
        # after it was dropped, or the fields after it were reported missing.
        for case in TRUTH["raw_nul"]:
            self.spec.write_bytes(
                case["before_nul"].encode("ascii")
                + b"\0"
                + case["after_nul"].encode("ascii")
            )
            specification = yamaa.spec(self.spec)
            for operation, result in (
                ("check", specification.check()),
                ("run", specification.run()),
            ):
                with self.subTest(case=f"{operation} of {case['name']}"):
                    self.assertEqual(
                        [
                            {field: row[field] for field in case["issue"]}
                            for row in result.issues.to_pylist()
                        ],
                        [case["issue"]],
                    )


if __name__ == "__main__":
    unittest.main()
