"""Run against the installed wheel from a directory outside the checkout."""

import unittest

import yamaa_native


class InstallationTests(unittest.TestCase):
    def test_capabilities_and_embedded_resource(self):
        self.assertEqual(
            yamaa_native.engine_info(),
            {
                "core_version": "0.1.0",
                "protocol_version": "installation-probe/1",
                "execution_supported": False,
                "installation_resource": "yamaa native installation probe\n",
            },
        )

    def test_each_call_owns_its_result(self):
        info = yamaa_native.engine_info()
        info["execution_supported"] = True
        self.assertIs(yamaa_native.engine_info()["execution_supported"], False)


if __name__ == "__main__":
    unittest.main()
