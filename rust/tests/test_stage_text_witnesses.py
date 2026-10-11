"""Independent captured witness bytes survive Windows text-mode staging."""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "stage_specification_inputs", ROOT / "rust/tools/stage_specification_inputs.py"
)
STAGER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(STAGER)
ORIGINAL_OPEN = Path.open


def windows_text_open(path, mode="r", buffering=-1, encoding=None, errors=None, newline=None):
    # Exercise real text IO with Windows' default write translation on every host.
    if "w" in mode and "b" not in mode and newline is None:
        newline = "\r\n"
    return ORIGINAL_OPEN(path, mode, buffering, encoding, errors, newline)


class TextWitnessStaging(unittest.TestCase):
    def test_extraction_witness_bytes_match_capture_under_windows_text_translation(self):
        source = ROOT / "rust/crates/yamaa-adapters/tests/fixtures/text_extract_reference.json"
        capture = json.loads(source.read_bytes())
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory)
            with patch.object(Path, "open", windows_text_open):
                STAGER.stage_text_extract_truth(destination)
            self.assertEqual((destination / "text-extract-reference.json").read_bytes(), source.read_bytes())
            for name, case in capture.items():
                if "specification" not in case or name == "wide-group":
                    continue
                with self.subTest(case=name):
                    work = destination / "text-extract-cases" / name
                    expected = json.dumps(case["specification"], ensure_ascii=True) + "\n"
                    self.assertEqual((work / "spec.yaml").read_bytes(), expected.encode("ascii"))
                    self.assertEqual((work / "input.csv").read_bytes(), case["input"].encode())
                    if "csv" in case:
                        self.assertEqual((work / "expected.csv").read_bytes(), case["csv"].encode())

    def test_padding_witness_bytes_match_capture_under_windows_text_translation(self):
        source = ROOT / "rust/crates/yamaa-adapters/tests/fixtures/text_pad_reference.json"
        capture = json.loads(source.read_bytes())
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory)
            with patch.object(Path, "open", windows_text_open):
                STAGER.stage_text_pad_truth(destination)
            self.assertEqual((destination / "text-pad-reference.json").read_bytes(), source.read_bytes())
            for name, case in capture.items():
                with self.subTest(case=name):
                    work = destination / "text-pad-cases" / name
                    self.assertEqual((work / "spec.yaml").read_bytes(), case["specification"].encode())
                    self.assertEqual((work / "input.csv").read_bytes(), case["input"].encode())
                    if "csv" in case:
                        self.assertEqual((work / "expected.csv").read_bytes(), case["csv"].encode())


if __name__ == "__main__":
    unittest.main()
