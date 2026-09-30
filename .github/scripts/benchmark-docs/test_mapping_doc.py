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


def load_define(name):
    return yaml.safe_load(
        (BENCHMARKS / name / "define.yaml").read_text(encoding="utf-8")
    )


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

    def test_study_document_supplies_controlled_terms_and_all_codelists(self):
        sheets = {
            tab_id: (headers, rows)
            for tab_id, _, headers, rows in mapping_doc.mapping_sheets(
                load_spec("sdtm-dm-metadata"), load_define("sdtm-dm-metadata")
            )
        }
        _, mapping = sheets["mapping"]
        terms = {row[0]: row[4] for row in mapping}
        self.assertEqual(terms["DOMAIN"], "Domain Abbreviation (DM)")
        self.assertEqual(terms["SEX"], "Sex")
        self.assertEqual(terms["AGEU"], "Age Unit")
        self.assertEqual(terms["COUNTRY"], "Country Codes (ISO 3166, version 2020)")
        headers, rows = sheets["codelists"]
        self.assertEqual(
            [row[0] for row in rows], ["DOMAIN", "AGEU", "SEX", "SEX", "SEX", "COUNTRY"]
        )
        sex = next(row for row in rows if row[0] == "SEX" and row[2] == "F")
        self.assertEqual(sex[headers.index("Decode")], "Female")
        self.assertEqual(sex[headers.index("Codelist NCI alias")], "C66731")
        self.assertEqual(sex[headers.index("Value NCI alias")], "C16576")
        self.assertEqual(sex[headers.index("SAS format")], "$SEX")
        country = rows[-1]
        self.assertEqual(country[headers.index("Dictionary")], "ISO 3166")
        self.assertEqual(country[headers.index("Version")], "2020")

    def test_temporal_submission_types_use_iso_8601_without_a_codelist(self):
        for data_type in mapping_doc.TEMPORAL_SUBMISSION_TYPES:
            with self.subTest(data_type=data_type):
                col = {
                    "name": "DTC",
                    "type": "str",
                    "submission": {"data_type": data_type},
                }
                self.assertEqual(mapping_doc.controlled_terms(col, {}, {}), "ISO 8601")
        self.assertEqual(
            mapping_doc.controlled_terms({"name": "DTC", "type": "str"}, {}, {}),
            "",
        )
        self.assertEqual(
            mapping_doc.controlled_terms({"name": "DT", "type": "date"}, {}, {}),
            "ISO 8601",
        )
        self.assertEqual(
            mapping_doc.controlled_terms(
                {
                    "name": "DTC",
                    "type": "str",
                    "submission": {"data_type": "partialDate"},
                },
                {"DTC": "2020, 2021"},
                {},
            ),
            "2020, 2021",
        )
        self.assertEqual(
            mapping_doc.controlled_terms(
                {
                    "name": "DTC",
                    "type": "str",
                    "submission": {"data_type": "partialDate", "codelist": "DATE_CL"},
                },
                {},
                {"DATE_CL": {"name": "Date terms"}},
            ),
            "Date terms",
        )

    def test_allowed_values_remain_the_fallback_without_a_document(self):
        spec = {
            "domain": "DM",
            "columns": [
                {
                    "name": "SEX",
                    "type": "str",
                    "verifications": {"allowed_values": {"values": ["F", "M"]}},
                }
            ],
        }
        sheets = {
            tab_id: (headers, rows)
            for tab_id, _, headers, rows in mapping_doc.mapping_sheets(spec)
        }
        self.assertEqual(sheets["mapping"][1][0][4], "F, M")
        self.assertEqual(
            sheets["codelists"], (["Variable", "Permitted values"], [["SEX", "F, M"]])
        )

    def test_document_extensibility_and_extended_values_are_visible(self):
        define = {
            "codelists": [
                {
                    "id": "EXAMPLE",
                    "name": "Example",
                    "extensible": True,
                    "items": [
                        {"value": "X", "decode": "Example value", "extended": True}
                    ],
                }
            ]
        }
        headers, rows = next(
            (headers, rows)
            for tab_id, _, headers, rows in mapping_doc.mapping_sheets(
                {"domain": "DM"}, define
            )
            if tab_id == "codelists"
        )
        self.assertEqual(rows[0][headers.index("Extensible")], "Yes")
        self.assertEqual(rows[0][headers.index("Extended value")], "Yes")

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
        sheets = {
            tab_id: (headers, rows)
            for tab_id, _, headers, rows in mapping_doc.mapping_sheets(spec)
        }
        _, rows = sheets["mapping"]
        rule = {row[1]: row[6] for row in rows}
        arrow = mapping_doc.ARROW
        self.assertIn(
            "; missing values stay missing; unlisted values "
            + arrow
            + ' "Outside codelist".',
            rule["SEXC"],
        )
        self.assertIn(
            "; missing or unlisted values " + arrow + ' "Unknown".',
            rule["SEXC_SINGLE"],
        )

    def test_mapping_rule_names_a_dictionary_kept_in_a_file(self):
        spec = load_spec("schema-text-mapping")
        sheets = {
            tab_id: (headers, rows)
            for tab_id, _, headers, rows in mapping_doc.mapping_sheets(spec)
        }
        _, rows = sheets["mapping"]
        rule = {row[1]: row[6] for row in rows}
        self.assertEqual(
            "Recode DM.RACE using the dictionary in input/race_dict.yaml"
            "; missing values " + mapping_doc.ARROW + ' "Unknown"'
            "; unlisted values are errors.",
            rule["RACEC"],
        )

    def test_mapping_rule_names_the_event_with_no_handler_as_an_error(self):
        rule = mapping_doc.describe_mapping(
            {
                "source": "RS.OVRLRESP",
                "dict": {"CR": "COMPLETE RESPONSE"},
                "missing": "NOT DONE",
            }
        )
        self.assertIn(
            "; missing values "
            + mapping_doc.ARROW
            + ' "NOT DONE"; unlisted values are errors.',
            rule,
        )

    def test_mapping_rule_with_no_handlers_names_both_events_as_errors(self):
        rule = mapping_doc.describe_mapping(
            {"source": "RS.OVRLRESP", "dict": {"CR": "COMPLETE RESPONSE"}}
        )
        self.assertTrue(rule.endswith("; missing or unlisted values are errors."))


