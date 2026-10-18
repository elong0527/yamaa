"""Installed public Define with independent XML/JSON oracles and no reference imports."""

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
from yamaa import DefineError, define

TRUTH = load_truth(__file__, "public-define")

ROOT = Path(__file__).with_name("define-fixtures")
SCHEMAS = ROOT / "schemas"


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

    def call(self, directory, created=TRUTH["created"]):
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
            result = define(directory / "environment.yaml", "sdtm", created=created)
        self.assertEqual(result.issues.columns, TRUTH["issue_columns"])
        return result

    def validate_xml(self, content):
        parser = etree.XMLParser(no_network=True, resolve_entities=False)
        schema = etree.XMLSchema(
            etree.parse(str(SCHEMAS / "define/2.1/define2-1-0.xsd"), parser)
        )
        root = etree.fromstring(content, parser)
        schema.assertValid(root)
        return root

    def test_lb_independent_completion_value_links_schema_json_and_held_save(self):
        directory = self.fixture("sdtm-lb-metadata", published=True)
        result = self.call(directory)
        self.assertEqual(result.issues.height, 0, result.issues)
        artifacts = result._native.artifacts()
        self.assertEqual(
            [(name, state) for name, _, state in artifacts],
            [("lb.json", "Ready"), ("define.xml", "Ready")],
        )
        xml = self.validate_xml(artifacts[1][1])
        ns = {
            "o": "http://www.cdisc.org/ns/odm/v1.3",
            "d": "http://www.cdisc.org/ns/def/v2.1",
        }
        self.assertEqual(xml.get("CreationDateTime"), TRUTH["created"])
        self.assertEqual(xml.get("SourceSystemVersion"), "1.0")
        self.assertEqual(
            xml.xpath("//d:ValueListDef/@OID", namespaces=ns), ["VL.LB.LBORRESU"]
        )
        self.assertEqual(
            xml.xpath("//o:ItemDef[d:ValueListRef]/@OID", namespaces=ns),
            ["IT.LB.LBORRESU"],
        )
        refs = xml.xpath("//d:ValueListDef/o:ItemRef", namespaces=ns)
        self.assertEqual([ref.get("OrderNumber") for ref in refs], ["1", "2"])
        self.assertEqual([ref.get("Mandatory") for ref in refs], ["No", "No"])
        self.assertEqual(len(xml.xpath("//o:ItemDef", namespaces=ns)), 13)
        for ordinal, code, source, codelist in [
            (1, "GLUC", "Vendor", "CL_GLUCOSE"),
            (2, "CREAT", "Sponsor", "CL_CREATININE"),
        ]:
            identity = f"LB.LBORRESU.{code}"
            self.assertEqual(refs[ordinal - 1].get("ItemOID"), "IT." + identity)
            self.assertEqual(
                refs[ordinal - 1].xpath(
                    "d:WhereClauseRef/@WhereClauseOID", namespaces=ns
                ),
                ["WC." + identity],
            )
            item = xml.xpath(
                "//o:ItemDef[@OID=$oid]", namespaces=ns, oid="IT." + identity
            )[0]
            # Every unit of both test codes is `mg/dL`, five bytes (SUB-24).
            self.assertEqual(
                (item.get("Name"), item.get("DataType"), item.get("Length")),
                ("LBORRESU", "text", "5"),
            )
            self.assertEqual(
                item.xpath("o:CodeListRef/@CodeListOID", namespaces=ns),
                ["CL." + codelist],
            )
            self.assertEqual(item.xpath("d:Origin/@Type", namespaces=ns), ["Assigned"])
            self.assertEqual(item.xpath("d:Origin/@Source", namespaces=ns), [source])
            check = xml.xpath(
                "//d:WhereClauseDef[@OID=$oid]/o:RangeCheck",
                namespaces=ns,
                oid="WC." + identity,
            )[0]
            self.assertEqual(
                (
                    check.get("Comparator"),
                    check.get("SoftHard"),
                    check.get("{" + ns["d"] + "}ItemOID"),
                ),
                ("EQ", "Soft", "IT.LB.LBTESTCD"),
            )
            self.assertEqual(check.xpath("o:CheckValue/text()", namespaces=ns), [code])
        data = json.loads(artifacts[0][1])
        Draft202012Validator(
            json.loads((SCHEMAS / "dataset.schema.json").read_bytes())
        ).validate(data)
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
        reads = result._native.capture_reads()
        for name in TRUTH["retained_source_files"]:
            (directory / name).unlink()
            self.assertFalse((directory / name).exists())
        (directory / "lb.csv").write_text("changed after preparation\n")
        self.assertTrue(result.save())
        self.assertFalse(result.save())
        self.assertEqual(result._native.capture_reads(), reads)
        for name, content, _ in artifacts:
            self.assertEqual((directory / name).read_bytes(), content)

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
                    del overlay["rows"]
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
                issues = result.issues.to_dicts()
                if mode == "original source":
                    self.assertTrue(
                        any(
                            row["requirement"] == "REQ-0919"
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
                    self.assertEqual(issue["requirement"], "REQ-0957")
                    context = json.loads(issue["context"])
                    self.assertEqual(
                        (
                            context["column"],
                            context["codelist"],
                            context["testcd"],
                            context["row"],
                        ),
                        ("LBORRESU", "CL_GLUCOSE", "GLUC", 0),
                    )
                    self.assertTrue(
                        issue["spec_paths"][0].endswith(
                            "rows.glucose.submission.LBORRESU.codelist"
                        )
                    )
                with self.assertRaises(DefineError):
                    result.save()
                self.assertFalse((directory / "define.xml").exists())
                self.assertFalse((directory / "lb.json").exists())
                shutil.rmtree(directory)

    def test_lb_metadata_only_and_all_independent_value_admission_controls(self):
        for case, requirement, condition, suffix in [
            ("valid", None, None, None),
            ("arbitrary row ID", None, None, None),
            ("identical duplicate", None, None, None),
            (
                "domain",
                "REQ-1163",
                "value_metadata_requires_testcd",
                "rows.glucose.submission",
            ),
            (
                "dynamic",
                "REQ-1164",
                "value_metadata_requires_literal_testcd",
                "rows.glucose.submission",
            ),
            (
                "unknown",
                "REQ-1165",
                "value_metadata_unknown_column",
                "rows.glucose.submission.BOGUS",
            ),
            (
                "output",
                "REQ-1166",
                "value_metadata_not_output_column",
                "rows.glucose.submission.LBSTRESU",
            ),
            (
                "conflict",
                "REQ-1167",
                "conflicting_value_metadata",
                "rows.creatinine.submission.LBORRESU",
            ),
            (
                "origin",
                "REQ-1168",
                "refuted_value_origin",
                "rows.glucose.submission.LBORRESU",
            ),
        ]:
            with self.subTest(case=case):
                directory = self.fixture("sdtm-lb-metadata")
                environment = yaml.safe_load(
                    (directory / "environment.yaml").read_text()
                )
                del environment["sdtm"]["transport"]
                (directory / "environment.yaml").write_text(
                    yaml.safe_dump(environment, sort_keys=False)
                )
                spec = yaml.safe_load((directory / "spec.yaml").read_text())
                overlay = yaml.safe_load(
                    (directory / "submission-spec.yaml").read_text()
                )
                first = spec["rows"][0]
                effective = overlay["rows"][0]["submission"]
                if case == "arbitrary row ID":
                    first["id"] = "glucose / primary test: 1"
                    overlay["rows"][0]["id"] = first["id"]
                elif case == "identical duplicate":
                    spec["rows"][1]["derivations"]["LBTESTCD"] = {"literal": "GLUC"}
                    overlay["rows"][1]["submission"] = {
                        "LBORRESU": {
                            "origin": {"type": "Assigned", "source": "Vendor"},
                            "codelist": "CL_GLUCOSE",
                        },
                    }
                    self.assertEqual(
                        overlay["rows"][0]["submission"],
                        overlay["rows"][1]["submission"],
                    )
                elif case == "domain":
                    spec["domain"] = "XX"
                elif case == "dynamic":
                    first["derivations"]["LBTESTCD"] = {"source": "ODM.TestCode"}
                elif case == "unknown":
                    effective["BOGUS"] = effective.pop("LBORRESU")
                elif case == "output":
                    spec["output"]["columns"].remove("LBSTRESU")
                    # Keep the declared internal column reachable independently
                    # through the effective row filter, without adding it to output.
                    overlay["rows"][0]["filter"] = "LBSTRESU IS NOT NULL"
                    effective["LBSTRESU"] = {
                        "origin": {"type": "Assigned", "source": "Vendor"}
                    }
                elif case == "conflict":
                    spec["rows"][1]["derivations"]["LBTESTCD"] = {"literal": "GLUC"}
                    self.assertNotEqual(
                        overlay["rows"][0]["submission"],
                        overlay["rows"][1]["submission"],
                    )
                elif case == "origin":
                    # A literal unit reads no input, so it refutes Derived.
                    effective["LBORRESU"]["origin"]["type"] = "Derived"
                if case != "valid":
                    (directory / "spec.yaml").write_text(
                        yaml.safe_dump(spec, sort_keys=False)
                    )
                    (directory / "submission-spec.yaml").write_text(
                        yaml.safe_dump(overlay, sort_keys=False)
                    )
                result = self.call(directory)
                if requirement is None:
                    self.assertEqual(result.issues.height, 0, result.issues)
                    self.assertEqual(
                        [name for name, _, _ in result._native.artifacts()],
                        ["define.xml"],
                    )
                    xml = self.validate_xml(result._native.artifacts()[0][1])
                    if case == "identical duplicate":
                        ns = {"d": "http://www.cdisc.org/ns/def/v2.1"}
                        self.assertEqual(
                            len(xml.xpath("//d:WhereClauseDef", namespaces=ns)), 1
                        )
                else:
                    self.assertTrue(
                        any(
                            row["requirement"] == requirement
                            and row["condition"] == condition
                            and any(path.endswith(suffix) for path in row["spec_paths"])
                            for row in result.issues.to_dicts()
                        ),
                        result.issues,
                    )
                    with self.assertRaises(DefineError):
                        result.save()
                self.assertFalse((directory / "input").exists())
                self.assertFalse((directory / "lb.csv").exists())
                self.assertFalse((directory / "lb.json").exists())
                self.assertFalse((directory / "define.xml").exists())
                shutil.rmtree(directory)

    def test_dm_goldens_public_issues_and_held_save(self):
        directory = self.fixture("sdtm-dm-metadata", published=True)
        result = self.call(directory)
        self.assertEqual(result.issues.height, 0, result.issues)
        artifacts = result._native.artifacts()
        self.assertEqual(
            [(p, state) for p, _, state in artifacts],
            [("dm.json", "Ready"), ("define.xml", "Ready")],
        )
        for name, content, _ in artifacts:
            self.assertEqual(content, (directory / "expected" / name).read_bytes())
            self.assertFalse((directory / name).exists())
        self.validate_xml(artifacts[1][1])
        Draft202012Validator(
            json.loads((SCHEMAS / "dataset.schema.json").read_bytes())
        ).validate(json.loads(artifacts[0][1]))
        reads = result._native.capture_reads()
        (directory / "environment.yaml").unlink()
        (directory / "spec.yaml").unlink()
        (directory / "dm.csv").write_text("changed published artifact\n")
        self.assertTrue(result.save())
        self.assertEqual(result._native.capture_reads(), reads)
        self.assertFalse(result.save())
        for name in ["define.xml", "dm.json"]:
            self.assertEqual(
                (directory / name).read_bytes(),
                (directory / "expected" / name).read_bytes(),
            )

    def test_ae_vs_independent_fixed_time_schema_and_structure(self):
        ns = {
            "o": "http://www.cdisc.org/ns/odm/v1.3",
            "d": "http://www.cdisc.org/ns/def/v2.1",
        }
        expected = {
            "sdtm-ae-coding": ("AE", TRUTH["ae_columns"], "Submission", "MEDDRA", True),
            "sdtm-vs-epoch-from-subject-elements": (
                "VS",
                TRUTH["vs_columns"],
                "Other",
                "EPOCH",
                False,
            ),
        }
        for name, (id_, columns, context, codelist, acrf) in expected.items():
            with self.subTest(name=name):
                directory = self.fixture(name)
                result = self.call(directory, "2026-09-30T00:00:00")
                self.assertEqual(result.issues.height, 0, result.issues)
                artifacts = result._native.artifacts()
                self.assertEqual(len(artifacts), 1)
                xml = self.validate_xml(artifacts[0][1])
                self.assertEqual(xml.get("CreationDateTime"), "2026-09-30T00:00:00")
                self.assertEqual(xml.get("SourceSystem"), "yamaa")
                self.assertEqual(xml.get("SourceSystemVersion"), "1.0")
                self.assertEqual(xml.get("{" + ns["d"] + "}Context"), context)
                self.assertEqual(
                    xml.xpath("//o:ItemGroupDef/@OID", namespaces=ns), ["IG." + id_]
                )
                self.assertEqual(xml.xpath("//o:ItemDef/@Name", namespaces=ns), columns)
                self.assertEqual(
                    xml.xpath("//o:ItemRef/@OrderNumber", namespaces=ns),
                    [str(i) for i in range(1, len(columns) + 1)],
                )
                self.assertEqual(
                    xml.xpath("//o:CodeList/@OID", namespaces=ns), ["CL." + codelist]
                )
                self.assertEqual(
                    bool(xml.xpath("//d:AnnotatedCRF", namespaces=ns)), acrf
                )
                if id_ == "AE":
                    self.assertEqual(
                        xml.xpath("//o:ExternalCodeList/@Dictionary", namespaces=ns),
                        ["MedDRA"],
                    )
                    self.assertEqual(
                        xml.xpath("//o:MethodDef/@OID", namespaces=ns),
                        ["MT.AE.AEDECOD", "MT.AE.AEBODSYS"],
                    )
                else:
                    self.assertEqual(
                        xml.xpath(
                            "//o:CodeList/o:EnumeratedItem/@CodedValue", namespaces=ns
                        ),
                        ["SCREENING", "TREATMENT", "FOLLOW-UP"],
                    )
                    self.assertEqual(
                        xml.xpath(
                            "//o:ItemDef[@Name='VSDTC']/@DataType", namespaces=ns
                        ),
                        ["datetime"],
                    )
                self.assertFalse((directory / (id_.lower() + ".csv")).exists())
                self.assertTrue(result.save())
                self.assertEqual(
                    (directory / "define.xml").read_bytes(), artifacts[0][1]
                )

    def test_missing_artifact_only_when_requested_and_semantic_failure_zero_writes(
        self,
    ):
        directory = self.fixture("sdtm-dm-metadata")
        result = self.call(directory)
        self.assertIn(
            "producer_contract_mismatch", result.issues["condition"].to_list()
        )
        self.assertIn("REQ-0535", result.issues["requirement"].to_list())
        with self.assertRaises(DefineError):
            result.save()
        environment = directory / "environment.yaml"
        environment.write_text(
            environment.read_text().replace("  transport: dataset_json\n", "")
        )
        result = self.call(directory)
        self.assertEqual(result.issues.height, 0, result.issues)
        environment.write_text(
            environment.read_text().replace("name: SDTMIG", "name: ADaMIG")
        )
        result = self.call(directory)
        self.assertIn("unknown_standard_family", result.issues["condition"].to_list())
        with self.assertRaises(DefineError) as caught:
            result.save()
        self.assertTrue(caught.exception.issues.equals(result.issues))
        self.assertEqual(caught.exception.diagnostics, result.diagnostics)
        self.assertFalse((directory / "define.xml").exists())
        self.assertFalse((directory / "dm.json").exists())

    def test_output_preflight_failure_stops_all_outputs_and_preserves_cause(self):
        directory = self.fixture("sdtm-dm-metadata", published=True)
        result = self.call(directory)
        (directory / "define.xml").mkdir()
        with self.assertRaises(DefineError):
            result.save()
        self.assertFalse((directory / "dm.json").exists())
        self.assertEqual(
            [s for _, _, s in result._native.artifacts()], ["Stopped", "Failed"]
        )
        self.assertEqual(result.issues["phase"].to_list(), ["engine"])
        self.assertEqual(result.issues["condition"].to_list(), ["engine_rejected"])
        self.assertEqual(result.issues["requirement"].to_list(), [None])
        context = json.loads(result.issues["context"][0])
        self.assertEqual(context["stage"], "publication_target_admission")
        self.assertIn("cause", context)

    def test_source_version_defaults_and_schema_facet_failures(self):
        directory = self.fixture("sdtm-dm-metadata", published=True)
        environment = directory / "environment.yaml"
        environment.write_text(
            environment.read_text().replace("  source_system_version: '1.0'\n", "")
        )
        result = self.call(directory)
        self.assertEqual(result.issues.height, 0, result.issues)
        artifacts = result._native.artifacts()
        self.assertEqual(
            json.loads(artifacts[0][1])["sourceSystem"]["version"], yamaa.__version__
        )
        self.assertEqual(
            self.validate_xml(artifacts[1][1]).get("SourceSystemVersion"),
            yamaa.__version__,
        )
        environment.write_text(
            environment.read_text().replace("name: CDISC/NCI", "name: InvalidStandard")
        )
        result = self.call(directory)
        self.assertIn("REQ-1026", result.issues["requirement"].to_list())
        with self.assertRaises(DefineError):
            result.save()
        self.assertFalse((directory / "define.xml").exists())

    def test_defaults_selection_bound_lists_and_required_acrf_aliases(self):
        directory = self.fixture("sdtm-dm-metadata", published=True)
        environment = directory / "environment.yaml"
        document = yaml.safe_load(environment.read_text())
        section = document["sdtm"]
        del section["file_oid"], section["metadata_version"]
        document["adam"] = {
            "standard": {"name": "ADaMIG", "version": "1.3", "status": "Final"},
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
        environment.write_text(yaml.safe_dump(document, sort_keys=False))
        result = self.call(directory)
        self.assertEqual(result.issues.height, 0, result.issues)
        content = result._native.artifacts()[1][1]
        xml = self.validate_xml(content)
        self.assertEqual(xml.get("FileOID"), "STUDY01.sdtm.Define-XML_2.1")
        ns = {"o": "http://www.cdisc.org/ns/odm/v1.3"}
        self.assertEqual(
            xml.xpath("//o:MetaDataVersion/@OID", namespaces=ns), ["MDV.STUDY01.sdtm"]
        )
        self.assertEqual(
            xml.xpath("//o:MetaDataVersion/@Name", namespaces=ns),
            ["STUDY01 sdtm metadata"],
        )
        self.assertNotIn(b"UNUSED", content)
        self.assertEqual(
            xml.xpath("//o:CodeList/@OID", namespaces=ns),
            ["CL.DOMAIN", "CL.AGEU", "CL.SEX", "CL.COUNTRY"],
        )
        self.assertEqual(content, self.call(directory)._native.artifacts()[1][1])
        self.assertFalse((directory / "adam.xml").exists())
        section["documents"] = [
            d for d in section["documents"] if d["kind"] != "annotated_crf"
        ]
        environment.write_text(yaml.safe_dump(document, sort_keys=False))
        result = self.call(directory)
        self.assertIn("origin_document_missing", result.issues["condition"].to_list())
        with self.assertRaises(DefineError):
            result.save()
        self.assertFalse((directory / "dm.json").exists())
        section["documents"] = yaml.safe_load(
            (ROOT / "sdtm-dm-metadata/environment.yaml").read_text()
        )["sdtm"]["documents"]
        del document["codelists"][0]["codelists"][0]["alias"]
        environment.write_text(yaml.safe_dump(document, sort_keys=False))
        result = self.call(directory)
        self.assertIn("REQ-1004", result.issues["requirement"].to_list())
        with self.assertRaises(DefineError):
            result.save()
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
            "schema_version: '1.0'\nparent: ../parents/base.yaml\n"
        )
        environment = directory / "environment.yaml"
        environment.write_text(
            environment.read_text()
            .replace("  - spec.yaml", "  - children/spec.yaml")
            .replace("define: define.xml", "define: artifacts/define.xml")
        )
        shutil.copy2(directory / "expected/dm.csv", directory / "artifacts/dm.csv")
        result = self.call(directory)
        self.assertEqual(result.issues.height, 0, result.issues)
        self.assertIn(b'xlink:href="dm.json"', result._native.artifacts()[1][1])
        self.assertEqual(
            json.loads(result._native.artifacts()[0][1])["metaDataRef"], "define.xml"
        )
        self.assertTrue(result.save())
        self.assertTrue((directory / "artifacts/dm.json").is_file())
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
        self.assertIn("redundant_declaration", result.issues["condition"].to_list())
        self.assertIn("REQ-0955", result.issues["requirement"].to_list())
        previous = (directory / "artifacts/define.xml").read_bytes()
        with self.assertRaises(DefineError):
            result.save()
        self.assertEqual((directory / "artifacts/define.xml").read_bytes(), previous)

    def test_mutual_reference_and_stale_published_contract_refusals(self):
        directory = self.fixture("sdtm-dm-metadata")
        (directory / "artifacts").mkdir()
        spec = directory / "spec.yaml"
        original = spec.read_text()
        spec.write_text(original.replace("path: dm.csv", "path: artifacts/dm.csv"))
        shutil.copy2(directory / "expected/dm.csv", directory / "artifacts/dm.csv")
        # The transport file sits beside the document wherever the artifact is.
        result = self.call(directory)
        self.assertEqual(result.issues.height, 0, result.issues)
        self.assertIn(b'xlink:href="dm.json"', result._native.artifacts()[1][1])
        self.assertTrue(result.save())
        self.assertTrue((directory / "dm.json").is_file())
        self.assertFalse((directory / "artifacts/dm.json").exists())
        (directory / "dm.json").unlink()
        (directory / "define.xml").unlink()
        spec.write_text(original)
        csv = (directory / "expected/dm.csv").read_text()
        (directory / "dm.csv").write_text(
            csv.replace("STUDYID,USUBJID", "USUBJID,STUDYID")
        )
        result = self.call(directory)
        self.assertIn("REQ-0535", result.issues["requirement"].to_list())
        with self.assertRaises(DefineError):
            result.save()
        (directory / "dm.csv").write_text(csv.replace("STUDYID", "WRONG"))
        result = self.call(directory)
        self.assertIn("REQ-0535", result.issues["requirement"].to_list())
        with self.assertRaises(DefineError):
            result.save()
        (directory / "dm.csv").write_text(csv.replace(",F,USA", ",Z,USA"))
        result = self.call(directory)
        issue = result.issues.filter(result.issues["requirement"] == "REQ-0957").row(
            0, named=True
        )
        self.assertEqual(issue["phase"], "verification")
        context = json.loads(issue["context"])
        self.assertEqual(context["value"], "Z")
        self.assertIn("CATH-UCSD-0001", context["key_values"])
        with self.assertRaises(DefineError):
            result.save()
        self.assertFalse((directory / "dm.json").exists())

    def test_transport_names_equal_after_lowercasing_preserve_publication_sentinel(
        self,
    ):
        directory = self.fixture("sdtm-dm-metadata", published=True)
        sentinel = directory / "dm.json"
        sentinel.write_bytes(b"original publication sentinel")
        environment = directory / "environment.yaml"
        environment.write_text(
            environment.read_text()
            .replace("  - spec.yaml", "  - spec.yaml\n  - second.yaml")
            .replace("context: Submission", "context: Other")
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
        self.assertEqual(result.issues["phase"].to_list(), ["validation"])
        self.assertEqual(
            result.issues["condition"].to_list(), ["transport_path_collision"]
        )
        self.assertEqual(result.issues["requirement"].to_list(), ["REQ-1226"])
        self.assertEqual(result.issues["spec_paths"].to_list(), [["sdtm.specs[1]"]])
        self.assertIn("dm.json", result.issues["context"][0])
        reads = result._native.capture_reads()
        with self.assertRaises(DefineError):
            result.save()
        self.assertEqual(result._native.capture_reads(), reads)
        self.assertEqual(sentinel.read_bytes(), b"original publication sentinel")
        self.assertEqual(define_sentinel.read_bytes(), b"original Define sentinel")
        self.assertEqual(sorted(path.name for path in directory.iterdir()), names)
        self.assertTrue(
            all(state == "Stopped" for _, _, state in result._native.artifacts())
        )

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
        environment = directory / "environment.yaml"
        environment.write_text(
            environment.read_text().replace("sdtm:\n", TRUTH["numeric_codelist_yaml"])
        )
        result = self.call(directory)
        self.assertEqual(result.issues.height, 0, result.issues)
        self.validate_xml(result._native.artifacts()[1][1])
        data = json.loads(result._native.artifacts()[0][1])
        self.assertEqual([row[5] for row in data["rows"]], [34.0, 67.0, 52.0])

    def test_xml_text_findings_name_the_original_field_and_keep_render_order(self):
        for case in TRUTH["xml_text_cases"]:
            with self.subTest(case=case["name"]):
                directory = self.fixture("sdtm-dm-metadata")
                for filename, mutations in [
                    ("spec.yaml", case["specification_mutations"]),
                    ("environment.yaml", case["environment_mutations"]),
                ]:
                    path = directory / filename
                    text = path.read_text()
                    for before, after in mutations:
                        self.assertIn(before, text)
                        text = text.replace(before, after, 1)
                    if filename == "environment.yaml":
                        text = text.replace("  transport: dataset_json\n", "")
                    path.write_text(text)
                if case["inherit_specification"]:
                    (directory / "spec.yaml").rename(directory / "base.yaml")
                    (directory / "spec.yaml").write_text(
                        "schema_version: '1.0'\nparent: base.yaml\n"
                    )
                result = self.call(directory)
                actual = [
                    {**row, "context": json.loads(row["context"])}
                    for row in result.issues.to_dicts()
                ]
                self.assertEqual(actual, case["issues"])
                self.assertFalse((directory / "define.xml").exists())
                self.assertFalse((directory / "dm.json").exists())
                if case["issues"]:
                    with self.assertRaises(DefineError):
                        result.save()
                    self.assertEqual(result._native.artifacts(), [])
                else:
                    [(name, content, state)] = result._native.artifacts()
                    self.assertEqual((name, state), ("define.xml", "Ready"))
                    self.validate_xml(content)
                    for fragment in case["xml_fragments"]:
                        self.assertIn(fragment.encode(), content)
                    self.assertTrue(result.save())
                    self.assertEqual((directory / name).read_bytes(), content)
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
                result = define(
                    directory / case["environment"],
                    case["standard"],
                    created=case["created"],
                )
                if case["error"] is not None:
                    first = result.issues.to_dicts()[0]
                    first["context"] = json.loads(first["context"])
                    want = case["error"]
                    for key in ("phase", "condition", "spec_paths", "requirement"):
                        self.assertEqual(first[key], want[key])
                    for key, value in want.get("context", {}).items():
                        self.assertEqual(first["context"][key], value, key)
                    with self.assertRaises(DefineError):
                        result.save()
                    self.assertFalse((directory / "define.xml").exists())
                    continue
                self.assertEqual(result.issues.height, 0, result.issues)
                held = result._native.artifacts()
                self.assertEqual(
                    sorted(name for name, _, _ in held), sorted(case["expected"])
                )
                for name, content, state in held:
                    self.assertEqual(state, "Ready")
                    self.assertEqual(
                        content, (source / "expected" / name).read_bytes(), name
                    )
                    if name == "define.xml":
                        self.validate_xml(content)
                self.assertTrue(result.save())
                for name, content, _ in held:
                    self.assertEqual((directory / name).read_bytes(), content)


if __name__ == "__main__":
    unittest.main()
