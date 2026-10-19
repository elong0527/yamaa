"""Harbor's packages are locked apart from the package's, and kept current.

The release lane audits every package in python/uv.lock, so a package that only
the Harbor evaluation needs stays in evaluations/harbor/uv.lock. No pull request
job installs that project, so the repository check keeps its lock current.
"""

import sys
import tomllib
import unittest
from unittest import mock

from tools.commands import repository
from tools.paths import ROOT


def locked(path):
    """The names of the packages that a uv lock selects."""
    document = tomllib.loads((ROOT / path).read_text())
    return {package["name"] for package in document["package"]}


class HarborProjectTests(unittest.TestCase):
    def test_harbor_is_locked_apart_from_the_package(self):
        self.assertNotIn("harbor", locked("python/uv.lock"))
        self.assertIn("harbor", locked("evaluations/harbor/uv.lock"))

    def test_the_repository_check_requires_a_current_harbor_lock(self):
        commands = []
        with (
            mock.patch.object(
                repository, "run", lambda *arguments, **_: commands.append(arguments)
            ),
            mock.patch.object(sys, "argv", ["repository", "check"]),
        ):
            repository.main()
        self.assertIn(
            ("lock", "--check", "--project", "evaluations/harbor"),
            [arguments[1:] for arguments in commands],
        )


if __name__ == "__main__":
    unittest.main()
