"""An unavailable coverage artifact must not prevent documentation publication."""

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[3]


class CoverageDeployment(unittest.TestCase):
    def test_download_success_absence_and_partial_failure(self):
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows/deploy-docs.yml").read_text()
        )
        script = next(
            step["run"]
            for step in workflow["jobs"]["deploy"]["steps"]
            if step.get("name") == "Prepare coverage report"
        )
        cases = (
            ("123", "", "success", ["run"], True),
            ("123", "", "missing", ["run"], False),
            ("", "", "missing", ["api"], False),
            ("", "456", "success", ["api", "run"], True),
            ("", "456", "missing", ["api", "run"], False),
        )
        for run_id, lookup_id, download, calls, measured in cases:
            with (
                self.subTest(run_id=run_id, lookup_id=lookup_id, download=download),
                tempfile.TemporaryDirectory() as temporary,
            ):
                root = Path(temporary)
                helper = root / ".github/scripts/ci/coverage_report.py"
                helper.parent.mkdir(parents=True)
                shutil.copy2(ROOT / helper.relative_to(root), helper)
                binary = root / "bin"
                binary.mkdir()
                (binary / "python").symlink_to(sys.executable)
                gh = binary / "gh"
                gh.write_text(
                    f"#!{sys.executable}\n"
                    "import os, sys\nfrom pathlib import Path\n"
                    "with Path('calls').open('a') as log:\n"
                    "    log.write(sys.argv[1] + '\\n')\n"
                    "if sys.argv[1] == 'api':\n"
                    "    print(os.environ['TEST_LOOKUP_ID'])\n"
                    "    raise SystemExit(0)\n"
                    "assert sys.argv[2:5] == ['download', "
                    "os.environ['TEST_DOWNLOAD_ID'], '--name']\n"
                    "output = Path(sys.argv[-1])\n"
                    "output.mkdir(parents=True, exist_ok=True)\n"
                    "(output / 'summary.json').write_text('measured totals')\n"
                    "if os.environ['TEST_DOWNLOAD'] == 'missing':\n"
                    "    (output / 'partial.xml').write_text('partial download')\n"
                    "    raise SystemExit(1)\n"
                    "(output / 'badge.svg').write_text('measured badge')\n"
                    "(output / 'index.md').write_text('measured coverage')\n"
                )
                gh.chmod(0o755)
                result = subprocess.run(
                    [
                        "bash",
                        "--noprofile",
                        "--norc",
                        "-e",
                        "-o",
                        "pipefail",
                        "-c",
                        script,
                    ],
                    cwd=root,
                    env={
                        **os.environ,
                        "PATH": str(binary) + os.pathsep + os.environ["PATH"],
                        "GITHUB_REPOSITORY": "elong0527/yamaa",
                        "COVERAGE_RUN_ID": run_id,
                        "TEST_LOOKUP_ID": lookup_id,
                        "TEST_DOWNLOAD_ID": run_id or lookup_id,
                        "TEST_DOWNLOAD": download,
                    },
                    text=True,
                    capture_output=True,
                    check=False,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual((root / "calls").read_text().splitlines(), calls)
                output = root / "docs/coverage"
                if measured:
                    self.assertEqual(
                        (output / "index.md").read_text(), "measured coverage"
                    )
                    self.assertEqual(
                        (output / "badge.svg").read_text(), "measured badge"
                    )
                    self.assertTrue((output / "summary.json").exists())
                else:
                    self.assertEqual(
                        {path.name for path in output.iterdir()},
                        {"index.md", "badge.svg"},
                    )
                    self.assertIn(
                        "coverage: pending", (output / "badge.svg").read_text()
                    )


if __name__ == "__main__":
    unittest.main()
