#!/usr/bin/env python3
"""Tests for mapping_doc.py: the human-review "Mapping spec" dashboard section."""

import unittest
from pathlib import Path

import yaml

import mapping_doc

HERE = Path(__file__).resolve().parent
BENCHMARKS = HERE.parents[2] / "benchmarks"


def load_spec(name):
    return yaml.safe_load((BENCHMARKS / name / "spec.yaml").read_text(encoding="utf-8"))


def mapping_sheet(name):
    sheets = {
        tab_id: (headers, rows)
        for tab_id, _, headers, rows in mapping_doc.mapping_sheets(load_spec(name))
    }
    return sheets["mapping"]


class SdtmMappingTests(unittest.TestCase):
    def test_headers_follow_the_sdtm_variable_sheet(self):
        headers, rows = mapping_sheet("sdtm-dm-basic")
        self.assertEqual(headers, mapping_doc.SDTM_HEADERS)
        names = [row[0] for row in rows]
        self.assertEqual(
            names,
            [
                "DOMAIN",
                "STUDYID",
                "USUBJID",
                "SUBJID",
                "SEX",
                "AGE",
                "ARM",
                "ACTARM",
                "ARMNRS",
            ],
        )

    def test_origin_falls_back_to_define_xml_vocabulary(self):
        _, rows = mapping_sheet("sdtm-dm-basic")
        origin = {row[0]: row[5] for row in rows}
        self.assertEqual(origin["DOMAIN"], "Assigned")
        self.assertEqual(origin["STUDYID"], "Collected")
        self.assertEqual(origin["SUBJID"], "Derived")
        self.assertEqual(origin["ARMNRS"], "Derived")

    def test_sex_recode_rule_is_plain_language(self):
        _, rows = mapping_sheet("sdtm-dm-basic")
        rule = {row[0]: row[7] for row in rows}["SEX"]
        self.assertIn("Recode ODM.Value", rule)
        self.assertIn('"Male"', rule)
        self.assertIn('"U"', rule)

    def test_submission_block_fills_the_variable_sheet_columns(self):
        _, rows = mapping_sheet("sdtm-dm-metadata")
        by_name = {row[0]: row for row in rows}
        domain = by_name["DOMAIN"]
        self.assertEqual(domain[2], "Char")  # Type
        self.assertEqual(domain[3], "2")  # Length
        self.assertEqual(domain[4], "DOMAIN")  # Controlled Terms or Format
        self.assertEqual(domain[5], "Assigned")  # Origin
        self.assertEqual(domain[6], "Req")  # Core
        self.assertEqual(domain[8], "SDTM")  # Variable Type
        self.assertEqual(domain[9], "1")  # Variable Order
        age = by_name["AGE"]
        self.assertEqual(age[2], "Num")
        self.assertEqual(age[3], "3")
        self.assertEqual(age[6], "Exp")

    def test_variable_type_marks_supplemental_domains(self):
        _, rows = mapping_sheet("sdtm-suppmh-qualifiers")
        self.assertTrue(all(row[8] == "SUPP" for row in rows))  # Variable Type

    def test_authored_method_becomes_the_conversion_definition(self):
        _, rows = mapping_sheet("sdtm-dm-metadata")
        usubjid = {row[0]: row for row in rows}["USUBJID"]
        self.assertIn("joined by hyphens", usubjid[7])

    def test_comment_travels_to_comments_for_define(self):
        _, rows = mapping_sheet("sdtm-dm-metadata")
        ageu = {row[0]: row for row in rows}["AGEU"]
        self.assertIn("Defaulted to YEARS", ageu[10])

    def test_mapping_rule_states_missing_and_unlisted_answers_separately(self):
        spec = load_spec("schema-text-mapping-unmapped")
        sheets = dict(
            (tab_id, (headers, rows))
            for tab_id, _, headers, rows in mapping_doc.mapping_sheets(spec)
        )
        _, rows = sheets["mapping"]
        rule = {row[1]: row[6] for row in rows}
        arrow = mapping_doc.ARROW
        self.assertIn(
            '; missing values stay missing; unlisted values ' + arrow
            + ' "Outside codelist".',
            rule["SEXC"],
        )
        self.assertIn(
            '; missing or unlisted values ' + arrow + ' "Unknown".',
            rule["SEXC_SINGLE"],
        )

    def test_strict_mapping_rule_names_the_event_that_errors(self):
        rule = mapping_doc.describe_mapping(
            {
                "source": "RS.OVRLRESP",
                "dict": {"CR": "COMPLETE RESPONSE"},
                "missing": "NOT DONE",
                "strict": True,
            }
        )
        self.assertIn(
            '; missing values ' + mapping_doc.ARROW
            + ' "NOT DONE"; unlisted values are errors.',
            rule,
        )


