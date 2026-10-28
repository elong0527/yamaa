"""Installed project submissions with independent XML/JSON oracles and no reference imports.

`project.submission(section, created=...)` composes Define-XML (and the
Dataset-JSON or SAS Transport files where the section sets
`transport: dataset_json` or `transport: xpt`) when called; nothing is written
until `save()`, which publishes the composed bytes.
"""

import builtins
import json
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import yaml
from host_fixtures import load as load_truth
from jsonschema import Draft202012Validator
from lxml import etree

import yamaa

TRUTH = load_truth(__file__, "public-define")

ROOT = Path(__file__).with_name("define-fixtures")
SCHEMAS = ROOT / "schemas"
NS = {
    "o": "http://www.cdisc.org/ns/odm/v1.3",
    "d": "http://www.cdisc.org/ns/def/v2.1",
}
# A document validates against the schema of the Define-XML version its `def`
# namespace names.
DEFINE_SCHEMAS = {
    "http://www.cdisc.org/ns/def/v2.0": "define/2.0/define2-0-0.xsd",
    "http://www.cdisc.org/ns/def/v2.1": "define/2.1/define2-1-0.xsd",
}


class PublicDefine(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)

    def fixture(self, name, published=False):
        directory = Path(tempfile.mkdtemp(prefix=name + "-", dir=self.base))
        shutil.copytree(ROOT / name, directory, dirs_exist_ok=True)
        if published:
            dataset = "lb" if name == "sdtm-lb-metadata" else "dm"
            shutil.copy2(
                directory / "expected" / f"{dataset}.csv", directory / f"{dataset}.csv"
            )
        self.assertFalse((directory / "input").exists())
        return directory

    def edit_project(self, directory, edit):
        path = directory / "project.yaml"
        document = yaml.safe_load(path.read_text())
        edit(document)
        path.write_text(yaml.safe_dump(document, sort_keys=False))
        return document

    def metadata_only(self, directory):
        """Compose Define-XML alone: the section no longer asks for Dataset-JSON."""
        self.edit_project(directory, lambda document: document["sdtm"].pop("transport"))

    def call(self, directory, created=TRUTH["created"], section="sdtm"):
        original = builtins.__import__

        def guarded(name, *args, **kwargs):
            if name == "jsonschema" or name.startswith(
                (
                    "jsonschema.",
                    "yamaa.specification",
                    "yamaa.submission",
                    "yamaa.expressions",
                    "yamaa._reference_domain",
                )
            ):
                raise AssertionError(
                    "Define imported reference or test-only validation: " + name
                )
            return original(name, *args, **kwargs)

        with patch("builtins.__import__", guarded):
            result = yamaa.project(directory / "project.yaml").submission(
                section, created=created
            )
        self.assertEqual(result.issues.column_names, TRUTH["issue_columns"])
        for row in result.issues.to_pylist():
            self.assertIsNone(row["source_span"])
            self.assertIsNone(row["operand_route"])
        return result

    def save(self, result, directory, names):
        """Save, require exactly `names` to appear, and return their bytes.

        save() returns their absolute paths, the transport files first and
        the document last (JSON-14).
        """
        before = {path.name for path in directory.iterdir()}
        paths = result.save()
        self.assertEqual(
            sorted(paths),
            sorted((directory / name).resolve().as_posix() for name in names),
        )
        self.assertTrue(paths[-1].endswith(".xml"), paths)
        self.assertEqual(
            {path.name for path in directory.iterdir()} - before, set(names)
        )
        return {name: (directory / name).read_bytes() for name in names}

    def refused(self, result):
        with self.assertRaises(yamaa.Error) as caught:
            result.save()
        self.assertTrue(
            caught.exception.issues.to_polars().equals(result.issues.to_polars())
        )
        self.assertEqual(caught.exception.paths, [])

    def validate_xml(self, content):
        parser = etree.XMLParser(no_network=True, resolve_entities=False)
        root = etree.fromstring(content, parser)
        schema = etree.XMLSchema(
            etree.parse(str(SCHEMAS / DEFINE_SCHEMAS[root.nsmap["def"]]), parser)
        )
        schema.assertValid(root)
        return root

    def validate_json(self, content):
        data = json.loads(content)
        Draft202012Validator(
            json.loads((SCHEMAS / "dataset.schema.json").read_bytes())
        ).validate(data)
        return data

    def validate_ndjson(self, content):
        # JSON-4, JSON-13: the first line, with the row lines appended as its
        # `rows`, is the single-object form, valid against the same schema.
        text = content.decode("utf-8")
        self.assertTrue(text.endswith("\n"))
        self.assertNotIn("\r", text)
        lines = text[:-1].split("\n")
        data = json.loads(lines[0])
        self.assertNotIn("rows", data)
        data["rows"] = [json.loads(line) for line in lines[1:]]
        self.assertEqual(data["records"], len(data["rows"]))
        Draft202012Validator(
            json.loads((SCHEMAS / "dataset.schema.json").read_bytes())
        ).validate(data)
        return data

    def test_lb_independent_completion_value_links_schema_json_and_held_save(self):
        directory = self.fixture("sdtm-lb-metadata", published=True)
        result = self.call(directory)
        self.assertEqual(result.issues.num_rows, 0, result.issues)
        # The composition is held: sources may change or go before save.
        for name in TRUTH["retained_source_files"]:
            (directory / name).unlink()
            self.assertFalse((directory / name).exists())
        (directory / "lb.csv").write_text("changed after preparation\n")
        written = self.save(result, directory, ["lb.json", "define.xml"])
        xml = self.validate_xml(written["define.xml"])
        self.assertEqual(xml.get("CreationDateTime"), TRUTH["created"])
        self.assertEqual(xml.get("SourceSystemVersion"), "1.0")
        self.assertEqual(
            xml.xpath("//d:ValueListDef/@OID", namespaces=NS), ["VL.LB.LBORRESU"]
        )
        self.assertEqual(
            xml.xpath("//o:ItemDef[d:ValueListRef]/@OID", namespaces=NS),
            ["IT.LB.LBORRESU"],
        )
        refs = xml.xpath("//d:ValueListDef/o:ItemRef", namespaces=NS)
        self.assertEqual([ref.get("OrderNumber") for ref in refs], ["1", "2"])
        self.assertEqual([ref.get("Mandatory") for ref in refs], ["No", "No"])
        self.assertEqual(len(xml.xpath("//o:ItemDef", namespaces=NS)), 13)
        for ordinal, code, source, codelist in [
            (1, "GLUC", "Vendor", "CL_GLUCOSE"),
            (2, "CREAT", "Sponsor", "CL_CREATININE"),
        ]:
            identity = f"LB.LBORRESU.{code}"
            self.assertEqual(refs[ordinal - 1].get("ItemOID"), "IT." + identity)
            self.assertEqual(
                refs[ordinal - 1].xpath(
                    "d:WhereClauseRef/@WhereClauseOID", namespaces=NS
                ),
                ["WC." + identity],
            )
            item = xml.xpath(
                "//o:ItemDef[@OID=$oid]", namespaces=NS, oid="IT." + identity
            )[0]
            # Every unit of both test codes is `mg/dL`, five bytes (SUB-24).
            self.assertEqual(
                (item.get("Name"), item.get("DataType"), item.get("Length")),
                ("LBORRESU", "text", "5"),
            )
            self.assertEqual(
                item.xpath("o:CodeListRef/@CodeListOID", namespaces=NS),
                ["CL." + codelist],
            )
            self.assertEqual(item.xpath("d:Origin/@Type", namespaces=NS), ["Assigned"])
            self.assertEqual(item.xpath("d:Origin/@Source", namespaces=NS), [source])
            check = xml.xpath(
                "//d:WhereClauseDef[@OID=$oid]/o:RangeCheck",
                namespaces=NS,
                oid="WC." + identity,
            )[0]
            self.assertEqual(
                (
                    check.get("Comparator"),
                    check.get("SoftHard"),
                    check.get("{" + NS["d"] + "}ItemOID"),
                ),
                ("EQ", "Soft", "IT.LB.LBTESTCD"),
            )
            self.assertEqual(check.xpath("o:CheckValue/text()", namespaces=NS), [code])
        data = self.validate_json(written["lb.json"])
        self.assertEqual(
            [column["name"] for column in data["columns"]], TRUTH["lb_columns"]
        )
        self.assertEqual(
            data["rows"],
            [
                [
                    "LB",
                    "STUDY01",
                    "001",
                    2,
                    "GLUC",
                    "Glucose",
                    "95",
                    "mg/dL",
                    95.0,
                    "mg/dL",
                    "2025-01-01",
                ],
                [
                    "LB",
                    "STUDY01",
                    "002",
                    1,
                    "GLUC",
                    "Glucose",
                    "101",
                    "mg/dL",
                    101.0,
                    "mg/dL",
                    "2025-01-02",
                ],
                [
                    "LB",
                    "STUDY01",
                    "001",
                    1,
                    "CREAT",
                    "Creatinine",
                    "0.9",
                    "mg/dL",
                    0.9,
                    "mg/dL",
                    "2025-01-01",
                ],
            ],
        )

    def test_lb_original_source_and_selected_value_codelist_rejections_write_nothing(
        self,
    ):
        for mode in ["original source", "value codelist"]:
            with self.subTest(mode=mode):
                directory = self.fixture("sdtm-lb-metadata", published=True)
                if mode == "original source":
                    # Original spec.yaml stays byte-identical. Only the independent
                    # completion's explicit Sponsor override is removed.
                    overlay = yaml.safe_load(
                        (directory / "submission-spec.yaml").read_text()
                    )
                    units = next(
                        column
                        for column in overlay["columns"]
                        if column["name"] == "LBORRESU"
                    )
                    del units["submission"]["value_level"]
                    (directory / "submission-spec.yaml").write_text(
                        yaml.safe_dump(overlay, sort_keys=False)
                    )
                else:
                    (directory / "lb.csv").write_text(
                        (directory / "lb.csv")
                        .read_text()
                        .replace("95,mg/dL,95", "95,mmol/L,95")
                    )
                result = self.call(directory)
                issues = result.issues.to_pylist()
                if mode == "original source":
                    self.assertTrue(
                        any(
                            row["rule"] == "SUB-15"
                            and row["condition"] == "origin_source_not_admitted"
                            for row in issues
                        ),
                        issues,
                    )
                    self.assertEqual(
                        (directory / "spec.yaml").read_bytes(),
                        (ROOT / "sdtm-lb-metadata/spec.yaml").read_bytes(),
                    )
                else:
                    issue = next(
                        row
                        for row in issues
                        if row["condition"] == "allowed_values_failed"
                    )
                    self.assertEqual(issue["rule"], "CT-7")
                    context = json.loads(issue["context"])
                    self.assertEqual(
                        (
                            context["column"],
                            context["codelist"],
                            context["entry"],
                            context["row"],
                        ),
                        ("LBORRESU", "CL_GLUCOSE", "GLUC", 0),
                    )
                    self.assertTrue(
                        issue["spec_paths"][0].endswith(
                            "columns.LBORRESU.submission.value_level.GLUC.codelist"
                        )
                    )
                self.refused(result)
                self.assertFalse((directory / "define.xml").exists())
                self.assertFalse((directory / "lb.json").exists())
                shutil.rmtree(directory)

    def test_lb_metadata_only_and_all_independent_value_admission_controls(self):
        entries = "sdtm.specs[0].columns.LBORRESU.submission.value_level"
        for case, rule, condition, suffix in [
            ("valid", None, None, None),
            ("arbitrary row ID", None, None, None),
            ("membership", None, None, None),
            (
                "unknown",
                "SUB-12",
                "value_level_where_invalid",
                entries + ".GLUC.where.BOGUS",
            ),
            (
                "output",
                "SUB-12",
                "value_level_where_invalid",
                entries + ".GLUC.where.LBSTRESU",
            ),
            ("overlap", "SUB-12", "value_level_overlap", entries + ".CREAT.where"),
            ("duplicate", "SUB-12", "duplicate_identifier", entries + ".GLUC.id"),
            ("origin", "SUB-13", "refuted_value_origin", entries + ".GLUC"),
        ]:
            with self.subTest(case=case):
                directory = self.fixture("sdtm-lb-metadata")
                self.metadata_only(directory)
                spec = yaml.safe_load((directory / "spec.yaml").read_text())
                overlay = yaml.safe_load(
                    (directory / "submission-spec.yaml").read_text()
                )
                units = next(
                    column
                    for column in overlay["columns"]
                    if column["name"] == "LBORRESU"
                )
                glucose, creatinine = units["submission"]["value_level"]
                if case == "arbitrary row ID":
                    spec["rows"][0]["id"] = "glucose / primary test: 1"
                elif case == "membership":
                    units["submission"]["value_level"] = [
                        {
                            "id": "CHEM",
                            "where": {"LBTESTCD": ["GLUC", "CREAT"]},
                            "origins": [{"type": "Assigned", "source": "Vendor"}],
                        }
                    ]
                elif case == "unknown":
                    glucose["where"] = {"BOGUS": ["GLUC"]}
                elif case == "output":
                    spec["output"]["columns"].remove("LBSTRESU")
                    glucose["where"] = {"LBSTRESU": ["mg/dL"]}
                elif case == "overlap":
                    creatinine["where"]["LBTESTCD"].append("GLUC")
                elif case == "duplicate":
                    creatinine["id"] = "GLUC"
                elif case == "origin":
                    # A literal unit reads no input, so it refutes Derived.
                    glucose["origins"][0]["type"] = "Derived"
                if case != "valid":
                    (directory / "spec.yaml").write_text(
                        yaml.safe_dump(spec, sort_keys=False)
                    )
                    (directory / "submission-spec.yaml").write_text(
                        yaml.safe_dump(overlay, sort_keys=False)
                    )
                result = self.call(directory)
                if rule is None:
                    self.assertEqual(result.issues.num_rows, 0, result.issues)
                    written = self.save(result, directory, ["define.xml"])
                    xml = self.validate_xml(written["define.xml"])
                    if case == "membership":
                        check = xml.xpath(
                            "//d:WhereClauseDef/o:RangeCheck", namespaces=NS
                        )
                        self.assertEqual(len(check), 1)
                        self.assertEqual(check[0].get("Comparator"), "IN")
                        self.assertEqual(
                            check[0].xpath("o:CheckValue/text()", namespaces=NS),
                            ["GLUC", "CREAT"],
                        )
                else:
                    self.assertTrue(
                        any(
                            row["rule"] == rule
                            and row["condition"] == condition
                            and suffix in row["spec_paths"]
                            for row in result.issues.to_pylist()
                        ),
                        result.issues,
                    )
                    self.refused(result)
                    self.assertFalse((directory / "define.xml").exists())
                self.assertFalse((directory / "input").exists())
                self.assertFalse((directory / "lb.csv").exists())
                self.assertFalse((directory / "lb.json").exists())
                shutil.rmtree(directory)

    def test_dm_goldens_public_issues_and_held_save(self):
        directory = self.fixture("sdtm-dm-metadata", published=True)
        result = self.call(directory)
        self.assertEqual(result.issues.num_rows, 0, result.issues)
        for name in ("dm.json", "define.xml"):
            self.assertFalse((directory / name).exists())
        (directory / "project.yaml").unlink()
        (directory / "spec.yaml").unlink()
        (directory / "dm.csv").write_text("changed published artifact\n")
        written = self.save(result, directory, ["dm.json", "define.xml"])
        for name, content in written.items():
            self.assertEqual(content, (directory / "expected" / name).read_bytes())
        self.validate_xml(written["define.xml"])
        self.validate_json(written["dm.json"])
        # Every save rewrites the held bytes: deleted files come back unchanged.
        for name in written:
            (directory / name).unlink()
        self.assertEqual(
            self.save(result, directory, ["dm.json", "define.xml"]), written
        )

    def test_every_declared_define_section_is_composed_without_a_section(self):
        directory = self.fixture("sdtm-dm-metadata", published=True)
        result = self.call(directory, section=None)
        self.assertEqual(result.issues.num_rows, 0, result.issues)
        written = self.save(result, directory, ["dm.json", "define.xml"])
        for name, content in written.items():
            self.assertEqual(content, (directory / "expected" / name).read_bytes())
        # A section the project does not declare refuses the call, naming the
        # sections it declares (PROJECT-14).
        with self.assertRaises(yamaa.Error) as caught:
            self.call(directory, section="adam")
        self.assertEqual(
            [
                (
                    row["condition"],
                    row["rule"],
                    row["spec_paths"],
                    json.loads(row["context"]),
                )
                for row in caught.exception.issues.to_pylist()
            ],
            [
                (
                    "unknown_section",
                    "PROJECT-14",
                    [],
                    {"section": "adam", "sections": ["sdtm"]},
                )
            ],
        )
        self.assertEqual(caught.exception.paths, [])

    def test_ae_vs_independent_fixed_time_schema_and_structure(self):
        expected = {
            "sdtm-ae-coding": ("AE", TRUTH["ae_columns"], "Other", "MEDDRA"),
            "sdtm-vs-epoch-from-subject-elements": (
                "VS",
                TRUTH["vs_columns"],
                "Other",
                "EPOCH",
            ),
        }
        for name, (id_, columns, context, codelist) in expected.items():
            with self.subTest(name=name):
                directory = self.fixture(name)
                result = self.call(directory, "2026-09-30T00:00:00")
                self.assertEqual(result.issues.num_rows, 0, result.issues)
                self.assertFalse((directory / (id_.lower() + ".csv")).exists())
                written = self.save(result, directory, ["define.xml"])
                xml = self.validate_xml(written["define.xml"])
                self.assertEqual(xml.get("CreationDateTime"), "2026-09-30T00:00:00")
                self.assertEqual(xml.get("SourceSystem"), "yamaa")
                self.assertEqual(xml.get("SourceSystemVersion"), "1.0")
                self.assertEqual(xml.get("{" + NS["d"] + "}Context"), context)
                self.assertEqual(
                    xml.xpath("//o:ItemGroupDef/@OID", namespaces=NS), ["IG." + id_]
                )
                self.assertEqual(xml.xpath("//o:ItemDef/@Name", namespaces=NS), columns)
                self.assertEqual(
                    xml.xpath("//o:ItemRef/@OrderNumber", namespaces=NS),
                    [str(i) for i in range(1, len(columns) + 1)],
                )
                self.assertEqual(
                    xml.xpath("//o:CodeList/@OID", namespaces=NS), ["CL." + codelist]
                )
                # Each project declares the annotated case report form its
                # collected values reference.
                self.assertEqual(
                    xml.xpath("//d:AnnotatedCRF/d:DocumentRef/@leafID", namespaces=NS),
                    ["LF.acrf"],
                )
                if id_ == "AE":
                    self.assertEqual(
                        xml.xpath("//o:ExternalCodeList/@Dictionary", namespaces=NS),
                        ["MedDRA"],
                    )
                    # The dictionary-derived term is a synonym of the reported
                    # term.
                    self.assertEqual(
                        xml.xpath(
                            "//o:ItemRef[@ItemOID='IT.AE.AEDECOD']/@Role", namespaces=NS
                        ),
                        ["Synonym Qualifier"],
                    )
                    self.assertEqual(
                        xml.xpath("//o:MethodDef/@OID", namespaces=NS),
                        ["MT.AE.AEDECOD", "MT.AE.AEBODSYS"],
                    )
                else:
                    self.assertEqual(
                        xml.xpath(
                            "//o:CodeList/o:EnumeratedItem/@CodedValue", namespaces=NS
                        ),
                        ["SCREENING", "TREATMENT", "FOLLOW-UP"],
                    )
                    self.assertEqual(
                        xml.xpath(
                            "//o:ItemDef[@Name='VSDTC']/@DataType", namespaces=NS
                        ),
                        ["partialDatetime"],
                    )
                    # Each column's role, whether it is mandatory, and its
                    # origin: type, source and document references. The
                    # investigator collects the result and its date-time, and
                    # under Other each references the annotated form with no
                    # page. EPOCH is a permissible timing variable the sponsor
                    # derives from the subject's elements.
                    collected = ("Collected", "Investigator")
                    for column, role, mandatory, origin, documents in (
                        ("VSORRES", "Result Qualifier", "No", collected, ["LF.acrf"]),
                        ("VSDTC", "Timing", "No", collected, ["LF.acrf"]),
                        ("EPOCH", "Timing", "No", ("Derived", "Sponsor"), []),
                    ):
                        reference = f"//o:ItemRef[@ItemOID='IT.VS.{column}']"
                        self.assertEqual(
                            xml.xpath(reference + "/@Role", namespaces=NS), [role]
                        )
                        self.assertEqual(
                            xml.xpath(reference + "/@Mandatory", namespaces=NS),
                            [mandatory],
                        )
                        declared = xml.xpath(
                            f"//o:ItemDef[@Name='{column}']/d:Origin", namespaces=NS
                        )
                        self.assertEqual(
                            [(o.get("Type"), o.get("Source")) for o in declared],
                            [origin],
                        )
                        self.assertEqual(
                            declared[0].xpath("d:DocumentRef/@leafID", namespaces=NS),
                            documents,
                        )
                        self.assertEqual(
                            declared[0].xpath(
                                "d:DocumentRef/d:PDFPageRef", namespaces=NS
                            ),
                            [],
                        )

    def test_missing_artifact_only_when_requested_and_semantic_failure_zero_writes(
        self,
    ):
        directory = self.fixture("sdtm-dm-metadata")
        result = self.call(directory)
        self.assertIn(
            "producer_contract_mismatch",
            result.issues.to_polars()["condition"].to_list(),
        )
        self.assertIn("INPUT-11", result.issues.to_polars()["rule"].to_list())
        self.refused(result)
        # A document beside the published tables, not a submission (DEFINE-4).
        self.metadata_only(directory)
        self.edit_project(
            directory, lambda document: document["sdtm"].update(context="Other")
        )
        result = self.call(directory)
        self.assertEqual(result.issues.num_rows, 0, result.issues)
        project = directory / "project.yaml"
        project.write_text(project.read_text().replace("name: SDTMIG", "name: ADaMIG"))
        result = self.call(directory)
        self.assertIn(
            "standard_model_mismatch", result.issues.to_polars()["condition"].to_list()
        )
        self.refused(result)
        self.assertFalse((directory / "define.xml").exists())
        self.assertFalse((directory / "dm.json").exists())

    def test_output_preflight_failure_stops_all_outputs_and_preserves_cause(self):
        directory = self.fixture("sdtm-dm-metadata", published=True)
        result = self.call(directory)
        self.assertEqual(result.issues.num_rows, 0, result.issues)
        (directory / "define.xml").mkdir()
        with self.assertRaises(yamaa.Error) as caught:
            result.save()
        self.assertEqual(caught.exception.paths, [])
        self.assertFalse((directory / "dm.json").exists())
        self.assertTrue((directory / "define.xml").is_dir())
        # The refused save explains itself, and the submission keeps that cause.
        self.assertTrue(
            caught.exception.issues.to_polars().equals(result.issues.to_polars())
        )
        # A directory where define.xml goes is refused at the section's
        # `define`, naming the file as written (FILE-7).
        self.assertEqual(result.issues.to_polars()["phase"].to_list(), ["output"])
        self.assertEqual(
            result.issues.to_polars()["condition"].to_list(),
            ["resource_path_not_regular_file"],
        )
        self.assertEqual(result.issues.to_polars()["rule"].to_list(), ["FILE-7"])
        self.assertEqual(
            result.issues.to_polars()["spec_paths"].to_list(), [["sdtm.define"]]
        )
        context = json.loads(result.issues.to_polars()["context"][0])
        self.assertEqual(context, {"path": "define.xml"})

    def test_source_version_defaults_and_schema_facet_failures(self):
        from yamaa import _native

        directory = self.fixture("sdtm-dm-metadata", published=True)
        project = directory / "project.yaml"
        project.write_text(
            project.read_text().replace("  source_system_version: '1.0'\n", "")
        )
        result = self.call(directory)
        self.assertEqual(result.issues.num_rows, 0, result.issues)
        written = self.save(result, directory, ["dm.json", "define.xml"])
        # Both documents name the engine's version by default, in its Cargo
        # spelling, so both hosts write the same bytes; for a release
        # candidate it differs from `__version__` (#3030 D53).
        self.assertEqual(
            json.loads(written["dm.json"])["sourceSystem"]["version"],
            _native._CORE_VERSION,
        )
        self.assertEqual(
            self.validate_xml(written["define.xml"]).get("SourceSystemVersion"),
            _native._CORE_VERSION,
        )
        for name in written:
            (directory / name).unlink()
        # A terminology standard's name is closed where the project is read
        # (CT-2), before any document is composed.
        project.write_text(
            project.read_text().replace("name: CDISC/NCI", "name: InvalidStandard")
        )
        result = self.call(directory)
        self.assertEqual(
            result.issues.to_polars()["condition"].to_list(), ["value_not_permitted"]
        )
        self.assertEqual(result.issues.to_polars()["rule"].to_list(), ["SCHEMA-12"])
        self.assertEqual(
            result.issues.to_polars()["spec_paths"].to_list(),
            [["codelists[0].standard.name"]],
        )
        self.refused(result)
        self.assertFalse((directory / "define.xml").exists())
        # A SAS format name longer than eight characters fails only against
        # the Define-XML schema's facet, as the document is generated.
        project.write_text(
            project.read_text()
            .replace("name: InvalidStandard", "name: CDISC/NCI")
            .replace("format_name: $SEX\n", "format_name: $SEXCODELIST\n")
        )
        result = self.call(directory)
        self.assertIn("DEFINE-25", result.issues.to_polars()["rule"].to_list())
        self.refused(result)
        self.assertFalse((directory / "define.xml").exists())

    def test_defaults_selection_bound_lists_and_required_acrf_aliases(self):
        directory = self.fixture("sdtm-dm-metadata", published=True)

        def edit(document):
            section = document["sdtm"]
            del section["file_oid"], section["metadata_version"]
            document["adam"] = {
                "standards": [{"name": "ADaMIG", "version": "1.3", "status": "Final"}],
                "specs": ["missing-other-section.yaml"],
                "define": "adam.xml",
            }
            document["codelists"].append(
                {
                    "codelists": [
                        {
                            "id": "UNUSED",
                            "name": "Used only by another submission",
                            "data_type": "text",
                            "items": [{"value": "OTHER"}],
                        },
                    ]
                }
            )

        document = self.edit_project(directory, edit)
        result = self.call(directory)
        self.assertEqual(result.issues.num_rows, 0, result.issues)
        content = self.save(result, directory, ["dm.json", "define.xml"])["define.xml"]
        xml = self.validate_xml(content)
        self.assertEqual(xml.get("FileOID"), "STUDY01.sdtm.Define-XML_2.1")
        self.assertEqual(
            xml.xpath("//o:MetaDataVersion/@OID", namespaces=NS), ["MDV.STUDY01.sdtm"]
        )
        self.assertEqual(
            xml.xpath("//o:MetaDataVersion/@Name", namespaces=NS),
            ["STUDY01 sdtm metadata"],
        )
        self.assertNotIn(b"UNUSED", content)
        self.assertEqual(
            xml.xpath("//o:CodeList/@OID", namespaces=NS),
            ["CL.DOMAIN", "CL.AGEU", "CL.SEX", "CL.COUNTRY"],
        )
        # Composing again gives the same bytes.
        for name in ("dm.json", "define.xml"):
            (directory / name).unlink()
        again = self.save(self.call(directory), directory, ["dm.json", "define.xml"])
        self.assertEqual(again["define.xml"], content)
        self.assertFalse((directory / "adam.xml").exists())
        for name in ("dm.json", "define.xml"):
            (directory / name).unlink()
        section = document["sdtm"]
        section["documents"] = [
            d for d in section["documents"] if d["kind"] != "annotated_crf"
        ]
        (directory / "project.yaml").write_text(
            yaml.safe_dump(document, sort_keys=False)
        )
        result = self.call(directory)
        self.assertIn(
            "origin_document_missing", result.issues.to_polars()["condition"].to_list()
        )
        self.refused(result)
        self.assertFalse((directory / "dm.json").exists())
        section["documents"] = yaml.safe_load(
            (ROOT / "sdtm-dm-metadata/project.yaml").read_text()
        )["sdtm"]["documents"]
        del document["codelists"][0]["codelists"][0]["alias"]
        (directory / "project.yaml").write_text(
            yaml.safe_dump(document, sort_keys=False)
        )
        result = self.call(directory)
        self.assertIn("DEFINE-4", result.issues.to_polars()["rule"].to_list())
        self.refused(result)
        self.assertFalse((directory / "define.xml").exists())

    def test_inherited_original_output_and_metadata_refusals(self):
        directory = self.fixture("sdtm-dm-metadata")
        for name in ["parents", "children", "artifacts"]:
            (directory / name).mkdir()
        spec = (
            (directory / "spec.yaml")
            .read_text()
            .replace("path: dm.csv", "path: ../artifacts/dm.csv")
        )
        (directory / "parents/base.yaml").write_text(spec)
        (directory / "children/spec.yaml").write_text(
            'yamaa: "1.0"\ntype: spec\nparent: ../parents/base.yaml\n'
        )
        project = directory / "project.yaml"
        project.write_text(
            project.read_text()
            .replace("  - spec.yaml", "  - children/spec.yaml")
            .replace("define: define.xml", "define: artifacts/define.xml")
        )
        shutil.copy2(directory / "expected/dm.csv", directory / "artifacts/dm.csv")
        result = self.call(directory)
        self.assertEqual(result.issues.num_rows, 0, result.issues)
        written = self.save(result, directory / "artifacts", ["dm.json", "define.xml"])
        self.assertIn(b'xlink:href="dm.json"', written["define.xml"])
        self.assertEqual(json.loads(written["dm.json"])["metaDataRef"], "define.xml")
        self.assertFalse((directory / "dm.json").exists())
        # The SEX codelist lists its items, so an allowed_values beside it
        # states the same value set twice.
        redundant = spec.replace(
            "derivation: DM_RAW.SEX\n",
            "derivation: DM_RAW.SEX\n    verifications:\n      - allowed_values:\n"
            "          values: [F, M, U]\n",
        )
        self.assertNotEqual(redundant, spec)
        (directory / "parents/base.yaml").write_text(redundant)
        result = self.call(directory)
        self.assertIn(
            "redundant_declaration", result.issues.to_polars()["condition"].to_list()
        )
        self.assertIn("CT-9", result.issues.to_polars()["rule"].to_list())
        self.refused(result)
        self.assertEqual(
            (directory / "artifacts/define.xml").read_bytes(), written["define.xml"]
        )

    def test_mutual_reference_and_stale_published_contract_refusals(self):
        directory = self.fixture("sdtm-dm-metadata")
        (directory / "artifacts").mkdir()
        spec = directory / "spec.yaml"
        original = spec.read_text()
        spec.write_text(original.replace("path: dm.csv", "path: artifacts/dm.csv"))
        shutil.copy2(directory / "expected/dm.csv", directory / "artifacts/dm.csv")
        # The transport file sits beside the document wherever the artifact is.
        result = self.call(directory)
        self.assertEqual(result.issues.num_rows, 0, result.issues)
        written = self.save(result, directory, ["dm.json", "define.xml"])
        self.assertIn(b'xlink:href="dm.json"', written["define.xml"])
        self.assertFalse((directory / "artifacts/dm.json").exists())
        (directory / "dm.json").unlink()
        (directory / "define.xml").unlink()
        spec.write_text(original)
        csv = (directory / "expected/dm.csv").read_text()
        (directory / "dm.csv").write_text(
            csv.replace("STUDYID,USUBJID", "USUBJID,STUDYID")
        )
        result = self.call(directory)
        self.assertIn("INPUT-11", result.issues.to_polars()["rule"].to_list())
        self.refused(result)
        (directory / "dm.csv").write_text(csv.replace("STUDYID", "WRONG"))
        result = self.call(directory)
        self.assertIn("INPUT-11", result.issues.to_polars()["rule"].to_list())
        self.refused(result)
        (directory / "dm.csv").write_text(csv.replace(",F,USA", ",Z,USA"))
        result = self.call(directory)
        issue = (
            result.issues.to_polars()
            .filter(result.issues.to_polars()["rule"] == "CT-7")
            .row(0, named=True)
        )
        self.assertEqual(issue["phase"], "verification")
        context = json.loads(issue["context"])
        self.assertEqual(context["value"], "Z")
        self.assertIn("CATH-UCSD-0001", context["key_values"])
        self.refused(result)
        self.assertFalse((directory / "dm.json").exists())

    def test_transport_names_equal_after_lowercasing_preserve_publication_sentinel(
        self,
    ):
        directory = self.fixture("sdtm-dm-metadata", published=True)
        sentinel = directory / "dm.json"
        sentinel.write_bytes(b"original publication sentinel")
        project = directory / "project.yaml"
        project.write_text(
            project.read_text()
            .replace("  - spec.yaml", "  - spec.yaml\n  - second.yaml")
            .replace("sdtm:\n", "sdtm:\n  context: Other\n")
        )
        (directory / "second.yaml").write_text(
            (directory / "spec.yaml")
            .read_text()
            .replace("domain: DM", "domain: dm")
            .replace("path: dm.csv", "path: second.csv")
        )
        define_sentinel = directory / "define.xml"
        define_sentinel.write_bytes(b"original Define sentinel")
        names = sorted(path.name for path in directory.iterdir())
        result = self.call(directory)
        # The shared truth's row, which R compares too. Its context names the
        # two files by their absolute paths, so it is compared by the file
        # name both writers would write.
        expected = dict(TRUTH["transport_name_collision_issue"])
        shared = expected.pop("context_names")
        expected["file"] = (directory.resolve() / expected["file"]).as_posix()
        [row] = result.issues.to_pylist()
        context = row.pop("context")
        self.assertEqual(row, expected, "two datasets named dm in any case")
        self.assertIn(shared, context, "the finding names the colliding file")
        self.refused(result)
        self.assertEqual(sentinel.read_bytes(), b"original publication sentinel")
        self.assertEqual(define_sentinel.read_bytes(), b"original Define sentinel")
        self.assertEqual(sorted(path.name for path in directory.iterdir()), names)

    def test_numeric_terminology_uses_numeric_equality(self):
        directory = self.fixture("sdtm-dm-metadata", published=True)
        spec = directory / "spec.yaml"
        spec.write_text(
            spec.read_text()
            .replace("name: AGE\n    type: int", "name: AGE\n    type: float")
            .replace(
                TRUTH["numeric_original_metadata"], TRUTH["numeric_completion_metadata"]
            )
        )
        project = directory / "project.yaml"
        project.write_text(
            project.read_text().replace("study:\n", TRUTH["numeric_codelist_yaml"])
        )
        result = self.call(directory)
        self.assertEqual(result.issues.num_rows, 0, result.issues)
        written = self.save(result, directory, ["dm.json", "define.xml"])
        self.validate_xml(written["define.xml"])
        data = json.loads(written["dm.json"])
        self.assertEqual([row[5] for row in data["rows"]], [34.0, 67.0, 52.0])

    def test_xml_text_findings_name_the_original_field_and_keep_render_order(self):
        for case in TRUTH["xml_text_cases"]:
            with self.subTest(case=case["name"]):
                directory = self.fixture("sdtm-dm-metadata")
                self.metadata_only(directory)
                for filename, mutations in [
                    ("spec.yaml", case["specification_mutations"]),
                    ("project.yaml", case["project_mutations"]),
                ]:
                    path = directory / filename
                    text = path.read_text()
                    for before, after in mutations:
                        self.assertIn(before, text)
                        text = text.replace(before, after, 1)
                    path.write_text(text)
                if case["inherit_specification"]:
                    (directory / "spec.yaml").rename(directory / "base.yaml")
                    (directory / "spec.yaml").write_text(
                        'yamaa: "1.0"\ntype: spec\nparent: base.yaml\n'
                    )
                result = self.call(directory)
                rows = result.issues.to_pylist()
                # Each finding is an error with a message, located in a file
                # the case wrote or, when no file holds its first spec path,
                # not at all (P-ISSUE-1); the case authors the rest.
                for row in rows:
                    self.assertEqual(row["severity"], "error")
                    located = row["file"] is not None
                    self.assertEqual(
                        (located, row["line"] is not None, row["column"] is not None),
                        (located,) * 3,
                    )
                    self.assertTrue(row["message"])
                unauthored = ("severity", "file", "line", "column", "message")
                actual = [
                    {
                        key: json.loads(value) if key == "context" else value
                        for key, value in row.items()
                        if key not in ("source_span", "operand_route", *unauthored)
                    }
                    for row in rows
                ]
                self.assertEqual(actual, case["issues"])
                self.assertFalse((directory / "define.xml").exists())
                self.assertFalse((directory / "dm.json").exists())
                if case["issues"]:
                    self.refused(result)
                    self.assertFalse((directory / "define.xml").exists())
                else:
                    content = self.save(result, directory, ["define.xml"])["define.xml"]
                    self.validate_xml(content)
                    for fragment in case["xml_fragments"]:
                        self.assertIn(fragment.encode(), content)
                self.assertFalse((directory / "input").exists())
                self.assertFalse((directory / "dm.csv").exists())

    def test_define_benchmarks_match_their_independent_truth(self):
        manifest = json.loads((ROOT / "benchmarks/manifest.json").read_bytes())
        names = [case["name"] for case in manifest["cases"]]
        for required in ["sdtm-dm-xpt", "adam-adsl-xpt", "sdtm-dm-metadata"]:
            self.assertIn(required, names)
        for case in manifest["cases"]:
            with self.subTest(case=case["name"]):
                source = ROOT / "benchmarks" / case["name"]
                directory = Path(
                    tempfile.mkdtemp(prefix=case["name"] + "-", dir=self.base)
                )
                for path in source.iterdir():
                    if path.is_file():
                        shutil.copy2(path, directory / path.name)
                for name in case["published"]:
                    shutil.copy2(source / "expected" / name, directory / name)
                result = yamaa.project(directory / case["project"]).submission(
                    case["standard"], created=case["created"]
                )
                if case["error"] is not None:
                    first = result.issues.to_pylist()[0]
                    first["context"] = json.loads(first["context"])
                    want = case["error"]
                    for key in ("phase", "condition", "spec_paths", "rule"):
                        self.assertEqual(first[key], want[key])
                    for key, value in want.get("context", {}).items():
                        self.assertEqual(first["context"][key], value, key)
                    self.refused(result)
                    self.assertFalse((directory / "define.xml").exists())
                    continue
                self.assertEqual(result.issues.num_rows, 0, result.issues)
                written = self.save(result, directory, case["expected"])
                for name, content in written.items():
                    self.assertEqual(
                        content, (source / "expected" / name).read_bytes(), name
                    )
                    if name == "define.xml":
                        self.validate_xml(content)
                    elif name.endswith(".ndjson"):
                        self.validate_ndjson(content)
                    elif name.endswith(".json"):
                        self.validate_json(content)


if __name__ == "__main__":
    unittest.main()
