import importlib.util
import shutil
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("build_index", HERE / "build_index.py")
index = importlib.util.module_from_spec(spec)
spec.loader.exec_module(index)


class BenchmarkIndex(unittest.TestCase):
    def test_malformed_heading_cannot_retain_old_rows_beside_generated_rows(self):
        for heading in (
            "",
            "## Benchmarks \n",
            "## Benchmarks\n## Benchmarks\n",
            "## Benchmarks \n| old-case |\n## Benchmarks\n",
            "## Benchmarks\n| old-case |\n## Benchmarks \n",
            "##  Benchmarks\n| old-case |\n## Benchmarks\n",
        ):
            with (
                self.subTest(heading=heading),
                self.assertRaisesRegex(ValueError, "exactly one"),
            ):
                index.render_index(
                    index.ROOT / "benchmarks", "# Index\n" + heading + "| old-case |\n"
                )

    def test_generated_index_is_complete_and_stable(self):
        fixtures = index.ROOT / "benchmarks"
        current = (fixtures / "README.md").read_text(encoding="utf-8")
        result = index.render_index(fixtures, current)
        self.assertEqual(result, current)
        self.assertEqual(index.render_index(fixtures, result), result)
        self.assertEqual(result.count("| [`"), 318)
        self.assertEqual(result.count("| Python, R |"), 316)
        self.assertEqual(result.count("| Excluded: "), 2)
        self.assertIn("reviewed-yellow)](https://github.com/", result)

    def test_directory_addition_and_removal_update_index_without_a_registry(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixtures = Path(temporary)
            case = fixtures / "new-case"
            case.mkdir()
            (case / "spec.yaml").write_text("domain: TEST\n")
            (case / "README.md").write_text(
                "# New | Case\n"
                "[![Lifecycle: draft](https://example.com/badge)](https://example.com/lifecycle)\n"
            )
            result = index.render_index(fixtures, "# Index\n\n## Benchmarks\n")
            self.assertIn("| [`new-case`](new-case/) | New \\| Case |", result)
            self.assertIn("1 authored cases; 1 active cases", result)
            shutil.rmtree(case)
            result = index.render_index(fixtures, result)
            self.assertNotIn("new-case", result)
            self.assertIn("0 authored cases; 0 active cases", result)

    def test_authored_case_without_readme_cannot_be_silently_omitted(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixtures = Path(temporary)
            case = fixtures / "new-case"
            case.mkdir()
            (case / "spec.yaml").write_text("domain: TEST\n")
            with self.assertRaises(FileNotFoundError):
                index.render_index(fixtures, "# Index\n\n## Benchmarks\n")


if __name__ == "__main__":
    unittest.main()