class AdamMappingTests(unittest.TestCase):
    def test_headers_follow_the_adam_variable_sheet(self):
        headers, rows = mapping_sheet("adam-adae-death")
        self.assertEqual(headers, mapping_doc.ADAM_HEADERS)
        self.assertTrue(all(row[0] == "ADAE" for row in rows))  # Dataset Name

    def test_type_uses_adam_words(self):
        _, rows = mapping_sheet("adam-adae-death")
        by_name = {row[1]: row for row in rows}
        self.assertEqual(by_name["AESEQ"][3], "numeric")
        self.assertEqual(by_name["AEDECOD"][3], "character")


class AdamDetectionTests(unittest.TestCase):
    def test_registered_adam_dataset_is_adam(self):
        self.assertTrue(mapping_doc.is_adam(load_spec("adam-adae-death")))

    def test_sdtm_domain_is_not_adam(self):
        self.assertFalse(mapping_doc.is_adam(load_spec("sdtm-dm-basic")))

    def test_unregistered_ad_prefixed_domain_raises(self):
        with self.assertRaises(ValueError):
            mapping_doc.is_adam({"domain": "ADHOC"})

    def test_missing_domain_defaults_to_sdtm(self):
        self.assertFalse(mapping_doc.is_adam({}))


class SheetStructureTests(unittest.TestCase):
    def test_adlb_row_construction_keeps_paramcd_and_param_separate(self):
        sheets = {
            tab_id: (headers, rows)
            for tab_id, _, headers, rows in mapping_doc.mapping_sheets(
                load_spec("adam-adlb-bds")
            )
        }
        headers, rows = sheets["row-construction"]
        self.assertEqual(headers[:4], ["Template", "Include when", "PARAMCD", "PARAM"])
        templates = {row[0]: row for row in rows}
        self.assertIn("ALTSI", templates["alt_si"][2])
        self.assertIn("Alanine Aminotransferase (SI)", templates["alt_si"][3])

    def test_rendering_is_deterministic(self):
        spec = load_spec("sdtm-dm-basic")
        self.assertEqual(
            mapping_doc.render_mapping_section(spec),
            mapping_doc.render_mapping_section(spec),
        )

    def test_section_has_matching_tabs_and_panes(self):
        spec = load_spec("adam-adlb-bds")
        section = mapping_doc.render_mapping_section(spec)
        self.assertIn('id="mapping-spec"', section)
        self.assertIn('role="tablist"', section)
        for tab_id in ("mapping", "row-construction", "revision-history"):
            self.assertIn('id="mapping-tab-' + tab_id + '"', section)
            self.assertIn('id="mapping-pane-' + tab_id + '"', section)
            self.assertIn('aria-controls="mapping-pane-' + tab_id + '"', section)
        self.assertIn('aria-selected="true"', section)

    def test_section_encodes_to_ascii_like_the_dashboard(self):
        spec = load_spec("sdtm-dm-basic")
        section = mapping_doc.render_mapping_section(spec)
        encoded = section.encode("ascii", "xmlcharrefreplace").decode("ascii")
        self.assertIn("&#8594;", encoded)
        self.assertNotIn("&amp;#", encoded)


if __name__ == "__main__":
    unittest.main()
