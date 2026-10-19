"""Public controls for the installed YAML parser: compact flow verifications
(#2196) and raw NUL refusal (#3125)."""

import json
import tempfile
import unittest
from pathlib import Path

import yamaa

SPEC = """schema_version: '1.0'
domain: TEST
keys: [ID]
input:
  SRC: input.csv
columns:
  - name: ID
    type: str
    label: ID
    derivation: SRC.ID
output:
  path: out.csv
  columns: [ID]
"""


class CompactYaml(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="yamaa-compact-yaml-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / "input.csv").write_bytes(b"ID\na\nb\nc\n")
        self.spec = self.root / "spec.yaml"

    def write(self, verification, *, prior_mapping=False):
        specification = SPEC
        if prior_mapping:
            specification = specification.replace(
                "input:\n  SRC: input.csv", "input: {SRC: input.csv}"
            )
        self.spec.write_text(specification + verification + "\n", encoding="ascii")

    def test_block_compact_and_explicit_verifications_have_equal_output_bytes(self):
        variants = [
            "verifications:\n  - row_count:\n      min: 3\n      max: 3",
            "verifications: [row_count: {min: 3, max: 3}]",
            "verifications: [{row_count: {min: 3, max: 3}}]",
            "verifications: [? row_count : {min: 3, max: 3}]",
            "verifications: [row_count: {min: 3, max: 3}, row_count: {min: 3, max: 3}]",
        ]
        for prior in (False, True):
            for verification in variants:
                with self.subTest(prior_mapping=prior, verification=verification):
                    self.write(verification, prior_mapping=prior)
                    self.assertEqual(yamaa.spec(self.spec).check().issues.height, 0)
                    built = yamaa.spec(self.spec).run()
                    self.assertEqual(built.issues.height, 0)
                    self.assertEqual(
                        built.output.to_dict(as_series=False), {"ID": ["a", "b", "c"]}
                    )
                    built.save()
                    self.assertEqual(
                        (self.root / "out.csv").read_bytes(), b"ID\na\nb\nc\n"
                    )

    def test_both_bounds_in_a_compact_pair_are_executed(self):
        for bounds in ("min: 0, max: 2", "min: 4, max: 10"):
            with self.subTest(bounds=bounds):
                self.write("verifications: [row_count: {" + bounds + "}]")
                self.assertEqual(yamaa.spec(self.spec).check().issues.height, 0)
                built = yamaa.spec(self.spec).run()
                self.assertIsNone(built.output)
                self.assertEqual(
                    built.issues["condition"].to_list(), ["row_count_failed"]
                )

    def test_malformed_compact_closers_remain_invalid_yaml(self):
        for verification in (
            "verifications: [row_count: {min: 3, max: 3]",
            "verifications: [row_count: {min: 3, max: 3}}]",
        ):
            self.write(verification)
            specification = yamaa.spec(self.spec)
            for operation, result in (
                ("check", specification.check()),
                ("run", specification.run()),
            ):
                with self.subTest(verification=verification, operation=operation):
                    self.assertEqual(
                        result.issues["condition"].to_list(), ["invalid_yaml"]
                    )

    def test_raw_nul_is_invalid_yaml_at_the_nul(self):
        # A raw NUL used to end the specification silently: the verification
        # after it was dropped, or the fields after it were reported missing.
        for text, line, column in (
            (SPEC + "\0verifications: [row_count: {min: 4}]\n", 14, 1),
            (SPEC.replace("keys: [ID]", "ke\0ys: [ID]"), 3, 3),
        ):
            self.spec.write_text(text, encoding="ascii")
            specification = yamaa.spec(self.spec)
            for operation, result in (
                ("check", specification.check()),
                ("run", specification.run()),
            ):
                with self.subTest(line=line, operation=operation):
                    self.assertEqual(
                        result.issues["condition"].to_list(), ["invalid_yaml"]
                    )
                    self.assertEqual(
                        json.loads(result.issues["context"][0]),
                        {
                            "line": line,
                            "column": column,
                            "reason": "found unprintable character",
                        },
                    )


if __name__ == "__main__":
    unittest.main()
