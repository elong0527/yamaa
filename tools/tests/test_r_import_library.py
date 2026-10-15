"""MSVC export projection controls for actual Windows R-binding test setup."""

import unittest

from tools.ci.r_import_library import export_definition


class RImportLibraryTest(unittest.TestCase):
    def test_dumpbin_headers_aliases_and_global_data_keep_their_exact_export_names(
        self,
    ):
        output = """Microsoft (R) COFF/PE Dumper

  Section contains the following exports for R.dll

    ordinal hint RVA      name

          3    2 00001040 Rf_eval = Rf_eval_alias
          1    0 00003000 R_GlobalEnv
          2    1 00001000 Rf_initialize_R

  Summary
        1000 .data
        2000 .text
"""
        self.assertEqual(
            export_definition(output),
            "LIBRARY R.dll\nEXPORTS\n    R_GlobalEnv\n    Rf_eval\n    Rf_initialize_R\n",
        )

    def test_missing_or_duplicate_export_names_are_rejected(self):
        for output in (
            "Summary\n  1000 .text",
            "  1 0 00001000 Rf_eval\n  2 1 00001010 Rf_eval\n",
        ):
            with self.subTest(output=output), self.assertRaises(ValueError):
                export_definition(output)


if __name__ == "__main__":
    unittest.main()