class AdamMappingTests(unittest.TestCase):
    def test_named_window_summary_keeps_the_reference_visible(self):
        self.assertEqual(
            mapping_doc.describe_derivation({"row_number": {"window": "VISIT_ORDER"}}),
            "Row number using window VISIT_ORDER.",
        )
        baseline = mapping_doc.describe_derivation(
            {
                "baseline_flag": {
                    "date": "ADT",
                    "reference_date": "TRTSDT",
                    "window": "BASELINE_GROUPS",
                }
            }
        )
        self.assertIn("ADT on or before TRTSDT using window BASELINE_GROUPS", baseline)

    def test_unknown_window_benchmark_still_renders_for_review(self):
        section = mapping_doc.render_mapping_section(
            load_spec("negative-unknown-window")
        )
        self.assertIn("Row number using window VISITS_ORDER.", section)

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


def method_column(name):
    """The Conversion Definition / Computational Method cell, by variable name."""
    spec = load_spec(name)
    headers, rows = mapping_sheet(name)
    index = headers.index(
        "Computational Method" if mapping_doc.is_adam(spec) else "Conversion Definition"
    )
    key = headers.index("Variable Name")
    return {row[key]: row[index] for row in rows}


class LookupResolutionTests(unittest.TestCase):
    """A reviewer reads the value, not the pipeline: an `intermediates:` id is
    an internal handle and must never reach a mapping cell."""

    def test_coded_term_states_the_merge_instead_of_naming_the_lookup(self):
        rule = method_column("sdtm-ae-coding")["AEDECOD"]
        self.assertNotIn("MEDDRA_CODING", rule)
        self.assertIn("Take PTNAME from the MEDDRA record", rule)
        self.assertIn("LLTNAME equals AE_RAW.AETERM", rule)
        self.assertIn('no match yields "NOT CODED"', rule)

    def test_selection_names_the_filter_order_and_kept_record(self):
        rule = method_column("adam-adae-death")["DTHCAUS"]
        self.assertNotIn("DEATHEV", rule)
        self.assertIn("the last AE record", rule)
        self.assertIn("matching on STUDYID and USUBJID", rule)
        self.assertIn("where AE.AEOUT = 'FATAL'", rule)
        self.assertIn("ordered by AE.ASTDT then AE.AESEQ", rule)

    def test_range_match_reads_as_an_inclusive_window(self):
        rule = method_column("sdtm-vs-epoch-from-subject-elements")["EPOCH"]
        self.assertNotIn("ELEM", rule)
        self.assertIn("VSDTM falls within SESTDTC to SEENDTC", rule)

    def test_lookup_reference_inside_a_predicate_is_resolved(self):
        rule = method_column("adam-adlb-end-of-treatment")["EOTFL"]
        self.assertNotIn("EOT_RANK", rule)
        self.assertIn('"Y" when EOT_SEQ = 1', rule)
        self.assertIn("EOT_SEQ taken from the LB record", rule)

    def test_absent_no_match_is_reported_as_an_error_not_a_blank(self):
        rule = method_column("adam-adcm-atc-classes")["ATC1CD"]
        self.assertIn("no match yields blank", rule)
        self.assertIn(
            "a row with no match is an error",
            method_column("adam-adlb-end-of-treatment")["ENDPOINT"],
        )

    def test_value_read_through_a_lookup_is_assigned_not_collected(self):
        headers, rows = mapping_sheet("sdtm-ae-coding")
        origin = {row[0]: row[headers.index("Origin")] for row in rows}
        self.assertEqual(origin["AETERM"], "Collected")
        self.assertEqual(origin["AEDECOD"], "Assigned")

    def test_a_spec_without_lookups_keeps_the_plain_copy_wording(self):
        self.assertEqual(
            mapping_doc.describe_derivation("DM.SEX"), "Copy value from DM.SEX."
        )


