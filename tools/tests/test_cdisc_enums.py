"""Core's enums of the closed CDISC vocabularies, generated from the data file."""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml
from tools.commands import repository
from tools.packaging import generate_cdisc_vocabulary as generator
from tools.paths import ROOT
from tools.tests import support

SPELLED = "yaml/vocabulary/cdisc.yaml.vocabularies"


def entries(*spellings):
    return [{"value": value, "since": "1.0"} for value in spellings]


class CdiscEnumTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def write(self, text):
        support.write(self.root, generator.VOCABULARY, text)

    def test_the_committed_module_is_the_data_file_s(self):
        self.assertEqual(
            generator.TARGET.read_text(encoding="ascii"), generator.render()
        )

    def test_the_repository_checks_run_it(self):
        self.assertIn("tools.packaging.generate_cdisc_vocabulary", repository.CHECKS)

    def test_a_variant_is_its_value_s_words_in_upper_camel_case(self):
        for value, variant in (
            ("Not Available", "NotAvailable"),
            ("partialDate", "PartialDate"),
            ("PhysicalRef", "PhysicalRef"),
            ("URI", "Uri"),
            ("NOTIN", "Notin"),
            ("ADaM", "ADaM"),
            ("TIME-TO-EVENT", "TimeToEvent"),
            ("CDISC/NCI", "CdiscNci"),
            ("ISO8601", "Iso8601"),
        ):
            with self.subTest(case=value):
                self.assertEqual(generator.variant(value), variant)

    def test_an_enum_spells_each_value_in_the_data_file_s_order(self):
        vocabulary = {
            "field": "where_comparison_class.comparator",
            "values": entries("NE", "NOTIN"),
        }
        self.assertEqual(
            generator.enum("comparator", vocabulary),
            [
                "",
                "/// The `comparator` vocabulary, which the class field",
                "/// `where_comparison_class.comparator` spells.",
                "#[rustfmt::skip]",
                "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
                "pub enum WhereComparator {",
                "    Ne,",
                "    Notin,",
                "}",
                "",
                "#[rustfmt::skip]",
                "impl WhereComparator {",
                "    /// Every value with its spelling, in the data file's order.",
                "    pub const VALUES: &'static [(&'static str, Self)] = &[",
                '        ("NE", Self::Ne),',
                '        ("NOTIN", Self::Notin),',
                "    ];",
                "    /// The value's spelling.",
                "    pub fn name(self) -> &'static str {",
                "        match self {",
                '            Self::Ne => "NE",',
                '            Self::Notin => "NOTIN",',
                "        }",
                "    }",
                "    /// The value a spelling names, or `None` for any other text.",
                "    pub fn from_name(name: &str) -> Option<Self> {",
                "        Self::VALUES.iter().find(|(spelling, _)| *spelling == name)"
                ".map(|&(_, value)| value)",
                "    }",
                "}",
            ],
        )

    def test_an_enum_names_the_schema_type_and_enumeration_that_spell_it(self):
        self.assertEqual(
            generator.doc(
                "dataset_subclass",
                {"type": "dataset_subclass_name", "enumeration": "ItemGroupSubClass"},
            ),
            [
                "/// The `dataset_subclass` vocabulary, which the schema type",
                "/// `dataset_subclass_name` spells, with the values of Define-XML's",
                "/// `ItemGroupSubClass`.",
            ],
        )

    def test_values_that_name_no_variant_or_one_variant_twice_are_refused(self):
        values = entries("Not Available", "NotAvailable", "2D", "self")
        with self.assertRaises(SystemExit) as raised:
            generator.variants("model", {"values": values})
        self.assertEqual(
            raised.exception.code,
            "\n".join(
                [
                    f"ERROR: {SPELLED}.model: 'Not Available' and 'NotAvailable' "
                    "both name NotAvailable",
                    f"ERROR: {SPELLED}.model: '2D' names no variant",
                    f"ERROR: {SPELLED}.model: 'self' names no variant",
                ]
            ),
        )

    def test_a_spelling_is_a_rust_literal_of_printable_ascii(self):
        self.assertEqual(generator.literal('a "b" \\ c'), '"a \\"b\\" \\\\ c"')
        with self.assertRaises(SystemExit) as raised:
            generator.literal("caf\u00e9")
        self.assertEqual(
            raised.exception.code, "ERROR: 'caf\\xe9' is not printable ASCII"
        )

    def test_each_vocabulary_has_an_enum_and_each_enum_a_vocabulary(self):
        document = support.load_yaml(ROOT / generator.VOCABULARY)
        document["vocabularies"] = {
            ("column_role" if name == "role" else name): vocabulary
            for name, vocabulary in document["vocabularies"].items()
        }
        self.write(yaml.safe_dump(document, sort_keys=False))
        with self.assertRaises(SystemExit) as raised:
            generator.vocabularies(self.root)
        self.assertEqual(
            raised.exception.code,
            f"ERROR: {SPELLED}.column_role: no enum in TYPES holds it\n"
            "ERROR: TYPES.role: yaml/vocabulary/cdisc.yaml has no such vocabulary",
        )

    def test_a_malformed_data_file_is_refused_with_its_errors(self):
        self.write("version: '1.0'\nsource: {}\nvocabularies: {}\n")
        with self.assertRaises(SystemExit) as raised:
            generator.vocabularies(self.root)
        self.assertEqual(
            raised.exception.code,
            "ERROR: yaml/vocabulary/cdisc.yaml.source: expected exactly "
            "['define_xml', 'terminology']\n"
            "ERROR: yaml/vocabulary/cdisc.yaml.vocabularies: expected a mapping",
        )

    def test_the_check_refuses_a_stale_module_and_write_regenerates_it(self):
        target = self.root / "cdisc_vocabulary.rs"
        target.write_text("// stale\n", encoding="ascii")
        with mock.patch.object(generator, "TARGET", target):
            self.assertEqual(
                support.run_main(generator.main),
                (
                    "ERROR: rust/crates/yamaa-core/src/cdisc_vocabulary.rs differs "
                    "from yaml/vocabulary/cdisc.yaml; regenerate it with python -m "
                    "tools.packaging.generate_cdisc_vocabulary --write",
                    "",
                    "",
                ),
            )
            self.assertEqual(support.run_main(generator.main, "--write"), (0, "", ""))
            self.assertEqual(target.read_text(encoding="ascii"), generator.render())
            self.assertEqual(
                support.run_main(generator.main),
                (
                    0,
                    "PASS: core's CDISC vocabularies are "
                    "yaml/vocabulary/cdisc.yaml's\n",
                    "",
                ),
            )


if __name__ == "__main__":
    unittest.main()
