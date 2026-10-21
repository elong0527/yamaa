"""pypi-install.yml installs each release from PyPI on every wheel platform.

After each PyPI upload the owner dispatches the workflow, which installs the
release into a fresh virtual environment on each platform that a release has
a wheel for, with the oldest and the newest supported CPython, and runs its
selftest (docs/release.md, "PyPI upload"). A platform or a Python that native
qualification gains must join this matrix too, or its wheel would never be
installed from PyPI.
"""

import unittest

import yaml
from tools.ci import native
from tools.paths import ROOT
from tools.validation._repository_yaml import UniqueKeyLoader


def version_key(version):
    return tuple(map(int, version.split(".")))


class PypiInstallWorkflow(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.jobs = yaml.load(
            (ROOT / ".github/workflows/pypi-install.yml").read_text(encoding="utf-8"),
            Loader=UniqueKeyLoader,
        )["jobs"]

    def test_each_wheel_platform_installs_with_the_oldest_and_newest_python(self):
        matrix = self.jobs["wheel"]["strategy"]["matrix"]
        self.assertEqual(set(matrix), {"os", "python"})
        self.assertEqual(sorted(matrix["os"]), sorted(native.PYTHON_PLATFORMS))
        pythons = sorted(native.PYTHONS, key=version_key)
        self.assertEqual(matrix["python"], [pythons[0], pythons[-1]])

    def test_no_job_checks_out_the_repository(self):
        # Each job installs from the index into a fresh virtual environment,
        # so no repository file can reach the installed package's path.
        for job_id, job in self.jobs.items():
            actions = [step.get("uses", "").split("@")[0] for step in job["steps"]]
            with self.subTest(job=job_id):
                self.assertNotIn("actions/checkout", actions)


if __name__ == "__main__":
    unittest.main()
