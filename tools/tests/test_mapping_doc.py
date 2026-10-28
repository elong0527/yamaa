"""Tests for mapping_doc.py: the "Mapping spec" pane of a benchmark dashboard."""

import functools
import unittest
from pathlib import Path

import yaml
from tools.benchmark_docs import generate, mapping_doc
from tools.paths import ROOT

HERE = Path(__file__).resolve().parent
BENCHMARKS = ROOT / "benchmarks"


def load_spec(name):
    return yaml.safe_load((BENCHMARKS / name / "spec.yaml").read_text(encoding="utf-8"))


def load_project_codelists(name):
    return generate.project_codelists(BENCHMARKS / name)


def rendered_tables(spec):
    """Every sheet's table HTML, joined, for assertions across sheets."""
    return "".join(table for _, _, table in mapping_doc.render_mapping_sheets(spec))


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
                load_spec("sdtm-dm-metadata"),
                load_project_codelists("sdtm-dm-metadata"),
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
                    "verifications": [{"allowed_values": {"values": ["F", "M"]}}],
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

    def test_mapping_rule_with_no_handlers_keeps_missing_and_fails_unlisted(self):
        # An omitted `missing` gives the missing value; an omitted
        # `unmapped` fails (COLUMN-10).
        rule = mapping_doc.describe_mapping(
            {"source": "RS.OVRLRESP", "dict": {"CR": "COMPLETE RESPONSE"}}
        )
        self.assertTrue(
            rule.endswith("; missing values stay missing; unlisted values are errors.")
        )

    def test_decode_rule_names_its_codelist_and_both_answers(self):
        # A decode takes mapping's handlers: an omitted `missing` keeps the
        # missing value and an omitted `unmapped` fails (EXPR-15, COLUMN-10).
        arrow = mapping_doc.ARROW
        for node, expected in (
            (
                {"source": "PARAMCD", "codelist": "PARAMCD"},
                "Decode PARAMCD through codelist PARAMCD; missing values stay"
                " missing; unlisted values are errors.",
            ),
            (
                {"source": "LB.LBTESTCD", "codelist": "LBTEST", "unmapped": "Other"},
                "Decode LB.LBTESTCD through codelist LBTEST; missing values stay"
                " missing; unlisted values " + arrow + ' "Other".',
            ),
        ):
            with self.subTest(case=expected):
                self.assertEqual(
                    mapping_doc.describe_derivation({"decode": node}), expected
                )


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
                    "reference": "TRTSDT",
                    "source": "AVAL",
                    "window": "BASELINE_GROUPS",
                }
            }
        )
        self.assertIn(
            "ADT on or before TRTSDT and a non-missing AVAL using window "
            "BASELINE_GROUPS, ordered by ADT;",
            baseline,
        )
        exclusive = mapping_doc.describe_derivation(
            {
                "baseline_flag": {
                    "date": "ADT",
                    "reference": "TRTSDT",
                    "source": "AVAL",
                    "inclusive": False,
                    "window": {
                        "group_by": ["USUBJID"],
                        "order_by": [
                            "ADT",
                            {"variable": "ATM", "direction": "desc"},
                            "ASEQ",
                        ],
                    },
                }
            }
        )
        self.assertIn(
            "ADT before TRTSDT and a non-missing AVAL within each group of "
            "USUBJID, ordered by ADT then ATM descending then ASEQ;",
            exclusive,
        )

    def test_unknown_window_benchmark_still_renders_for_review(self):
        tables = rendered_tables(load_spec("negative-unknown-window"))
        self.assertIn("Row number using window VISITS_ORDER.", tables)

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

    def test_a_template_cell_tells_a_written_null_from_an_absent_variable(self):
        # A written null is the missing literal (#3030 D18); a variable that
        # only another template fills is absent from this one.
        spec = {
            "yamaa": "1.0",
            "type": "spec",
            "domain": "DM",
            "keys": ["STUDYID", "USUBJID"],
            "input": {"RAW": "raw.csv"},
            "output": {"path": "dm.csv", "columns": ["USUBJID", "AGE", "AGEGR1"]},
            "columns": [
                {"name": "USUBJID", "type": "str", "derivation": "RAW.USUBJID"},
                {"name": "AGE", "type": "int"},
                {"name": "AGEGR1", "type": "str"},
            ],
            "rows": [
                {"id": "a", "derivations": {"AGE": None}},
                {"id": "b", "derivations": {"AGE": 1, "AGEGR1": "RAW.AGEGRP"}},
            ],
        }
        _, _, _, rows = mapping_doc.mapping_sheets(spec)[1]
        self.assertEqual(rows[0][2:], ['Constant "None"', "None"])

    def test_rendering_is_deterministic(self):
        spec = load_spec("sdtm-dm-basic")
        self.assertEqual(
            mapping_doc.render_mapping_sheets(spec),
            mapping_doc.render_mapping_sheets(spec),
        )

    def test_each_sheet_renders_its_own_labelled_table(self):
        sheets = mapping_doc.render_mapping_sheets(load_spec("adam-adlb-bds"))
        self.assertEqual(
            [(sheet_id, label) for sheet_id, label, _ in sheets],
            [
                ("mapping", "Mapping"),
                ("row-construction", "Row construction"),
                ("revision-history", "Revision history"),
            ],
        )
        for sheet_id, _, table in sheets:
            self.assertIn('aria-label="Mapping spec ' + sheet_id + ' table"', table)
            self.assertEqual(table.count('<table class="data-table mapping-table">'), 1)

    def test_sheets_encode_to_ascii_like_the_dashboard(self):
        tables = rendered_tables(load_spec("sdtm-dm-basic"))
        encoded = tables.encode("ascii", "xmlcharrefreplace").decode("ascii")
        self.assertIn("&#8594;", encoded)
        self.assertNotIn("&amp;#", encoded)

    def test_malformed_spec_fields_render_empty_sheets_instead_of_crashing(self):
        # A field written with no value parses as None, not as an absent
        # key; a column without a name and a non-mapping row template are
        # the same kind of authoring error. Each used to escape as a
        # TypeError/KeyError/AttributeError and abort the dashboard build.
        base = load_spec("sdtm-dm-basic")
        for spec in (
            {**base, "columns": None},
            {**base, "columns": 42},
            {**base, "columns": True},
            {**base, "columns": [{"name": ["DOMAIN"], "type": "str"}]},
            {
                **base,
                "output": {"columns": ["DOMAIN"]},
                "columns": [{"name": {"id": "DOMAIN"}, "type": "str"}],
            },
            {**base, "intermediates": 7},
            {
                **base,
                "columns": [
                    {
                        "name": "DOMAIN",
                        "type": ["str"],
                        "submission": {"data_type": ["text"], "codelist": ["CL"]},
                        "verifications": 3,
                    }
                ],
            },
            {
                **base,
                "columns": [
                    {
                        "name": "DOMAIN",
                        "type": "str",
                        "verifications": [{"allowed_values": 5}],
                    }
                ],
            },
            {
                **base,
                "output": None,
                "columns": [{"type": "str", "label": "No name"}],
            },
            {**base, "rows": ["not-a-template"]},
            {**base, "rows": 1},
            {**base, "output": {"columns": [{"name": "STUDYID"}]}},
            {**base, "output": {"columns": "STUDYID"}},
            {**base, "rows": [{"id": "t1", "derivations": ["STUDYID"]}]},
            None,
        ):
            with self.subTest(spec=spec):
                sheets = mapping_doc.mapping_sheets(spec)
                self.assertEqual(sheets[0][0], "mapping")
                self.assertIsInstance(rendered_tables(spec), str)
        for define in (
            {"codelists": 9},
            {"codelists": [{"id": "C1", "items": 5}]},
            {"codelists": [{"id": ["C1"]}]},
        ):
            with self.subTest(define=define):
                sheets = mapping_doc.mapping_sheets(base, define)
                self.assertEqual(sheets[0][0], "mapping")


