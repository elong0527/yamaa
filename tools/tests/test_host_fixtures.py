"""The installed suites' summary of where an observed value first differs from
its truth (`tools.conformance.host_fixtures.first_difference`, #3882)."""

import unittest

from tools.conformance.host_fixtures import ABSENT, first_difference

ROW = ("S-001", *["x"] * 10)


class FirstDifference(unittest.TestCase):
    def test_first_difference_names_the_path_and_items_where_two_values_first_differ(
        self,
    ):
        cases = [
            (
                "equal nested values have no difference",
                [{"ID": 1, "V": [1, 2]}, ("a", None)],
                [{"ID": 1, "V": [1, 2]}, ("a", None)],
                None,
            ),
            (
                "a list names its first differing index",
                [1, 2, 3, 9, 5],
                [1, 2, 4, 4, 5],
                {"at": [2], "observed": 3, "expected": 4},
            ),
            (
                "rows of cells name the row, then the cell",
                [("S-001", "x", "x"), ("S-002", "y", "x")],
                [("S-001", "x", "x"), ("S-002", "y", "P")],
                {"at": [1, 2], "observed": "x", "expected": "P"},
            ),
            (
                "a dictionary row names its first differing key in expected order",
                [{"ItemOID": "I1", "Value": "a", "IsNull": False}],
                [{"ItemOID": "I1", "Value": "b", "IsNull": True}],
                {"at": [0, "Value"], "observed": "a", "expected": "b"},
            ),
            (
                "a key observed lacks is absent there",
                {"a": 1, "c": 3},
                {"a": 1, "b": 2, "c": 3},
                {"at": ["b"], "observed": ABSENT, "expected": 2},
            ),
            (
                "a key expected lacks is absent there",
                {"a": 1, "z": 26},
                {"a": 1},
                {"at": ["z"], "observed": 26, "expected": ABSENT},
            ),
            (
                "a shorter list is absent past its end",
                [1, 2],
                [1, 2, 3],
                {"at": [2], "observed": ABSENT, "expected": 3},
            ),
            (
                "a list and a tuple of the same items differ where they are",
                [1, 2],
                (1, 2),
                {"at": [], "observed": [1, 2], "expected": (1, 2)},
            ),
            (
                "a missing row of more than ten cells shows as its type and length",
                [ROW],
                [ROW, ROW],
                {"at": [1], "observed": ABSENT, "expected": "<tuple of 11 items>"},
            ),
            (
                "fifty thousand values name the last when it alone differs",
                [*range(49_999), -1],
                list(range(50_000)),
                {"at": [49_999], "observed": -1, "expected": 49_999},
            ),
        ]
        for label, observed, expected, summary in cases:
            with self.subTest(case=label):
                self.assertEqual(first_difference(observed, expected), summary)


if __name__ == "__main__":
    unittest.main()
