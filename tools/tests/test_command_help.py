"""`make help` offers only the routes AGENTS.md allows."""

import unittest

from tools.commands import help as command_help


class CommandHelp(unittest.TestCase):
    def test_help_names_native_qualification_on_main_only(self):
        # The help text is a constant no reader covers. D233 on #3673 runs
        # native qualification only on main, and AGENTS.md forbids pushing a
        # qualification/issue-* branch, so make help must not offer one (#4977).
        self.assertNotIn("qualification/issue-", command_help.HELP)
        self.assertIn(
            "native.yml, run only on main: weekly, and dispatched\n"
            "at the candidate and release cuts (D233 on #3673).",
            command_help.HELP,
        )


if __name__ == "__main__":
    unittest.main()