if __name__ == "__main__":
    unittest.main()


@functools.cache
def spec_sheets(path):
    """A specification file and its sheets, read and rendered once per module:
    the corpus scans below read every benchmark's sheets, and none changes
    them."""
    spec = yaml.safe_load(path.read_text(encoding="utf-8"))
    return spec, mapping_doc.mapping_sheets(spec)


@functools.cache
def method_column(name):
    """The Conversion Definition / Computational Method cell, by variable name."""
    spec, sheets = spec_sheets(BENCHMARKS / name / "spec.yaml")
    [(headers, rows)] = [
        (headers, rows) for tab_id, _, headers, rows in sheets if tab_id == "mapping"
    ]
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

    def test_required_match_is_reported_as_an_error_not_a_blank(self):
        rule = method_column("adam-adcm-atc-classes")["ATC1CD"]
        self.assertIn("no match yields blank", rule)
        self.assertIn(
            "a row with no match is an error",
            method_column("adam-adlb-end-of-treatment")["ENDPOINT"],
        )

    def test_a_required_implicit_join_is_reported_as_an_error(self):
        ctx = mapping_doc.SpecContext(
            {
                "base": "AE",
                "input": {
                    "AE": "ae.csv",
                    "ADSL": {"path": "adsl.csv", "require_match": True},
                    "EX": "ex.csv",
                },
            }
        )
        self.assertEqual(
            mapping_doc.describe_derivation("ADSL.TRTSDT", ctx),
            "Copy value from ADSL.TRTSDT; a row with no matching record is an error.",
        )
        self.assertEqual(
            mapping_doc.describe_derivation("EX.EXDOSE", ctx),
            "Copy value from EX.EXDOSE.",
        )
        bound = {"source": {"variable": "EX.EXDOSE", "require_match": True}}
        self.assertIn(
            "a row with no matching record is an error",
            mapping_doc.describe_derivation(bound, ctx),
        )
        # A declared answer replaces the input's requirement for its read.
        answered = {"source": {"variable": "ADSL.TRTSDT", "absent": "NONE"}}
        self.assertNotIn("error", mapping_doc.describe_derivation(answered, ctx))

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

    def test_proper_case_names_its_declared_word_separators(self):
        # SAS's six separators are the default and go unstated; a declared
        # list replaces them, so the sheet quotes it (TEXT-5).
        for node, expected in (
            (
                {"source": "AETERM", "to": "proper"},
                "Convert AETERM to proper case.",
            ),
            (
                {"source": "AETERM", "to": "proper", "delimiters": " /-(.,["},
                'Convert AETERM to proper case, starting a word after any of " /-(.,[".',
            ),
        ):
            with self.subTest(case=node):
                self.assertEqual(
                    mapping_doc.describe_derivation({"str_case": node}), expected
                )

    def test_case_otherwise_branch_carries_its_own_value(self):
        rule = mapping_doc.describe_derivation(
            {
                "case": [
                    {"when": "X = 1", "then": "'Y'"},
                    {"otherwise": "'N'"},
                ]
            }
        )
        self.assertEqual(rule, 'If X = 1 then "Y"; otherwise "N".')

    def test_aggregate_names_the_records_it_summarises(self):
        rule = method_column("adam-adex-cumulative-dose")["DOSECUM"]
        self.assertIn("SUM(EX.EXDOSE)", rule)
        self.assertIn("matching on the shared output keys", rule)

    def test_no_spec_renders_a_temporal_verb_as_raw_yaml(self):
        # A verb nested in a case branch or a lookup sits mid-cell, where the
        # leading-brace check above cannot see it, and an entry spec need not
        # be spec.yaml; so every spec file and every cell is read.
        verbs = (
            "diff",
            "shift",
            "impute",
            "precision",
            "imputation_flag",
            "to_date",
            "dtc",
            "time_of_day",
            "duration_text",
            "date_text",
        )
        raw = []
        for benchmark in sorted(BENCHMARKS.iterdir()):
            for path in generate.benchmark_spec_files(benchmark):
                for _, _, _, rows in spec_sheets(path)[1]:
                    for row in rows:
                        for cell in row:
                            if any("{'" + verb + "'" in str(cell) for verb in verbs):
                                raw.append(benchmark.name + "/" + path.name)
        self.assertEqual(raw, [])

    def test_diff_of_dates_states_which_endpoints_are_counted(self):
        rule = mapping_doc.describe_derivation(
            {
                "diff": {
                    "type": "date",
                    "start": "BRTHDT",
                    "end": "RANDDT",
                    "unit": "year",
                    "closed": "right",
                }
            }
        )
        self.assertEqual(
            rule,
            "Whole years from BRTHDT to RANDDT, counting the end date but not the start.",
        )

    def test_diff_of_datetimes_states_its_sign_and_rounding(self):
        def rule(unit):
            return mapping_doc.describe_derivation(
                {
                    "diff": {
                        "type": "datetime",
                        "start": "FANLDTM",
                        "end": "ADTM",
                        "unit": unit,
                    }
                }
            )

        self.assertEqual(
            rule("hour"),
            "Elapsed hours from FANLDTM to ADTM, negative when ADTM is earlier, "
            "not rounded.",
        )
        self.assertEqual(
            rule("second"),
            "Whole seconds from FANLDTM to ADTM, negative when ADTM is earlier.",
        )

    def test_shift_names_how_far_and_which_way_the_value_moves(self):
        def rule(node):
            return mapping_doc.describe_derivation({"shift": node})

        self.assertEqual(
            rule({"type": "date", "source": "AP02SDT", "days": -1}),
            "The date 1 day before AP02SDT.",
        )
        self.assertEqual(
            rule({"type": "date", "source": "LASTEXDT", "days": 30}),
            "The date 30 days after LASTEXDT.",
        )
        self.assertEqual(
            rule({"type": "date", "source": "TRTSDT", "days": "OFFSET"}),
            "The date TRTSDT plus OFFSET days.",
        )
        self.assertEqual(
            rule(
                {"type": "datetime", "source": "EXSTDTC", "amount": 24, "unit": "hour"}
            ),
            "The datetime 24 hours after EXSTDTC.",
        )
        self.assertEqual(
            rule(
                {"type": "datetime", "source": "WINEND", "amount": -1, "unit": "second"}
            ),
            "The datetime 1 second before WINEND.",
        )

    def test_shift_reads_as_a_value_inside_a_case_branch(self):
        rule = mapping_doc.describe_derivation(
            {
                "case": [
                    {
                        "when": "AP02SDT IS NOT NULL",
                        "then": {
                            "shift": {"type": "date", "source": "AP02SDT", "days": -1}
                        },
                    },
                    {
                        "otherwise": {
                            "shift": {"type": "date", "source": "LASTEXDT", "days": 30}
                        }
                    },
                ]
            }
        )
        self.assertEqual(
            rule,
            "If AP02SDT IS NOT NULL then the date 1 day before AP02SDT; "
            "otherwise the date 30 days after LASTEXDT.",
        )

    def test_precision_names_the_words_of_its_own_type(self):
        def rule(kind):
            return mapping_doc.describe_derivation(
                {
                    "precision": {
                        "type": kind,
                        "source": "PC.PCDTC",
                        "invalid": None,
                    }
                }
            )

        blank = "; invalid text " + mapping_doc.ARROW + " blank."
        self.assertEqual(
            rule("datetime"),
            'Collected datetime precision of PC.PCDTC: "second" through the '
            'second, "minute" through the minute, "hour" through the hour, '
            '"day" for a date alone, "month" for a year and month, "year" for '
            'a year alone, "none" for an entirely imputed datetime or text '
            "with an unknown year" + blank,
        )
        self.assertEqual(
            rule("date"),
            'Collected date precision of PC.PCDTC: "day" for a complete date, '
            '"month" for a year and month, "year" for a year alone, "none" for '
            "an entirely imputed date or text with an unknown year" + blank,
        )

    def test_invalid_text_with_no_handler_is_an_error(self):
        for verb, node in (
            ("precision", {"type": "date", "source": "AESTDTC"}),
            ("impute", {"type": "date", "source": "AESTDTC", "month": 1, "day": 1}),
        ):
            with self.subTest(verb=verb):
                rule = mapping_doc.describe_derivation({verb: node})
                self.assertTrue(rule.endswith("; invalid text is an error."), rule)

    def test_impute_of_a_datetime_supplies_only_missing_time_parts(self):
        rule = mapping_doc.describe_derivation(
            {
                "impute": {
                    "type": "datetime",
                    "source": "AEENDTC",
                    "time": "last",
                    "invalid": None,
                }
            }
        )
        self.assertEqual(
            rule,
            "Impute incomplete datetime from AEENDTC, giving each time part the "
            "source lacks its latest value; a source without a complete date "
            "gives blank; invalid text " + mapping_doc.ARROW + " blank.",
        )

    def test_impute_states_what_its_month_and_day_rules_leave_blank(self):
        # The month and day rules a node declares state the minimum source
        # precision (TIME-10, P-IMP-1).
        def rule(**parts):
            node = {"type": "date", "source": "AESTDTC", **parts}
            return mapping_doc.describe_derivation({"impute": node})

        self.assertNotIn("gives blank", rule(month=6, day=15))
        self.assertIn("day=15; a year alone gives blank;", rule(day=15))
        self.assertIn("; a source without a complete date gives blank;", rule())

    def test_impute_bound_outside_the_collected_period_is_ignored(self):
        def rule(**bounds):
            return mapping_doc.describe_derivation(
                {
                    "impute": {
                        "type": "date",
                        "source": "AESTDTC",
                        "month": 1,
                        "day": "first",
                        **bounds,
                    }
                }
            )

        self.assertEqual(
            rule(not_before="TRTSDT", not_after="AENDT"),
            "Impute incomplete date from AESTDTC, defaulting month=1, day=first, "
            "never before TRTSDT, never after AENDT; a bound outside the "
            "collected period is ignored, and AENDT wins over a later TRTSDT; "
            "invalid text is an error.",
        )
        single = rule(not_after="DTHDT")
        self.assertIn(
            "never after DTHDT; a bound outside the collected period is ignored;",
            single,
        )
        self.assertNotIn("wins", single)

    def test_impute_bound_read_through_a_lookup_names_its_column_again(self):
        ctx = mapping_doc.SpecContext(
            {"intermediates": [{"id": "DEATH", "dataset": "DS", "absent": None}]}
        )
        rule = mapping_doc.describe_derivation(
            {
                "impute": {
                    "type": "date",
                    "source": "AEENDTC",
                    "month": 12,
                    "day": "last",
                    "not_before": "TRTSDT",
                    "not_after": "DEATH.DTHDT",
                }
            },
            ctx,
        )
        self.assertNotIn("DEATH.", rule)
        self.assertIn("never after DTHDT from the DS record", rule)
        self.assertIn(", and DTHDT wins over a later TRTSDT;", rule)

    def test_impute_fallback_counts_as_entirely_imputed(self):
        rule = mapping_doc.describe_derivation(
            {
                "impute": {
                    "type": "datetime",
                    "source": "AESTDTC",
                    "month": 1,
                    "day": "first",
                    "time": "first",
                    "fallback": "TRTSDTM",
                }
            }
        )
        self.assertEqual(
            rule,
            "Impute incomplete datetime from AESTDTC, defaulting month=1, "
            "day=first, giving each time part the source lacks its earliest "
            "value; a missing source takes TRTSDTM, counted as entirely "
            "imputed; invalid text is an error.",
        )

    def test_imputation_flag_names_the_adam_codes(self):
        def rule(**fields):
            return mapping_doc.describe_derivation(
                {"imputation_flag": {"source": "ASTDTM", **fields}}
            )

        self.assertEqual(
            rule(part="date"),
            'Date imputation flag for ASTDTM: "Y" when the whole date was '
            'imputed, "M" when the month and day were, "D" when only the day '
            "was; blank when the date was collected in full.",
        )
        self.assertEqual(
            rule(part="time"),
            'Time imputation flag for ASTDTM: "H" when the whole time was '
            'imputed, "M" when the minutes and seconds were, "S" when only the '
            "seconds were; blank when the time was collected to the second.",
        )
        self.assertEqual(
            rule(part="time", seconds_collected=False),
            'Time imputation flag for ASTDTM: "H" when the whole time was '
            'imputed, "M" when the minutes were; blank when the minutes were '
            "collected, since the study does not collect seconds.",
        )

    def test_to_date_leaves_a_partial_date_blank(self):
        self.assertEqual(
            mapping_doc.describe_derivation({"to_date": {"source": "CMSTDTC"}}),
            "Take the date part of CMSTDTC, blank for a year or a year and month "
            "alone.",
        )

    def test_dtc_stops_before_the_first_blank_part(self):
        self.assertEqual(
            mapping_doc.describe_derivation(
                {"dtc": {"year": "AEST_YR", "month": "AEST_MO", "day": "AEST_DY"}}
            ),
            "Write the collected year AEST_YR, month AEST_MO, day AEST_DY as ISO "
            "8601 date text, stopping before the first blank part; blank when the "
            "year is blank; a part that is not its digits is an error.",
        )
        rule = mapping_doc.describe_derivation({"dtc": {"year": "YR", "invalid": None}})
        self.assertIn("a part that is not its digits " + mapping_doc.ARROW, rule)

    def test_time_of_day_counts_seconds_since_midnight(self):
        self.assertEqual(
            mapping_doc.describe_derivation({"time_of_day": {"source": "ASTDTM"}}),
            "Seconds since midnight for the time of day of ASTDTM.",
        )

    def test_duration_text_names_the_unit_its_number_counts(self):
        self.assertEqual(
            mapping_doc.describe_derivation(
                {"duration_text": {"source": "EXDURMIN", "unit": "minute"}}
            ),
            "Write EXDURMIN, a number of minutes, as ISO 8601 duration text with "
            "days as the largest unit.",
        )

    def test_str_substr_names_the_characters_it_keeps_and_when_it_is_blank(self):
        for source, expected in [
            (
                {"source": "COMMENT", "start": 201, "length": 200},
                "Take characters 201 to 400 of COMMENT, blank when it has fewer "
                "than 201 characters.",
            ),
            (
                {"source": "COMMENT", "start": 1, "missing": "NONE"},
                "Take characters 1 to the end of COMMENT, blank when it is empty; "
                "a missing source " + mapping_doc.ARROW + ' "NONE".',
            ),
        ]:
            with self.subTest(source=source):
                self.assertEqual(
                    mapping_doc.describe_derivation({"str_substr": source}), expected
                )

    def test_str_length_counts_the_characters(self):
        self.assertEqual(
            mapping_doc.describe_derivation({"str_length": {"source": "COMMENT"}}),
            "Number of characters in COMMENT.",
        )

    def test_date_text_names_each_declared_spelling_and_survives_none(self):
        # An empty or absent list is refused before data (TIME-22), but the
        # sheet still renders the specification that writes it.
        for forms, spellings in [
            (
                ["iso", "mdy_slash"],
                "ISO 8601 date text or month-first slashed dates such as "
                "03/04/2001 for March 4",
            ),
            (["monyyyy"], "month-year text such as MAR2001 for a month alone"),
            ([], "no declared spelling"),
            (None, "no declared spelling"),
        ]:
            node = {"source": "CM.CMSTDAT"}
            if forms is not None:
                node["forms"] = forms
            with self.subTest(forms=forms):
                self.assertEqual(
                    mapping_doc.describe_derivation({"date_text": node}),
                    "Read CM.CMSTDAT as ISO 8601 date text from "
                    + spellings
                    + "; blank when it is blank; any other spelling is an error.",
                )

    def test_cut_names_the_closed_side_of_each_interval(self):
        rule = method_column("adam-adsl-demographics")["AGEGR1"]
        self.assertIn("intervals closed on the left", rule)
        self.assertIn('"18-64"', rule)

    def test_handled_expression_reports_the_unconvertible_value(self):
        rule = method_column("adam-adsl-demographics")["AGE"]
        self.assertIn("cannot be converted", rule)

    def test_decimal_text_states_its_decimals_width_and_missing_answer(self):
        # A declared `missing: null` restates the default and is left out.
        for case, node, expected in [
            (
                "aligned",
                {"source": "AVAL", "decimals": 2, "width": 8, "missing": "."},
                "Write AVAL as text with exactly 2 decimals, rounded half away "
                "from zero, padded on the left with spaces to at least 8 "
                "characters; a missing source " + mapping_doc.ARROW + ' ".".',
            ),
            (
                "one decimal",
                {"source": "AVAL", "decimals": 1},
                "Write AVAL as text with exactly 1 decimal, rounded half away "
                "from zero.",
            ),
            (
                "no decimals",
                {"source": "AVAL", "decimals": 0, "missing": None},
                "Write AVAL as text with no decimals, rounded half away from zero.",
            ),
        ]:
            with self.subTest(case=case):
                self.assertEqual(
                    mapping_doc.describe_derivation({"str_decimal": node}), expected
                )


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
            with self.subTest(name=name):
                sheets = {
                    tab_id: rows
                    for tab_id, _, _, rows in mapping_doc.mapping_sheets(
                        load_spec(name)
                    )
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
        # A child layer that restates a column without its derivation.
        spec = {
            "yamaa": "1.0",
            "type": "spec",
            "parent": "base.yaml",
            "domain": "ADSL",
            "output": {"columns": ["USUBJID"]},
            "columns": [{"name": "USUBJID", "label": "Unique Subject Identifier"}],
        }
        _, _, _, rows = mapping_doc.mapping_sheets(spec)[0]
        self.assertEqual(rows[0][6], mapping_doc.INHERITED_RULE)
        # A derivation written as YAML null is the missing literal (D18).
        spec["columns"][0]["derivation"] = None
        _, _, _, rows = mapping_doc.mapping_sheets(spec)[0]
        self.assertEqual(rows[0][6], 'Set constant value "None".')

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
