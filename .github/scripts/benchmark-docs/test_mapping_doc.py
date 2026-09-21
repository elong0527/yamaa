#!/usr/bin/env python3
"""Tests for mapping_doc.py: the human-review "Derived Excel Specification" dashboard section."""

import unittest
from pathlib import Path

import yaml

import mapping_doc


HERE = Path(__file__).resolve().parent
BENCHMARKS = HERE.parents[2] / "benchmarks"


def load_spec(name):
    return yaml.safe_load((BENCHMARKS / name / "spec.yaml").read_text(encoding="utf-8"))


class MappingSheetsTests(unittest.TestCase):
    def test_dm_mapping_rows_cover_every_output_column(self):
        spec = load_spec("sdtm-dm-basic")
        sheets = dict(
            (tab_id, (headers, rows))
            for tab_id, _, headers, rows in mapping_doc.mapping_sheets(spec)
        )
        headers, rows = sheets["mapping"]
        self.assertEqual(headers[1], "Target variable")
        names = [row[1] for row in rows]
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

    def test_dm_origins_follow_define_xml_vocabulary(self):
        spec = load_spec("sdtm-dm-basic")
        sheets = dict(
            (tab_id, (headers, rows))
            for tab_id, _, headers, rows in mapping_doc.mapping_sheets(spec)
        )
        _, rows = sheets["mapping"]
        origin = {row[1]: row[7] for row in rows}
        self.assertEqual(origin["DOMAIN"], "Assigned")
        self.assertEqual(origin["STUDYID"], "Collected")
        self.assertEqual(origin["SUBJID"], "Derived")
        self.assertEqual(origin["ARMNRS"], "Derived")

    def test_dm_provenance_chain_links_output_columns(self):
        spec = load_spec("sdtm-dm-basic")
        sheets = dict(
            (tab_id, (headers, rows))
            for tab_id, _, headers, rows in mapping_doc.mapping_sheets(spec)
        )
        _, rows = sheets["mapping"]
        by_name = {row[1]: row for row in rows}
        self.assertIn("USUBJID (output column)", by_name["SUBJID"][5])
        self.assertIn("ODM.SubjectKey", by_name["SUBJID"][5])
        self.assertIn("IT.DM.SEX", by_name["SEX"][5])

    def test_dm_sex_recode_rule_is_plain_language(self):
        spec = load_spec("sdtm-dm-basic")
        sheets = dict(
            (tab_id, (headers, rows))
            for tab_id, _, headers, rows in mapping_doc.mapping_sheets(spec)
        )
        _, rows = sheets["mapping"]
        rule = {row[1]: row[6] for row in rows}["SEX"]
        self.assertIn("Recode ODM.Value", rule)
        self.assertIn('"Male"', rule)
        self.assertIn('"U"', rule)

    def test_adlb_row_construction_keeps_paramcd_and_param_separate(self):
        spec = load_spec("adam-adlb-bds")
        sheets = dict(
            (tab_id, (headers, rows))
            for tab_id, _, headers, rows in mapping_doc.mapping_sheets(spec)
        )
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
        self.assertIn("&#8212;", encoded)
        self.assertNotIn("&amp;#", encoded)


if __name__ == "__main__":
    unittest.main()