class DerivationShapeTests(unittest.TestCase):
    """Every expression the corpus uses has a sentence; a shape with none would
    render its YAML into a cell a reviewer is asked to sign."""

    def test_no_benchmark_renders_raw_yaml_into_a_method_cell(self):
        raw = []
        for spec_path in sorted(BENCHMARKS.glob("*/spec.yaml")):
            name = spec_path.parent.name
            for variable, rule in method_column(name).items():
                if rule.startswith(("{", "[")):
                    raw.append(name + "/" + variable)
        self.assertEqual(raw, [])

    def test_flag_states_all_three_outcomes(self):
        self.assertEqual(
            mapping_doc.describe_derivation({"flag": "DTHDT IS NOT NULL"}),
            '"Y" when DTHDT IS NOT NULL; blank otherwise.',
        )
        rule = mapping_doc.describe_derivation(
            {"flag": {"condition": "AVAL > 0", "false_value": "N", "missing": "U"}}
        )
        self.assertIn('"Y" when AVAL > 0', rule)
        self.assertIn('"N" otherwise', rule)
        self.assertIn('"U" when the condition is unknown', rule)

    def test_case_otherwise_branch_carries_its_own_value(self):
        rule = mapping_doc.describe_derivation(
            {
                "case": [
                    {"when": "X = 1", "then": {"literal": "Y"}},
                    {"otherwise": {"literal": "N"}},
                ]
            }
        )
        self.assertEqual(rule, 'If X = 1 then "Y"; otherwise "N".')

    def test_aggregate_names_the_records_it_summarises(self):
        rule = method_column("adam-adex-cumulative-dose")["DOSECUM"]
        self.assertIn("SUM(EX.EXDOSE)", rule)
        self.assertIn("matching on the shared output keys", rule)

    def test_date_diff_states_which_endpoints_are_counted(self):
        rule = mapping_doc.describe_derivation(
            {
                "date_diff": {
                    "start": "BRTHDT",
                    "end": "RANDDT",
                    "unit": "year",
                    "bounds": "exclusive",
                }
            }
        )
        self.assertEqual(
            rule,
            "Whole years from BRTHDT to RANDDT, counting the end date but not the start.",
        )

    def test_cut_names_the_closed_side_of_each_interval(self):
        rule = method_column("adam-adsl-demographics")["AGEGR1"]
        self.assertIn("intervals closed on the left", rule)
        self.assertIn('"18-64"', rule)

    def test_handled_expression_reports_the_unconvertible_value(self):
        rule = method_column("adam-adsl-demographics")["AGE"]
        self.assertIn("cannot be converted", rule)


