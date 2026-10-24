"""Controls for the newline-delimited Dataset-JSON goldens: each committed
golden is the derivation of its authored single-object golden, carries that
golden's value line by line, and is valid against the vendored schema."""

import json
import unittest

from jsonschema import Draft202012Validator

from tools.conformance import ndjson_goldens
from tools.paths import ROOT

SCHEMA = ROOT / "rust/tests/schemas/dataset.schema.json"


class NewlineDelimitedGoldens(unittest.TestCase):
    def test_each_golden_is_its_single_object_golden_one_text_per_line(self):
        validator = Draft202012Validator(json.loads(SCHEMA.read_bytes()))
        for golden in ndjson_goldens.GOLDENS:
            benchmark, name, source, origin = golden
            with self.subTest(golden=f"{benchmark}/{name}"):
                committed = (
                    ROOT / "benchmarks" / benchmark / "expected" / name
                ).read_bytes()
                self.assertEqual(committed, ndjson_goldens.derive(golden))
                authored = json.loads(
                    (ROOT / "benchmarks" / source / "expected" / origin).read_bytes()
                )
                text = committed.decode("utf-8")
                self.assertTrue(text.endswith("\n"))
                self.assertNotIn("\r", text)
                lines = text[:-1].split("\n")
                first = json.loads(lines[0])
                self.assertNotIn("rows", first)
                self.assertEqual(
                    list(first), [key for key in authored if key != "rows"]
                )
                first["rows"] = [json.loads(line) for line in lines[1:]]
                self.assertEqual(first, authored)
                self.assertEqual(first["records"], len(lines) - 1)
                validator.validate(first)
                # The document differs from the source's only in the leaf it
                # names (JSON-12).
                xml = (
                    ROOT / "benchmarks" / benchmark / "expected/define.xml"
                ).read_bytes()
                source_xml = (
                    ROOT / "benchmarks" / source / "expected/define.xml"
                ).read_bytes()
                self.assertEqual(
                    xml, source_xml.replace(origin.encode(), name.encode())
                )

    def test_a_cell_outside_the_goldens_spellings_is_refused(self):
        for cell in (1.5, True):
            with self.subTest(cell=cell), self.assertRaises(ValueError):
                ndjson_goldens.lines({"records": 1, "rows": [[cell]]})


if __name__ == "__main__":
    unittest.main()
