"""Installed public Define with independent XML/JSON oracles and no reference imports."""
import builtins
import json
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import yamaa
from jsonschema import Draft202012Validator
from lxml import etree
from yamaa import DefineError, define

import yaml

ROOT = Path(__file__).with_name("define-fixtures")
SCHEMAS = Path(yamaa.__file__).parent / "submission/schemas"


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
            shutil.copy2(directory / "expected" / f"{dataset}.csv", directory / f"{dataset}.csv")
        self.assertFalse((directory / "input").exists())
        return directory

    def call(self, directory, created="2026-09-11T00:00:00"):
        original = builtins.__import__
        def guarded(name, *args, **kwargs):
            if name.startswith(("yamaa.specification", "yamaa.submission", "yamaa.expressions", "yamaa._reference_domain")):
                raise AssertionError("Define imported reference semantics: " + name)
            return original(name, *args, **kwargs)
        with patch("builtins.__import__", guarded):
            result = define(directory / "environment.yaml", "sdtm", created=created)
        self.assertEqual(result.issues.columns, ["phase", "condition", "requirement", "spec_paths", "context"])
        return result

    def validate_xml(self, content):
        parser = etree.XMLParser(no_network=True, resolve_entities=False)
        schema = etree.XMLSchema(etree.parse(str(SCHEMAS / "define/2.1/define2-1-0.xsd"), parser))
        root = etree.fromstring(content, parser)
        schema.assertValid(root)
        return root

    def test_lb_independent_completion_value_links_schema_json_and_held_save(self):
        directory = self.fixture("sdtm-lb-metadata", published=True)
        result = self.call(directory)
        self.assertEqual(result.issues.height, 0, result.issues)
        artifacts = result._native.artifacts()
        self.assertEqual([(name, state) for name, _, state in artifacts], [("lb.json", "Ready"), ("define.xml", "Ready")])
        xml = self.validate_xml(artifacts[1][1])
        ns = {"o": "http://www.cdisc.org/ns/odm/v1.3", "d": "http://www.cdisc.org/ns/def/v2.1"}
        self.assertEqual(xml.get("CreationDateTime"), "2026-09-11T00:00:00")
        self.assertEqual(xml.get("SourceSystemVersion"), "1.0")
        self.assertEqual(xml.xpath("//d:ValueListDef/@OID", namespaces=ns), ["VL.LB.LBORRESU"])
        self.assertEqual(xml.xpath("//o:ItemDef[d:ValueListRef]/@OID", namespaces=ns), ["IT.LB.LBORRESU"])
        refs = xml.xpath("//d:ValueListDef/o:ItemRef", namespaces=ns)
        self.assertEqual([ref.get("OrderNumber") for ref in refs], ["1", "2"])
        self.assertEqual([ref.get("Mandatory") for ref in refs], ["No", "No"])
        self.assertEqual(len(xml.xpath("//o:ItemDef", namespaces=ns)), 13)
        for ordinal, code, source, codelist in [(1, "GLUC", "Vendor", "CL_GLUCOSE"), (2, "CREAT", "Sponsor", "CL_CREATININE")]:
            identity = f"LB.LBORRESU.V{ordinal}"
            self.assertEqual(refs[ordinal - 1].get("ItemOID"), "IT." + identity)
            self.assertEqual(refs[ordinal - 1].xpath("d:WhereClauseRef/@WhereClauseOID", namespaces=ns), ["WC." + identity])
            item = xml.xpath("//o:ItemDef[@OID=$oid]", namespaces=ns, oid="IT." + identity)[0]
            self.assertEqual((item.get("Name"), item.get("DataType"), item.get("Length")), ("LBORRESU", "text", "8"))
            self.assertEqual(item.xpath("o:CodeListRef/@CodeListOID", namespaces=ns), ["CL." + codelist])
            self.assertEqual(item.xpath("d:Origin/@Type", namespaces=ns), ["Assigned"])
            self.assertEqual(item.xpath("d:Origin/@Source", namespaces=ns), [source])
            check = xml.xpath("//d:WhereClauseDef[@OID=$oid]/o:RangeCheck", namespaces=ns, oid="WC." + identity)[0]
            self.assertEqual((check.get("Comparator"), check.get("SoftHard"), check.get("{" + ns["d"] + "}ItemOID")), ("EQ", "Soft", "IT.LB.LBTESTCD"))
            self.assertEqual(check.xpath("o:CheckValue/text()", namespaces=ns), [code])
        data = json.loads(artifacts[0][1])
        Draft202012Validator(json.loads((SCHEMAS / "dataset.schema.json").read_bytes())).validate(data)
        self.assertEqual([column["name"] for column in data["columns"]], ["DOMAIN", "STUDYID", "USUBJID", "LBSEQ", "LBTESTCD", "LBTEST", "LBORRES", "LBORRESU", "LBSTRESN", "LBSTRESU", "LBDTC"])
        self.assertEqual(data["rows"], [
            ["LB", "STUDY01", "001", 2, "GLUC", "Glucose", "95", "mg/dL", 95.0, "mg/dL", "2025-01-01"],
            ["LB", "STUDY01", "002", 1, "GLUC", "Glucose", "101", "mg/dL", 101.0, "mg/dL", "2025-01-02"],
            ["LB", "STUDY01", "001", 1, "CREAT", "Creatinine", "0.9", "mg/dL", 0.9, "mg/dL", "2025-01-01"],
        ])
        reads = result._native.capture_reads()
        for name in ["spec.yaml", "submission-spec.yaml", "environment.yaml"]:
            (directory / name).unlink()
            self.assertFalse((directory / name).exists())
        (directory / "lb.csv").write_text("changed after preparation\n")
        self.assertTrue(result.save())
        self.assertFalse(result.save())
        self.assertEqual(result._native.capture_reads(), reads)
        for name, content, _ in artifacts:
            self.assertEqual((directory / name).read_bytes(), content)

    def test_lb_original_source_and_selected_value_codelist_rejections_write_nothing(self):
        for mode in ["original source", "value codelist"]:
            with self.subTest(mode=mode):
                directory = self.fixture("sdtm-lb-metadata", published=True)
                if mode == "original source":
                    # Original spec.yaml stays byte-identical. Only the independent
                    # completion's explicit Sponsor override is removed.
                    overlay = yaml.safe_load((directory / "submission-spec.yaml").read_text())
                    del overlay["rows"]
                    (directory / "submission-spec.yaml").write_text(yaml.safe_dump(overlay, sort_keys=False))
                else:
                    (directory / "lb.csv").write_text((directory / "lb.csv").read_text().replace("95,mg/dL,95", "95,mmol/L,95"))
                result = self.call(directory)
                issues = result.issues.to_dicts()
                if mode == "original source":
                    self.assertTrue(any(row["requirement"] == "REQ-0919" and row["condition"] == "origin_source_not_admitted" for row in issues), issues)
                    self.assertEqual((directory / "spec.yaml").read_bytes(), (ROOT / "sdtm-lb-metadata/spec.yaml").read_bytes())
                else:
                    issue = next(row for row in issues if row["condition"] == "allowed_values_failed")
                    self.assertEqual(issue["requirement"], "REQ-0957")
                    context = json.loads(issue["context"])
                    self.assertEqual((context["column"], context["codelist"], context["testcd"], context["row"]), ("LBORRESU", "CL_GLUCOSE", "GLUC", 0))
                    self.assertTrue(issue["spec_paths"][0].endswith("rows.glucose.submission.LBORRESU.codelist"))
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
            ("domain", "REQ-1163", "value_metadata_requires_testcd", "rows.glucose.submission"),
            ("dynamic", "REQ-1164", "value_metadata_requires_literal_testcd", "rows.glucose.submission"),
            ("unknown", "REQ-1165", "value_metadata_unknown_column", "rows.glucose.submission.BOGUS"),
            ("output", "REQ-1166", "value_metadata_not_output_column", "rows.glucose.submission.LBSTRESU"),
            ("conflict", "REQ-1167", "conflicting_value_metadata", "rows.creatinine.submission.LBORRESU"),
            ("origin", "REQ-1168", "refuted_value_origin", "rows.glucose.submission.LBORRESU"),
        ]:
            with self.subTest(case=case):
                directory = self.fixture("sdtm-lb-metadata")
                environment = yaml.safe_load((directory / "environment.yaml").read_text())
                environment["sdtm"]["dataset_json"] = False
                (directory / "environment.yaml").write_text(yaml.safe_dump(environment, sort_keys=False))
                spec = yaml.safe_load((directory / "spec.yaml").read_text())
                overlay = yaml.safe_load((directory / "submission-spec.yaml").read_text())
                first = spec["rows"][0]
                effective = overlay["rows"][0]["submission"]
                if case == "arbitrary row ID":
                    first["id"] = "glucose / primary test: 1"
                    overlay["rows"][0]["id"] = first["id"]
                elif case == "identical duplicate":
                    spec["rows"][1]["derivations"]["LBTESTCD"] = {"literal": "GLUC"}
                    overlay["rows"][1]["submission"] = {
                        "LBORRESU": {"origin": {"type": "Assigned", "source": "Vendor"}, "codelist": "CL_GLUCOSE"},
                    }
                    self.assertEqual(overlay["rows"][0]["submission"], overlay["rows"][1]["submission"])
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
                    effective["LBSTRESU"] = {"origin": {"type": "Assigned", "source": "Vendor"}}
                elif case == "conflict":
                    spec["rows"][1]["derivations"]["LBTESTCD"] = {"literal": "GLUC"}
                    self.assertNotEqual(overlay["rows"][0]["submission"], overlay["rows"][1]["submission"])
                elif case == "origin":
                    effective["LBORRESU"]["origin"]["type"] = "Collected"
                if case != "valid":
                    (directory / "spec.yaml").write_text(yaml.safe_dump(spec, sort_keys=False))
                    (directory / "submission-spec.yaml").write_text(yaml.safe_dump(overlay, sort_keys=False))
                result = self.call(directory)
                if requirement is None:
                    self.assertEqual(result.issues.height, 0, result.issues)
                    self.assertEqual([name for name, _, _ in result._native.artifacts()], ["define.xml"])
                    xml = self.validate_xml(result._native.artifacts()[0][1])
                    if case == "identical duplicate":
                        ns = {"d": "http://www.cdisc.org/ns/def/v2.1"}
                        self.assertEqual(len(xml.xpath("//d:WhereClauseDef", namespaces=ns)), 1)
                else:
                    self.assertTrue(any(row["requirement"] == requirement and row["condition"] == condition
                        and any(path.endswith(suffix) for path in row["spec_paths"])
                        for row in result.issues.to_dicts()), result.issues)
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
        self.assertEqual([(p, state) for p, _, state in artifacts], [("dm.json", "Ready"), ("define.xml", "Ready")])
        for name, content, _ in artifacts:
            self.assertEqual(content, (directory / "expected" / name).read_bytes())
            self.assertFalse((directory / name).exists())
        self.validate_xml(artifacts[1][1])
        Draft202012Validator(json.loads((SCHEMAS / "dataset.schema.json").read_bytes())).validate(json.loads(artifacts[0][1]))
        reads = result._native.capture_reads()
        (directory / "environment.yaml").unlink()
        (directory / "spec.yaml").unlink()
        (directory / "dm.csv").write_text("changed published artifact\n")
        self.assertTrue(result.save())
        self.assertEqual(result._native.capture_reads(), reads)
        self.assertFalse(result.save())
        for name in ["define.xml", "dm.json"]:
            self.assertEqual((directory / name).read_bytes(), (directory / "expected" / name).read_bytes())

    def test_ae_vs_independent_fixed_time_schema_and_structure(self):
        ns = {"o": "http://www.cdisc.org/ns/odm/v1.3", "d": "http://www.cdisc.org/ns/def/v2.1"}
        expected = {
            "sdtm-ae-coding": ("AE", ["DOMAIN", "STUDYID", "USUBJID", "AESEQ", "AETERM", "AEDECOD", "AEBODSYS"], "Submission", "MEDDRA", True),
            "sdtm-vs-epoch-from-subject-elements": ("VS", ["DOMAIN", "STUDYID", "USUBJID", "VSSEQ", "VSTESTCD", "VSORRES", "VSDTC", "EPOCH"], "Other", "EPOCH", False),
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
                self.assertEqual(xml.xpath("//o:ItemGroupDef/@OID", namespaces=ns), ["IG." + id_])
                self.assertEqual(xml.xpath("//o:ItemDef/@Name", namespaces=ns), columns)
                self.assertEqual(xml.xpath("//o:ItemRef/@OrderNumber", namespaces=ns), [str(i) for i in range(1, len(columns) + 1)])
                self.assertEqual(xml.xpath("//o:CodeList/@OID", namespaces=ns), ["CL." + codelist])
                self.assertEqual(bool(xml.xpath("//d:AnnotatedCRF", namespaces=ns)), acrf)
                if id_ == "AE":
                    self.assertEqual(xml.xpath("//o:ExternalCodeList/@Dictionary", namespaces=ns), ["MedDRA"])
                    self.assertEqual(xml.xpath("//o:MethodDef/@OID", namespaces=ns), ["MT.AE.AEDECOD", "MT.AE.AEBODSYS"])
                else:
                    self.assertEqual(xml.xpath("//o:CodeList/o:EnumeratedItem/@CodedValue", namespaces=ns), ["SCREENING", "TREATMENT", "FOLLOW-UP"])
                    self.assertEqual(xml.xpath("//o:ItemDef[@Name='VSDTC']/@DataType", namespaces=ns), ["datetime"])
                self.assertFalse((directory / (id_.lower() + ".csv")).exists())
                self.assertTrue(result.save())
                self.assertEqual((directory / "define.xml").read_bytes(), artifacts[0][1])

    def test_missing_artifact_only_when_requested_and_semantic_failure_zero_writes(self):
        directory = self.fixture("sdtm-dm-metadata")
        result = self.call(directory)
        self.assertIn("producer_contract_mismatch", result.issues["condition"].to_list())
        self.assertIn("REQ-0535", result.issues["requirement"].to_list())
        with self.assertRaises(DefineError): result.save()
        environment = directory / "environment.yaml"
        environment.write_text(environment.read_text().replace("dataset_json: true", "dataset_json: false"))
        result = self.call(directory)
        self.assertEqual(result.issues.height, 0, result.issues)
        environment.write_text(environment.read_text().replace("name: SDTMIG", "name: ADaMIG"))
        result = self.call(directory)
        self.assertIn("unknown_standard_family", result.issues["condition"].to_list())
        with self.assertRaises(DefineError) as caught: result.save()
        self.assertTrue(caught.exception.issues.equals(result.issues))
        self.assertFalse((directory / "define.xml").exists())
        self.assertFalse((directory / "dm.json").exists())

    def test_output_preflight_failure_stops_all_outputs_and_preserves_cause(self):
        directory = self.fixture("sdtm-dm-metadata", published=True)
        result = self.call(directory)
        (directory / "define.xml").mkdir()
        with self.assertRaises(DefineError): result.save()
        self.assertFalse((directory / "dm.json").exists())
        self.assertEqual([s for _, _, s in result._native.artifacts()], ["Stopped", "Failed"])
        self.assertEqual(result.issues["phase"].to_list(), ["engine"])
        self.assertEqual(result.issues["condition"].to_list(), ["engine_rejected"])
        self.assertEqual(result.issues["requirement"].to_list(), [None])
        context = json.loads(result.issues["context"][0])
        self.assertEqual(context["stage"], "publication_target_admission")
        self.assertIn("cause", context)

    def test_source_version_defaults_and_schema_facet_failures(self):
        directory = self.fixture("sdtm-dm-metadata", published=True)
        environment = directory / "environment.yaml"
        environment.write_text(environment.read_text().replace("  source_system_version: '1.0'\n", ""))
        result = self.call(directory)
        self.assertEqual(result.issues.height, 0, result.issues)
        artifacts = result._native.artifacts()
        self.assertEqual(json.loads(artifacts[0][1])["sourceSystem"]["version"], yamaa.__version__)
        self.assertEqual(self.validate_xml(artifacts[1][1]).get("SourceSystemVersion"), yamaa.__version__)
        environment.write_text(environment.read_text().replace("name: CDISC/NCI", "name: InvalidStandard"))
        result = self.call(directory)
        self.assertIn("REQ-1026", result.issues["requirement"].to_list())
        with self.assertRaises(DefineError): result.save()
        self.assertFalse((directory / "define.xml").exists())

    def test_defaults_selection_bound_lists_and_required_acrf_aliases(self):
        directory = self.fixture("sdtm-dm-metadata", published=True)
        environment = directory / "environment.yaml"
        document = yaml.safe_load(environment.read_text())
        section = document["sdtm"]
        del section["file_oid"], section["metadata_version"]
        document["adam"] = {
            "standard": {"name": "ADaMIG", "version": "1.3", "status": "Final"},
            "specs": ["missing-other-section.yaml"], "define": "adam.xml",
        }
        document["codelists"].append({"codelists": [
            {"id": "UNUSED", "name": "Used only by another submission", "data_type": "text", "items": [{"value": "OTHER"}]},
        ]})
        environment.write_text(yaml.safe_dump(document, sort_keys=False))
        result = self.call(directory)
        self.assertEqual(result.issues.height, 0, result.issues)
        content = result._native.artifacts()[1][1]
        xml = self.validate_xml(content)
        self.assertEqual(xml.get("FileOID"), "STUDY01.sdtm.Define-XML_2.1")
        ns = {"o": "http://www.cdisc.org/ns/odm/v1.3"}
        self.assertEqual(xml.xpath("//o:MetaDataVersion/@OID", namespaces=ns), ["MDV.STUDY01.sdtm"])
        self.assertEqual(xml.xpath("//o:MetaDataVersion/@Name", namespaces=ns), ["STUDY01 sdtm metadata"])
        self.assertNotIn(b"UNUSED", content)
        self.assertEqual(xml.xpath("//o:CodeList/@OID", namespaces=ns), ["CL.DOMAIN", "CL.AGEU", "CL.SEX", "CL.COUNTRY"])
        self.assertEqual(content, self.call(directory)._native.artifacts()[1][1])
        self.assertFalse((directory / "adam.xml").exists())
        section["documents"] = [d for d in section["documents"] if d["kind"] != "annotated_crf"]
        environment.write_text(yaml.safe_dump(document, sort_keys=False))
        result = self.call(directory)
        self.assertIn("origin_document_missing", result.issues["condition"].to_list())
        with self.assertRaises(DefineError): result.save()
        self.assertFalse((directory / "dm.json").exists())
        section["documents"] = yaml.safe_load((ROOT / "sdtm-dm-metadata/environment.yaml").read_text())["sdtm"]["documents"]
        del document["codelists"][0]["codelists"][0]["alias"]
        environment.write_text(yaml.safe_dump(document, sort_keys=False))
        result = self.call(directory)
        self.assertIn("REQ-1004", result.issues["requirement"].to_list())
        with self.assertRaises(DefineError): result.save()
        self.assertFalse((directory / "define.xml").exists())

    def test_inherited_original_output_and_metadata_refusals(self):
        directory = self.fixture("sdtm-dm-metadata")
        for name in ["parents", "children", "artifacts"]: (directory / name).mkdir()
        spec = (directory / "spec.yaml").read_text().replace("path: dm.csv", "path: ../artifacts/dm.csv")
        (directory / "parents/base.yaml").write_text(spec)
        (directory / "children/spec.yaml").write_text("schema_version: '1.0'\nparents: ../parents/base.yaml\n")
        environment = directory / "environment.yaml"
        environment.write_text(environment.read_text().replace("  - spec.yaml", "  - children/spec.yaml").replace("define: define.xml", "define: artifacts/define.xml"))
        shutil.copy2(directory / "expected/dm.csv", directory / "artifacts/dm.csv")
        result = self.call(directory)
        self.assertEqual(result.issues.height, 0, result.issues)
        self.assertIn(b'xlink:href="dm.json"', result._native.artifacts()[1][1])
        self.assertEqual(json.loads(result._native.artifacts()[0][1])["metaDataRef"], "define.xml")
        self.assertTrue(result.save())
        self.assertTrue((directory / "artifacts/dm.json").is_file())
        self.assertFalse((directory / "dm.json").exists())
        (directory / "parents/base.yaml").write_text(spec.replace("values: [F, M, U]", "values: [F, M]"))
        result = self.call(directory)
        self.assertIn("codelist_values_conflict", result.issues["condition"].to_list())
        self.assertIn("REQ-0955", result.issues["requirement"].to_list())
        previous = (directory / "artifacts/define.xml").read_bytes()
        with self.assertRaises(DefineError): result.save()
        self.assertEqual((directory / "artifacts/define.xml").read_bytes(), previous)

    def test_mutual_reference_and_stale_published_contract_refusals(self):
        directory = self.fixture("sdtm-dm-metadata")
        (directory / "artifacts").mkdir()
        spec = directory / "spec.yaml"
        original = spec.read_text()
        spec.write_text(original.replace("path: dm.csv", "path: artifacts/dm.csv"))
        result = self.call(directory)
        self.assertIn("REQ-1229", result.issues["requirement"].to_list())
        with self.assertRaises(DefineError): result.save()
        self.assertFalse((directory / "define.xml").exists())
        spec.write_text(original)
        csv = (directory / "expected/dm.csv").read_text()
        (directory / "dm.csv").write_text(csv.replace("STUDYID,USUBJID", "USUBJID,STUDYID"))
        result = self.call(directory)
        self.assertIn("REQ-0535", result.issues["requirement"].to_list())
        with self.assertRaises(DefineError): result.save()
        (directory / "dm.csv").write_text(csv.replace("STUDYID", "WRONG"))
        result = self.call(directory)
        self.assertIn("REQ-0535", result.issues["requirement"].to_list())
        with self.assertRaises(DefineError): result.save()
        (directory / "dm.csv").write_text(csv.replace(",F,USA", ",Z,USA"))
        result = self.call(directory)
        issue = result.issues.filter(result.issues["requirement"] == "REQ-0957").row(0, named=True)
        self.assertEqual(issue["phase"], "verification")
        context = json.loads(issue["context"])
        self.assertEqual(context["value"], "Z")
        self.assertIn("CATH-UCSD-0001", context["key_values"])
        with self.assertRaises(DefineError): result.save()
        self.assertFalse((directory / "dm.json").exists())

    def test_physical_case_aliases_preserve_existing_publication_sentinel(self):
        directory = self.fixture("sdtm-dm-metadata", published=True)
        sentinel = directory / "dm.json"
        sentinel.write_bytes(b"original publication sentinel")
        if not (directory / "DM.JSON").exists():
            self.skipTest("This volume admits distinct case-sensitive target names")
        environment = directory / "environment.yaml"
        environment.write_text(environment.read_text().replace("  - spec.yaml", "  - spec.yaml\n  - second.yaml"))
        (directory / "second.yaml").write_text((directory / "spec.yaml").read_text().replace("domain: DM", "domain: DM2").replace("path: dm.csv", "path: DM.csv"))
        define_sentinel = directory / "define.xml"
        define_sentinel.write_bytes(b"original Define sentinel")
        names = sorted(path.name for path in directory.iterdir())
        result = self.call(directory)
        self.assertEqual(result.issues["phase"].to_list(), ["validation"])
        self.assertEqual(result.issues["condition"].to_list(), ["dataset_json_path_collision"])
        self.assertEqual(result.issues["requirement"].to_list(), ["REQ-1226"])
        context = result.issues["context"][0]
        self.assertIn("dm.json", context)
        self.assertIn("DM.json", context)
        reads = result._native.capture_reads()
        with self.assertRaises(DefineError): result.save()
        self.assertEqual(result._native.capture_reads(), reads)
        self.assertEqual(sentinel.read_bytes(), b"original publication sentinel")
        self.assertEqual(define_sentinel.read_bytes(), b"original Define sentinel")
        self.assertEqual(sorted(path.name for path in directory.iterdir()), names)
        self.assertTrue(all(state == "Stopped" for _, _, state in result._native.artifacts()))

    def test_numeric_terminology_uses_numeric_equality(self):
        directory = self.fixture("sdtm-dm-metadata", published=True)
        spec = directory / "spec.yaml"
        spec.write_text(spec.read_text().replace("name: AGE\n    type: int", "name: AGE\n    type: float").replace("core: Exp\n      role: Record Qualifier\n      length: 3", "core: Exp\n      role: Record Qualifier\n      length: 3\n      significant_digits: 3\n      codelist: AGE_VALUES"))
        environment = directory / "environment.yaml"
        environment.write_text(environment.read_text().replace("sdtm:\n", "- codelists:\n  - id: AGE_VALUES\n    name: Ages\n    data_type: float\n    items: [{value: 34}, {value: 67}, {value: 52}]\nsdtm:\n"))
        result = self.call(directory)
        self.assertEqual(result.issues.height, 0, result.issues)
        self.validate_xml(result._native.artifacts()[1][1])
        data = json.loads(result._native.artifacts()[0][1])
        self.assertEqual([row[5] for row in data["rows"]], [34.0, 67.0, 52.0])


if __name__ == "__main__": unittest.main()
