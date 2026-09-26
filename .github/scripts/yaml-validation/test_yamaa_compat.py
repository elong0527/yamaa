#!/usr/bin/env python3
"""Regression checks for the yaml-validation compat shim's predicate AST
converter: the str_contains branch must check the function name carried by
the node and never mislabel a future second Boolean call."""

import unittest

from yamaa_compat import PredicateError, _to_validator_ast


class StrContainsNameGuardTests(unittest.TestCase):
    def test_well_formed_node_converts(self):
        out = _to_validator_ast(("str_contains", ("ident", "DM.SEX"), "^M"))
        self.assertEqual(out["kind"], "call")
        self.assertEqual(out["name"], "str_contains")
        self.assertEqual(
            out["source"], {"kind": "identifier", "name": "DM.SEX", "position": 0}
        )
        self.assertEqual(out["pattern"], "^M")

    def test_malformed_nodes_raise_predicate_error(self):
        # Wrong arity or a non-string pattern must raise PredicateError,
        # never a bare ValueError from tuple unpacking.
        bad_nodes = [
            ("str_contains", ("ident", "x")),
            ("str_contains", ("ident", "x"), "p", "extra"),
            ("str_contains", ("ident", "x"), 123),
        ]
        for node in bad_nodes:
            with self.assertRaises(PredicateError, msg=f"{node!r}"):
                _to_validator_ast(node)

    def test_second_boolean_call_is_not_mislabeled(self):
        # A future second Boolean call in the predicate grammar must raise
        # PredicateError here instead of being labeled "str_contains".
        with self.assertRaises(PredicateError):
            _to_validator_ast(("endswith", ("ident", "x"), "p"))


if __name__ == "__main__":
    unittest.main()
