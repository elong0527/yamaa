"""Reading a file's language and a 1.x expression field (#3030 P2-3)."""

import unittest

from tools import surface


class LanguageTests(unittest.TestCase):
    def test_a_root_declaring_yamaa_and_no_schema_version_is_1_x(self):
        self.assertTrue(surface.declares_header({"yamaa": "1.0", "type": "spec"}))
        self.assertTrue(surface.declares_header({"yamaa": "1.0"}))

    def test_any_other_root_is_read_as_0_5(self):
        # As the engine routes a root (language.rs::declares_header).
        for document in (
            {"schema_version": "1.0"},
            {"schema_version": "1.0", "yamaa": "1.0"},
            {"domain": "DM"},
            None,
            [],
            "yamaa",
        ):
            with self.subTest(document=document):
                self.assertFalse(surface.declares_header(document), document)


class ExpressionTests(unittest.TestCase):
    def test_a_bare_or_qualified_name_is_a_reference(self):
        for text in ("AGE", "DM.AGE", "_X1", "DM.NULL", " DM.AGE "):
            with self.subTest(text=text):
                self.assertEqual(surface.expression(text), ("reference", text.strip()))

    def test_a_reserved_word_is_no_bare_reference(self):
        self.assertEqual(surface.expression("NULL"), ("literal", None))
        self.assertEqual(surface.expression("true"), ("literal", True))
        self.assertEqual(surface.expression("FALSE"), ("literal", False))
        self.assertFalse(surface.is_reference("and"))
        self.assertEqual(surface.expression("AND"), ("formula", "AND"))

    def test_quoted_and_temporal_text_are_literals(self):
        self.assertEqual(surface.expression("'DM'"), ("literal", "DM"))
        self.assertEqual(
            surface.expression("'Bazett''s formula'"), ("literal", "Bazett's formula")
        )
        self.assertEqual(surface.expression("''"), ("literal", ""))
        self.assertEqual(
            surface.expression("DATE '2025-01-31'"), ("literal", "2025-01-31")
        )
        self.assertEqual(
            surface.expression("DATETIME '2025-02-01T09:30'"),
            ("literal", "2025-02-01T09:30"),
        )
        self.assertEqual(surface.expression("1.5e3"), ("literal", "1.5e3"))

    def test_yaml_scalars_are_literals(self):
        for value in (0, 1.5, True, False, None):
            with self.subTest(value=value):
                self.assertEqual(surface.expression(value), ("literal", value))

    def test_other_text_is_a_formula_and_a_mapping_an_operation(self):
        for text in ("A / B", "'A' || 'B'", "-1", "UPPER(DM.SEX)", "'it''s' = X"):
            with self.subTest(text=text):
                self.assertEqual(surface.expression(text), ("formula", text))
        operation = {"study_day": {"source": "ADT", "reference": "TRTSDT"}}
        self.assertEqual(surface.expression(operation), ("operation", operation))


if __name__ == "__main__":
    unittest.main()
