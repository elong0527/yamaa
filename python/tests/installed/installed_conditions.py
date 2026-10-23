"""Installed yamaa.conditions(): the condition catalogue as a table (#3700).

The facade's own tests check each entry against the registry (the conditions
tests of the yamaa crate); this checks what the installed host returns for the
shared truth, which R and the command line read too: every condition in name
order, and the authored rows whole, their lists as Python lists and a missing
cell as None.
"""

import inspect
import unittest

from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "conditions")


class Conditions(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.table = yamaa.conditions()
        cls.rows = cls.table.to_pylist()

    def test_conditions_takes_no_arguments_and_is_exported(self):
        self.assertEqual(list(inspect.signature(yamaa.conditions).parameters), [])
        self.assertIn("conditions", yamaa.__all__)

    def test_the_table_names_every_condition_in_name_order(self):
        self.assertIsInstance(self.table, yamaa.Table)
        self.assertEqual(self.table.column_names, TRUTH["columns"])
        self.assertEqual([row["condition"] for row in self.rows], TRUTH["names"])

    def test_each_authored_row_is_the_row_of_its_condition(self):
        rows = {row["condition"]: row for row in self.rows}
        for cells in TRUTH["rows"]:
            with self.subTest(case=cells[0]):
                self.assertEqual(
                    rows[cells[0]], dict(zip(TRUTH["columns"], cells, strict=True))
                )

    def test_a_second_call_gives_the_same_catalogue(self):
        self.assertEqual(yamaa.conditions().to_pylist(), self.rows)


if __name__ == "__main__":
    unittest.main()
