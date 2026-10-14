"""Coverage badges must use complete measured totals, never partial estimates."""

import json
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

import coverage_report as report


class CoverageReport(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        (self.root / "rust.lcov").write_text(
            "SF:src/lib.rs\nDA:1,1\nDA:2,0\nend_of_record\n", encoding="utf-8"
        )
        for filename, covered, total in (("python.xml", 1, 1), ("r.xml", 0, 7)):
            (self.root / filename).write_text(
                f'<coverage lines-covered="{covered}" lines-valid="{total}"/>',
                encoding="utf-8",
            )

    def test_combines_line_counts_instead_of_averaging_percentages(self):
        counts = report.collect(self.root)
        self.assertEqual((counts["covered"], counts["total"]), (2, 10))
        self.assertEqual(report.percent(counts), 20)

    def test_missing_language_cannot_produce_a_percentage(self):
        for filename in report.REPORTS.values():
            with self.subTest(filename=filename):
                path = self.root / filename
                content = path.read_bytes()
                path.unlink()
                with self.assertRaises(FileNotFoundError):
                    report.collect(self.root)
                path.write_bytes(content)

    def test_invalid_or_empty_counts_are_rejected(self):
        for covered, total in ((0, 0), (-1, 10), (11, 10), (0, -1)):
            with (
                self.subTest(covered=covered, total=total),
                self.assertRaises(ValueError),
            ):
                report.checked_counts(covered, total)

    def test_duplicate_lcov_records_count_a_source_line_once(self):
        path = self.root / "rust.lcov"
        with path.open("a") as output:
            output.write("SF:src/lib.rs\nDA:2,3\nend_of_record\n")
        self.assertEqual(report.read_lcov(path), {"covered": 2, "total": 2})

    def test_unframed_lcov_cannot_be_counted(self):
        path = self.root / "rust.lcov"
        path.write_text("DA:1,1\n", encoding="utf-8")
        with self.assertRaises(ValueError):
            report.read_lcov(path)

    def test_pending_badge_does_not_claim_a_measured_percentage(self):
        output = self.root / "pending"
        report.publish(output)
        svg = ET.parse(output / "badge.svg").getroot()
        self.assertEqual(svg.attrib["aria-label"], "coverage: pending")
        self.assertFalse((output / "summary.json").exists())

    def test_measured_badge_and_page_keep_source_and_per_language_totals(self):
        output = self.root / "site"
        report.publish(
            output,
            report.collect(self.root),
            commit="a" * 40,
            run_url="https://github.com/elong0527/yamaa/actions/runs/123",
        )
        svg = ET.parse(output / "badge.svg").getroot()
        self.assertEqual(svg.attrib["aria-label"], "coverage: 20.0%")
        data = json.loads((output / "summary.json").read_text())
        self.assertEqual(data["commit"], "a" * 40)
        self.assertEqual(set(data["languages"]), {"Rust", "Python", "R"})
        page = (output / "index.md").read_text()
        self.assertIn("2 of 10 reported lines", page)
        self.assertIn("actions/runs/123", page)


if __name__ == "__main__":
    unittest.main()