class RevisionHistoryTests(unittest.TestCase):
    def test_history_tracks_the_renderer_newest_first(self):
        sheets = {
            tab_id: (headers, rows)
            for tab_id, _, headers, rows in mapping_doc.mapping_sheets(
                load_spec("sdtm-ae-coding")
            )
        }
        headers, rows = sheets["revision-history"]
        self.assertEqual(headers, mapping_doc.REVISION_HISTORY_HEADERS)
        versions = [row[0] for row in rows]
        self.assertEqual(versions, sorted(versions, reverse=True))
        self.assertEqual(rows[-1][3], "Initial generation from spec.yaml")
        for row in rows:
            self.assertEqual(row[2], mapping_doc.RENDERER_AUTHOR)
            # Reviewer and Sign-off belong to the reviewer, not the build.
            self.assertEqual(row[4:], ["", ""])

    def test_every_benchmark_shows_the_same_renderer_history(self):
        for name in ("sdtm-ae-coding", "adam-adlb-bds", "sdtm-dm-basic"):
            sheets = {
                tab_id: rows
                for tab_id, _, _, rows in mapping_doc.mapping_sheets(load_spec(name))
            }
            self.assertEqual(
                sheets["revision-history"], mapping_doc.revision_history_rows()
            )


class ExcelProseTests(unittest.TestCase):
    """A variable sheet is written in sentences. A bracketed aside is a
    programmer's habit, so the describers add none of their own -- a
    parenthesis in a cell can only have come from the spec's own text."""

    def test_no_describer_invents_a_parenthesis(self):
        invented = []
        for spec_path in sorted(BENCHMARKS.glob("*/spec.yaml")):
            name = spec_path.parent.name
            source = spec_path.read_text(encoding="utf-8")
            for variable, rule in method_column(name).items():
                if "(" in rule and "(" not in source:
                    invented.append(name + "/" + variable + ": " + rule)
        self.assertEqual(invented, [])

    def test_a_lookup_states_its_no_match_answer_as_a_clause(self):
        rule = method_column("sdtm-ae-coding")["AEDECOD"]
        self.assertNotIn("(", rule)
        self.assertTrue(rule.endswith('no match yields "NOT CODED".'), rule)


class NoDerivationTests(unittest.TestCase):
    """A column can reach the variable sheet with no derivation of its own.
    The cell must say where the value comes from, not print Python's None."""

    def test_row_template_column_points_at_the_row_construction_sheet(self):
        rules = method_column("adam-adlb-bds")
        self.assertEqual(rules["PARAMCD"], mapping_doc.ROW_TEMPLATE_RULE)
        self.assertEqual(rules["AVAL"], mapping_doc.ROW_TEMPLATE_RULE)

    def test_inherited_column_says_so(self):
        self.assertEqual(
            method_column("negative-property-clear")["USUBJID"],
            mapping_doc.INHERITED_RULE,
        )

    def test_no_benchmark_prints_none_into_a_method_cell(self):
        empty = []
        for spec_path in sorted(BENCHMARKS.glob("*/spec.yaml")):
            name = spec_path.parent.name
            for variable, rule in method_column(name).items():
                if rule in ("None", ""):
                    empty.append(name + "/" + variable)
        self.assertEqual(empty, [])

    def test_a_nested_lookup_answers_in_the_short_form(self):
        rule = method_column("adam-adlb-end-of-treatment")["ENDPOINT"]
        # The inner lookup's answer must not read as the outer lookup's.
        self.assertIn("blank when unmatched", rule)
        self.assertTrue(rule.endswith("a row with no match is an error."), rule)
