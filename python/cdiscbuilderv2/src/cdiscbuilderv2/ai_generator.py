"""
AI and CDISC Expert Knowledge Engine for auto-generating Yamaa YAML specifications
from ingested EDC ODM XML metadata (ItemDefs, FormDefs, CodeLists, Sample Values).
Supports CRF Form-driven domain exploration and multi-provider LLM synthesis.
"""

import json
import logging
import os
import re
from typing import Any, Dict, List, Optional
import httpx
import polars as pl
import yaml

logger = logging.getLogger(__name__)

# Standard CDISC SDTM Domain Knowledge Dictionary
CDISC_SDTM_STANDARDS = {
    "DM": {
        "class": "SPECIAL_PURPOSE",
        "description": "Demographics",
        "keys": ["STUDYID", "USUBJID"],
        "keywords": ["demog", "dm", "subject", "baseline", "patient"],
        "variables": {
            "SEX": {"keywords": ["sex", "gender"], "codelist": {"Male": "M", "Female": "F", "M": "M", "F": "F"}},
            "AGE": {"keywords": ["age", "age_yr", "years"], "type": "int"},
            "AGEU": {"keywords": ["ageu", "age_unit"], "literal": "YEARS"},
            "RACE": {"keywords": ["race", "ethnicity_race"]},
            "ETHNIC": {"keywords": ["ethnic", "ethnicity"]},
            "COUNTRY": {"keywords": ["country", "nation", "site_country"]},
            "ARMCD": {"keywords": ["armcd", "arm_cd", "trtcd"]},
            "ARM": {"keywords": ["arm", "treatment_arm", "planned_arm", "random", "group"]},
            "ACTARMCD": {"keywords": ["actarmcd", "actual_armcd"]},
            "ACTARM": {"keywords": ["actarm", "actual_arm"]},
            "RFSTDTC": {"keywords": ["rfstdtc", "start_date", "first_dose", "rand_dtc"], "type": "date"}
        }
    },
    "VS": {
        "class": "FINDINGS",
        "description": "Vital Signs",
        "keys": ["STUDYID", "USUBJID", "VSSEQ"],
        "keywords": ["vital", "vs", "vitalsigns", "bp", "pulse", "temp", "weight", "height"],
        "tests": {
            "SYSBP": {"name": "Systolic Blood Pressure", "unit": "mmHg", "keywords": ["sysbp", "systolic", "sbp"]},
            "DIABP": {"name": "Diastolic Blood Pressure", "unit": "mmHg", "keywords": ["diabp", "diastolic", "dbp"]},
            "PULSE": {"name": "Pulse Rate", "unit": "BEATS/MIN", "keywords": ["pulse", "heartrate", "hr"]},
            "TEMP": {"name": "Temperature", "unit": "C", "keywords": ["temp", "temperature"]},
            "RESP": {"name": "Respiratory Rate", "unit": "BREATHS/MIN", "keywords": ["resp", "respiration", "rr"]},
            "WEIGHT": {"name": "Weight", "unit": "kg", "keywords": ["weight", "wt", "bodyweight"]},
            "HEIGHT": {"name": "Height", "unit": "cm", "keywords": ["height", "ht"]}
        }
    },
    "LB": {
        "class": "FINDINGS",
        "description": "Laboratory Test Results",
        "keys": ["STUDYID", "USUBJID", "LBSEQ"],
        "keywords": ["lab", "lb", "laboratory", "blood", "chemistry", "hematology", "urinalysis", "biopsy", "bx", "sal", "sw", "saliva", "swab"],
        "tests": {
            "ALB": {"name": "Albumin", "unit": "g/dL", "cat": "CHEMISTRY", "keywords": ["alb", "albumin"]},
            "ALT": {"name": "Alanine Aminotransferase", "unit": "U/L", "cat": "CHEMISTRY", "keywords": ["alt", "sgpt"]},
            "AST": {"name": "Aspartate Aminotransferase", "unit": "U/L", "cat": "CHEMISTRY", "keywords": ["ast", "sgot"]},
            "BILI": {"name": "Bilirubin", "unit": "mg/dL", "cat": "CHEMISTRY", "keywords": ["bili", "bilirubin", "tbil"]},
            "CREAT": {"name": "Creatinine", "unit": "mg/dL", "cat": "CHEMISTRY", "keywords": ["creat", "creatinine"]},
            "GLUC": {"name": "Glucose", "unit": "mg/dL", "cat": "CHEMISTRY", "keywords": ["gluc", "glucose", "fbs"]},
            "HGB": {"name": "Hemoglobin", "unit": "g/dL", "cat": "HEMATOLOGY", "keywords": ["hgb", "hemoglobin", "hb"]},
            "WBC": {"name": "Leukocytes", "unit": "10^3/uL", "cat": "HEMATOLOGY", "keywords": ["wbc", "leukocytes", "white_blood_cell"]},
            "PLAT": {"name": "Platelets", "unit": "10^3/uL", "cat": "HEMATOLOGY", "keywords": ["plat", "platelets", "plt"]},
            "CALCIUM": {"name": "Calcium", "unit": "mg/dL", "cat": "CHEMISTRY", "keywords": ["calcium", "ca"]},
            "VITD25OH": {"name": "25-Hydroxyvitamin D", "unit": "ng/mL", "cat": "CHEMISTRY", "keywords": ["vitd", "vitd25oh", "vitamin_d"]}
        }
    },
    "MB": {
        "class": "FINDINGS",
        "description": "Microbiology Specimen",
        "keys": ["STUDYID", "USUBJID", "MBSEQ"],
        "keywords": ["micro", "mb", "microbiology", "culture", "organism", "pathogen"],
        "tests": {
            "ORGANISM": {"name": "Organism Identified", "unit": None, "keywords": ["organism", "isolate", "species"]},
            "COLONY": {"name": "Colony Count", "unit": "CFU/mL", "keywords": ["colony", "count", "cfu"]},
            "SUSCEPT": {"name": "Susceptibility", "unit": None, "keywords": ["suscept", "sensitivity", "mic"]}
        }
    },
    "RS": {
        "class": "FINDINGS",
        "description": "Disease Response / Clinical Assessments",
        "keys": ["STUDYID", "USUBJID", "RSSEQ"],
        "keywords": ["response", "rs", "pasi", "skin", "sk", "severity_index", "lesion", "assessment", "efficacy", "score"],
        "tests": {
            "PASI": {"name": "Psoriasis Area and Severity Index", "unit": "", "cat": "DISEASE SEVERITY", "keywords": ["pasi", "sk", "skin"]}
        }
    },
    "RP": {
        "class": "FINDINGS",
        "description": "Reproductive System Findings",
        "keys": ["STUDYID", "USUBJID", "RPSEQ"],
        "keywords": ["reproductive", "rp", "preg", "pregnancy", "pg", "pt"],
        "tests": {
            "PREGIND": {"name": "Pregnancy Test Indicator", "unit": "", "cat": "PREGNANCY", "keywords": ["pgres", "preg", "pregnancy", "pt"]}
        }
    },
    "PE": {
        "class": "FINDINGS",
        "description": "Physical Examination",
        "keys": ["STUDYID", "USUBJID", "PESEQ"],
        "keywords": ["pe", "physical", "exam", "phys_exam", "body_system"],
        "tests": {
            "PEEVAL": {"name": "Physical Examination Result", "unit": "", "keywords": ["eval", "pe", "result"]}
        }
    },
    "QS": {
        "class": "FINDINGS",
        "description": "Questionnaires / Patient Reported Outcomes",
        "keys": ["STUDYID", "USUBJID", "QSSEQ"],
        "keywords": ["qs", "questionnaire", "pro", "qol", "survey", "sf36", "promis"]
    },
    "AE": {
        "class": "EVENTS",
        "description": "Adverse Events",
        "keys": ["STUDYID", "USUBJID", "AESEQ"],
        "keywords": ["adverse", "ae", "safety", "toxicity", "event", "sae"],
        "variables": {
            "AETERM": {"keywords": ["aeterm", "term", "event_term", "verbatim", "toxicity_name"]},
            "AEDECOD": {"keywords": ["aedecod", "preferred_term", "pt", "pt_name", "meddra_pt"]},
            "AEBODSYS": {"keywords": ["aebodsys", "soc", "body_system", "system_organ_class"]},
            "AESEV": {"keywords": ["aesev", "severity", "grade", "intensity"], "codelist": {"Mild": "MILD", "Moderate": "MODERATE", "Severe": "SEVERE", "1": "MILD", "2": "MODERATE", "3": "SEVERE"}},
            "AESER": {"keywords": ["aeser", "serious", "is_serious", "sae"], "codelist": {"Yes": "Y", "No": "N", "1": "Y", "0": "N", "True": "Y", "False": "N"}},
            "AEREL": {"keywords": ["aerel", "causality", "relationship", "related"], "codelist": {"Related": "RELATED", "Not Related": "NOT RELATED", "Possible": "POSSIBLE"}},
            "AESTDTC": {"keywords": ["aestdtc", "onset_date", "start_date", "ae_stdtc"], "type": "date"},
            "AEENDTC": {"keywords": ["aeendtc", "resolution_date", "end_date", "ae_endtc"], "type": "date"},
            "AEOUT": {"keywords": ["aeout", "outcome", "action_taken"]}
        }
    },
    "CM": {
        "class": "INTERVENTIONS",
        "description": "Concomitant Medications",
        "keys": ["STUDYID", "USUBJID", "CMSEQ"],
        "keywords": ["cm", "concomitant", "medication", "med", "prior_med", "conmed"],
        "variables": {
            "CMTRT": {"keywords": ["cmtrt", "med_name", "drug_name", "medication", "verbatim"]},
            "CMDECOD": {"keywords": ["cmdecod", "standard_med", "whodrug_pt"]},
            "CMINDC": {"keywords": ["cmindc", "indication", "reason"]},
            "CMDOSE": {"keywords": ["cmdose", "dose", "amount"], "type": "float"},
            "CMDOSU": {"keywords": ["cmdosu", "unit"]},
            "CMSTDTC": {"keywords": ["cmstdtc", "start_date", "med_start"], "type": "date"},
            "CMENDTC": {"keywords": ["cmendtc", "end_date", "med_end"], "type": "date"}
        }
    },
    "EX": {
        "class": "INTERVENTIONS",
        "description": "Exposure / Study Treatment",
        "keys": ["STUDYID", "USUBJID", "EXSEQ"],
        "keywords": ["exposure", "ex", "dosing", "drug", "treatment", "administration"],
        "variables": {
            "EXTRT": {"keywords": ["extrt", "drug_name", "treatment", "study_drug", "agent"]},
            "EXDOSE": {"keywords": ["exdose", "dose", "dose_amount", "amount"], "type": "float"},
            "EXDOSU": {"keywords": ["exdosu", "dose_unit", "unit"]},
            "EXROUTE": {"keywords": ["exroute", "route", "route_admin"]},
            "EXDOSFRQ": {"keywords": ["exdosfrq", "frequency", "freq"]},
            "EXSTDTC": {"keywords": ["exstdtc", "dose_date", "start_date", "date_admin"], "type": "date"},
            "EXENDTC": {"keywords": ["exendtc", "end_date", "last_dose_date"], "type": "date"}
        }
    },
    "DS": {
        "class": "EVENTS",
        "description": "Disposition",
        "keys": ["STUDYID", "USUBJID", "DSSEQ"],
        "keywords": ["disposition", "ds", "completion", "status", "study_exit", "withdrawal", "end_of_study", "dc", "discontinuation"],
        "variables": {
            "DSTERM": {"keywords": ["dsterm", "status", "reason", "completion_status", "disc_reason"]},
            "DSDECOD": {"keywords": ["dsdecod", "standard_status", "completion", "completed"]},
            "DSCAT": {"keywords": ["dscat", "category"], "literal": "DISPOSITION EVENT"},
            "DSSTDTC": {"keywords": ["dsstdtc", "exit_date", "comp_date", "date"], "type": "date"}
        }
    },
    "MH": {
        "class": "EVENTS",
        "description": "Medical History",
        "keys": ["STUDYID", "USUBJID", "MHSEQ"],
        "keywords": ["med_hist", "mh", "history", "condition", "past_medical", "prior_condition"],
        "variables": {
            "MHTERM": {"keywords": ["mhterm", "condition", "diagnosis", "disease_name"]},
            "MHDECOD": {"keywords": ["mhdecod", "pt", "preferred_term"]},
            "MHCAT": {"keywords": ["mhcat", "category", "body_system"]},
            "MHOCCUR": {"keywords": ["mhoccur", "occurred", "present"], "codelist": {"Yes": "Y", "No": "N", "1": "Y", "0": "N"}},
            "MHSTDTC": {"keywords": ["mhstdtc", "onset_date", "start_date", "diagnosis_date"], "type": "date"}
        }
    }
}


class AISDTMSchemaGenerator:
    """
    Intelligent AI & Rule-based Generator that inspects EDC CRF Forms & metadata
    and produces standard Yamaa YAML schemas for user-selected domains.
    """

    def __init__(
        self,
        metadata_df: Optional[pl.DataFrame] = None,
        df_long: Optional[pl.DataFrame] = None,
        parser: Optional[Any] = None,
        provider: str = "auto",
        api_key: Optional[str] = None,
        model_name: Optional[str] = None
    ):
        self.metadata_df = metadata_df
        self.df_long = df_long
        self.parser = parser
        self.provider = provider.lower() if provider else "auto"
        self.api_key = api_key or os.getenv("GEMINI_API_KEY") or os.getenv("ANTHROPIC_API_KEY") or os.getenv("OPENAI_API_KEY")
        self.model_name = model_name

        if self.provider == "auto":
            if self.api_key:
                if self.api_key.startswith("AIzaSy"):
                    self.provider = "gemini"
                elif self.api_key.startswith("sk-ant-"):
                    self.provider = "anthropic"
                elif self.api_key.startswith("sk-"):
                    self.provider = "openai"
                else:
                    self.provider = "gemini"
            else:
                self.provider = "local"

    def discover_crf_forms(self) -> List[Dict[str, Any]]:
        """
        Explores all CRF Forms from the ingested ODM XML and suggests matching CDISC SDTM domains.
        """
        if self.df_long is None or self.df_long.height == 0:
            return []

        forms = []
        meta_items = self.metadata_df.to_dicts() if self.metadata_df is not None else []
        unique_forms = sorted(list(set(self.df_long["FormOID"].unique().to_list())))

        for f_oid in unique_forms:
            f_items = [it for it in meta_items if it.get("FormOID") == f_oid]
            f_data = self.df_long.filter(pl.col("FormOID") == f_oid)
            
            form_name = f_oid.split(".")[-1]
            if self.parser and f_oid in self.parser.form_defs:
                form_name = self.parser.form_defs[f_oid].get("Name") or form_name

            # Match suggested domain
            best_domain = "FINDINGS"
            best_class = "FINDINGS"
            best_score = 0

            f_lower = f_oid.lower()
            items_text = " ".join([str(it.get("ItemOID", "")).lower() + " " + str(it.get("Question", "")).lower() for it in f_items])

            for d_name, d_info in CDISC_SDTM_STANDARDS.items():
                score = 0
                for kw in d_info.get("keywords", []):
                    if kw in f_lower:
                        score += 50
                    if kw in items_text:
                        score += 20

                if score > best_score:
                    best_score = score
                    best_domain = d_name
                    best_class = d_info.get("class", "FINDINGS")

            # Fallbacks
            if best_score == 0:
                if "dm" in f_lower or "demog" in f_lower:
                    best_domain, best_class = "DM", "SPECIAL_PURPOSE"
                elif "vs" in f_lower or "vital" in f_lower:
                    best_domain, best_class = "VS", "FINDINGS"
                elif "ae" in f_lower:
                    best_domain, best_class = "AE", "EVENTS"
                elif "ex" in f_lower or "dose" in f_lower:
                    best_domain, best_class = "EX", "INTERVENTIONS"
                else:
                    best_domain = form_name.upper()[:8]

            forms.append({
                "FormOID": f_oid,
                "FormName": form_name,
                "ItemCount": len(f_items),
                "TotalRecords": f_data.height,
                "SubjectCount": len(f_data["SubjectKey"].unique()),
                "SuggestedDomain": best_domain,
                "SuggestedClass": best_class,
                "Confidence": min(100, max(50, best_score)) if best_score > 0 else 60,
                "Items": f_items
            })

        return forms

    def discover_domains(self) -> List[Dict[str, Any]]:
        """Scans forms and groups them into domain generation manifests."""
        crf_forms = self.discover_crf_forms()
        domains_map: Dict[str, Dict[str, Any]] = {}

        for f in crf_forms:
            d_name = f["SuggestedDomain"]
            if d_name not in domains_map:
                d_info = CDISC_SDTM_STANDARDS.get(d_name, {})
                domains_map[d_name] = {
                    "domain": d_name,
                    "class": d_info.get("class", f["SuggestedClass"]),
                    "description": d_info.get("description", f"SDTM {d_name} Domain"),
                    "confidence": f["Confidence"],
                    "forms": [f["FormOID"]],
                    "matched_items": list(f["Items"])
                }
            else:
                domains_map[d_name]["forms"].append(f["FormOID"])
                domains_map[d_name]["matched_items"].extend(f["Items"])

        discovered = list(domains_map.values())
        discovered.sort(key=lambda x: x["confidence"], reverse=True)
        return discovered

    def generate_all_schemas(self, custom_prompt: Optional[str] = None) -> Dict[str, str]:
        """Auto-generates Yamaa YAML schemas for all detected domains."""
        domains = self.discover_domains()
        results = {}
        for d in domains:
            domain_name = d["domain"]
            yaml_str = self.generate_domain_schema(domain_name, d["matched_items"], custom_prompt=custom_prompt)
            results[domain_name] = yaml_str
        return results

    def generate_schemas_for_form_mappings(self, mappings: Dict[str, str], custom_prompt: Optional[str] = None) -> Dict[str, str]:
        """
        Generates Yamaa YAML schemas from user-selected { FormOID: TargetDomain } mappings.
        """
        crf_forms = self.discover_crf_forms()
        forms_by_domain: Dict[str, List[Dict[str, Any]]] = {}

        for f in crf_forms:
            f_oid = f["FormOID"]
            if f_oid not in mappings:
                continue
            target_domain = mappings[f_oid].upper()
            if target_domain not in forms_by_domain:
                forms_by_domain[target_domain] = []
            forms_by_domain[target_domain].append(f)

        results = {}
        for domain, form_list in forms_by_domain.items():
            all_items = []
            form_oids = []
            for f in form_list:
                form_oids.append(f["FormOID"])
                all_items.extend(f["Items"])

            yaml_str = self.generate_domain_schema(domain, matched_items=all_items, custom_prompt=custom_prompt, form_oids=form_oids)
            results[domain] = yaml_str

        return results

    def generate_domain_schema(
        self,
        domain: str,
        matched_items: Optional[List[Dict[str, Any]]] = None,
        custom_prompt: Optional[str] = None,
        form_oids: Optional[List[str]] = None
    ) -> str:
        """Synthesizes a complete Yamaa YAML specification for a domain."""
        if self.provider in ("gemini", "anthropic", "openai") and self.api_key:
            try:
                if self.provider == "gemini":
                    llm_schema = self._generate_with_gemini(domain, matched_items, custom_prompt)
                elif self.provider == "anthropic":
                    llm_schema = self._generate_with_anthropic(domain, matched_items, custom_prompt)
                elif self.provider == "openai":
                    llm_schema = self._generate_with_openai(domain, matched_items, custom_prompt)
                else:
                    llm_schema = None

                if llm_schema and "domain:" in llm_schema and ("columns:" in llm_schema or "rows:" in llm_schema):
                    return llm_schema
            except Exception as e:
                logger.warning(f"LLM generation failed ({e}), falling back to built-in CDISC engine")

        matched_items = matched_items or []
        if not form_oids:
            form_oids = list(set([it.get("FormOID") for it in matched_items if it.get("FormOID")]))

        if domain == "DM":
            return self._generate_dm_schema(domain, matched_items, form_oids)
        elif domain in CDISC_SDTM_STANDARDS and CDISC_SDTM_STANDARDS[domain]["class"] == "FINDINGS":
            return self._generate_findings_schema(domain, CDISC_SDTM_STANDARDS[domain], matched_items, form_oids)
        else:
            return self._generate_events_schema(domain, CDISC_SDTM_STANDARDS.get(domain, {}), matched_items, form_oids)

    def _generate_findings_schema(self, domain: str, d_info: Dict[str, Any], matched_items: List[Dict[str, Any]], form_oids: List[str]) -> str:
        seq_col = f"{domain}SEQ"
        testcd_col = f"{domain}TESTCD"
        test_col = f"{domain}TEST"
        orres_col = f"{domain}ORRES"
        stresn_col = f"{domain}STRESN"
        dtc_col = f"{domain}DTC"

        tests_cfg = d_info.get("tests", {})
        rows_list = []

        for t_code, t_meta in tests_cfg.items():
            best_item_oid = None
            for item in matched_items:
                i_oid = str(item.get("ItemOID", "")).lower()
                i_name = str(item.get("ItemName", "")).lower()
                q_text = str(item.get("Question", "")).lower()
                for kw in t_meta["keywords"]:
                    if kw in i_oid or kw in i_name or kw in q_text:
                        best_item_oid = item.get("ItemOID")
                        break
                if best_item_oid:
                    break

            if best_item_oid:
                row_entry = {
                    "id": t_code.lower(),
                    "filter": f"ODM.ItemOID = '{best_item_oid}' AND ODM.Value IS NOT NULL",
                    "derivations": {
                        testcd_col: {"literal": t_code},
                        test_col: {"literal": t_meta["name"]},
                        orres_col: {"source": "ODM.Value"},
                        stresn_col: {
                            "value": {"source": "ODM.Value"},
                            "conversion_failure": None
                        }
                    }
                }
                rows_list.append(row_entry)

        # Fallback to items in form
        if not rows_list and matched_items:
            for idx, item in enumerate(matched_items[:12]):
                item_oid = item.get("ItemOID", "TEST")
                item_name = item.get("ItemName", item_oid.split(".")[-1])
                rows_list.append({
                    "id": f"test_{idx+1}",
                    "filter": f"ODM.ItemOID = '{item_oid}' AND ODM.Value IS NOT NULL",
                    "derivations": {
                        testcd_col: {"literal": item_name.upper()[:8]},
                        test_col: {"literal": item_name},
                        orres_col: {"source": "ODM.Value"},
                        stresn_col: {
                            "value": {"source": "ODM.Value"},
                            "conversion_failure": None
                        }
                    }
                })

        columns = [
            {"name": "STUDYID", "type": "str", "derivation": {"source": "StudyOID"}},
            {"name": "DOMAIN", "type": "str", "derivation": {"literal": domain}},
            {"name": "USUBJID", "type": "str", "derivation": {"source": "SubjectKey"}},
            {
                "name": seq_col,
                "type": "int",
                "derivation": {
                    "row_number": {
                        "group_by": ["STUDYID", "USUBJID"],
                        "order_by": [dtc_col, testcd_col]
                    }
                }
            },
            {"name": testcd_col, "type": "str"},
            {"name": test_col, "type": "str"},
            {"name": orres_col, "type": "str"},
            {"name": stresn_col, "type": "float"}
        ]

        spec_obj = {
            "domain": domain,
            "datasets": {"ODM": "input/odm.csv"},
            "base": "ODM",
            "keys": ["STUDYID", "USUBJID", seq_col],
            "columns": columns,
            "rows": rows_list,
            "verifications": [
                {"unique": {"columns": ["STUDYID", "USUBJID", seq_col]}},
                {"not_missing": {"columns": ["STUDYID", "USUBJID", testcd_col]}}
            ]
        }
        return yaml.dump(spec_obj, sort_keys=False, default_flow_style=False)

    def _generate_dm_schema(self, domain: str, matched_items: List[Dict[str, Any]], form_oids: List[str]) -> str:
        site_oid = None
        age_oid = None
        sex_oid = None
        race_oid = None
        ethnic_oid = None
        country_oid = None
        armcd_oid = None
        arm_oid = None
        rfstdtc_oid = None

        for item in matched_items:
            i_oid = str(item.get("ItemOID", ""))
            i_lower = i_oid.lower()
            q_lower = str(item.get("Question", "")).lower()

            if "site" in i_lower or "site" in q_lower:
                site_oid = i_oid
            elif "age" in i_lower or "age" in q_lower:
                age_oid = i_oid
            elif "sex" in i_lower or "gender" in i_lower or "gender" in q_lower:
                sex_oid = i_oid
            elif "race" in i_lower or "race" in q_lower:
                race_oid = i_oid
            elif "ethnic" in i_lower or "ethnic" in q_lower:
                ethnic_oid = i_oid
            elif "country" in i_lower or "nation" in q_lower:
                country_oid = i_oid
            elif "armcd" in i_lower:
                armcd_oid = i_oid
            elif "arm" in i_lower:
                arm_oid = i_oid
            elif "rfstdtc" in i_lower or "start_date" in i_lower or "first_dose" in i_lower:
                rfstdtc_oid = i_oid

        columns = [
            {"name": "STUDYID", "type": "str", "label": "Study Identifier", "derivation": {"source": "StudyOID"}},
            {"name": "DOMAIN", "type": "str", "label": "Domain Abbreviation", "derivation": {"literal": "DM"}},
            {"name": "USUBJID", "type": "str", "label": "Unique Subject Identifier", "derivation": {"source": "SubjectKey"}},
            {"name": "SUBJID", "type": "str", "label": "Subject Identifier for the Study", "derivation": {"source": "SubjectKey"}}
        ]

        if site_oid:
            columns.append({"name": "SITEID", "type": "str", "derivation": {"source": site_oid}})
        if age_oid:
            columns.append({"name": "AGE", "type": "int", "derivation": {"source": age_oid}})
            columns.append({"name": "AGEU", "type": "str", "derivation": {"literal": "YEARS"}})
        if sex_oid:
            columns.append({
                "name": "SEX",
                "type": "str",
                "derivation": {
                    "mapping": {
                        "source": sex_oid,
                        "dict": {"M": "M", "F": "F", "Male": "M", "Female": "F", "Unknown": "U", "U": "U", "1": "M", "2": "F"},
                        "case_sensitive": False
                    }
                }
            })
        if race_oid:
            columns.append({"name": "RACE", "type": "str", "derivation": {"source": race_oid}})
        if ethnic_oid:
            columns.append({
                "name": "ETHNIC",
                "type": "str",
                "derivation": {
                    "mapping": {
                        "source": ethnic_oid,
                        "dict": {"NOT HISPANIC OR LATINO": "NOT HISPANIC OR LATINO", "HISPANIC OR LATINO": "HISPANIC OR LATINO", "NOT HISPANIC": "NOT HISPANIC OR LATINO", "HISPANIC": "HISPANIC OR LATINO"},
                        "case_sensitive": False
                    }
                }
            })
        if country_oid:
            columns.append({"name": "COUNTRY", "type": "str", "derivation": {"source": country_oid}})
        if armcd_oid:
            columns.append({"name": "ARMCD", "type": "str", "derivation": {"source": armcd_oid}})
        if arm_oid:
            columns.append({"name": "ARM", "type": "str", "derivation": {"source": arm_oid}})
            if armcd_oid:
                columns.append({"name": "ACTARMCD", "type": "str", "derivation": {"source": "ARMCD"}})
            columns.append({"name": "ACTARM", "type": "str", "derivation": {"source": "ARM"}})
        if rfstdtc_oid:
            columns.append({"name": "RFSTDTC", "type": "date", "derivation": {"source": rfstdtc_oid}})

        spec_obj = {
            "domain": "DM",
            "datasets": {"ODM": "input/odm.csv"},
            "formoid": form_oids,
            "base": "ODM",
            "keys": ["STUDYID", "USUBJID"],
            "columns": columns,
            "verifications": [
                {"unique": {"columns": ["STUDYID", "USUBJID"]}},
                {"not_missing": {"columns": ["STUDYID", "USUBJID", "SEX"]}}
            ]
        }
        return yaml.dump(spec_obj, sort_keys=False, default_flow_style=False)

    def _generate_events_schema(self, domain: str, d_info: Dict[str, Any], matched_items: List[Dict[str, Any]], form_oids: List[str]) -> str:
        seq_col = f"{domain}SEQ"
        term_col = f"{domain}TERM"
        stdtc_col = f"{domain}STDTC"
        endtc_col = f"{domain}ENDTC"

        term_oid = None
        stdtc_oid = None
        endtc_oid = None
        sev_oid = None
        ser_oid = None
        rel_oid = None

        for item in matched_items:
            i_oid = str(item.get("ItemOID", ""))
            i_lower = i_oid.lower()
            q_lower = str(item.get("Question", "")).lower()

            if "term" in i_lower or "verbatim" in i_lower or "event" in i_lower or "drug" in i_lower or "status" in i_lower or "condition" in i_lower or "med" in i_lower:
                if not term_oid:
                    term_oid = i_oid
            elif "stdtc" in i_lower or "start" in i_lower or "onset" in i_lower:
                stdtc_oid = i_oid
            elif "endtc" in i_lower or "end" in i_lower or "resolution" in i_lower or "stop" in i_lower:
                endtc_oid = i_oid
            elif "sev" in i_lower or "grade" in i_lower or "severity" in i_lower:
                sev_oid = i_oid
            elif "ser" in i_lower or "serious" in i_lower:
                ser_oid = i_oid
            elif "rel" in i_lower or "causality" in i_lower:
                rel_oid = i_oid

        columns = [
            {"name": "STUDYID", "type": "str", "derivation": {"source": "StudyOID"}},
            {"name": "DOMAIN", "type": "str", "derivation": {"literal": domain}},
            {"name": "USUBJID", "type": "str", "derivation": {"source": "SubjectKey"}},
            {
                "name": seq_col,
                "type": "int",
                "derivation": {
                    "row_number": {
                        "group_by": ["STUDYID", "USUBJID"],
                        "order_by": [stdtc_col if stdtc_oid else term_col, term_col]
                    }
                }
            }
        ]

        if term_oid:
            columns.append({"name": term_col, "type": "str", "derivation": {"source": term_oid}})
        if sev_oid and domain == "AE":
            columns.append({
                "name": "AESEV",
                "type": "str",
                "derivation": {
                    "mapping": {
                        "source": sev_oid,
                        "dict": {"Mild": "MILD", "Moderate": "MODERATE", "Severe": "SEVERE", "1": "MILD", "2": "MODERATE", "3": "SEVERE"},
                        "case_sensitive": False
                    }
                }
            })
        if ser_oid and domain == "AE":
            columns.append({
                "name": "AESER",
                "type": "str",
                "derivation": {
                    "mapping": {
                        "source": ser_oid,
                        "dict": {"Yes": "Y", "No": "N", "1": "Y", "0": "N"},
                        "case_sensitive": False
                    }
                }
            })
        if rel_oid and domain == "AE":
            columns.append({"name": "AEREL", "type": "str", "derivation": {"source": rel_oid}})
        if stdtc_oid:
            columns.append({"name": stdtc_col, "type": "date", "derivation": {"source": stdtc_oid}})
        if endtc_oid:
            columns.append({"name": endtc_col, "type": "date", "derivation": {"source": endtc_oid}})

        spec_obj = {
            "domain": domain,
            "datasets": {"ODM": "input/odm.csv"},
            "formoid": form_oids,
            "base": "ODM",
            "keys": ["STUDYID", "USUBJID", seq_col],
            "columns": columns,
            "verifications": [
                {"unique": {"columns": ["STUDYID", "USUBJID", seq_col]}},
                {"not_missing": {"columns": ["STUDYID", "USUBJID", term_col]}}
            ]
        }
        return yaml.dump(spec_obj, sort_keys=False, default_flow_style=False)

    def _build_prompt_context(self, domain: str, matched_items: Optional[List[Dict[str, Any]]], custom_prompt: Optional[str] = None) -> (str, str):
        crf_summary = []
        items_to_use = matched_items or (self.metadata_df.to_dicts()[:40] if self.metadata_df is not None else [])
        
        for it in items_to_use[:35]:
            crf_summary.append({
                "FormOID": it.get("FormOID"),
                "ItemOID": it.get("ItemOID"),
                "ItemName": it.get("ItemName"),
                "Question": it.get("Question"),
                "DataType": it.get("DataType"),
                "SampleValues": it.get("SampleValues")
            })

        system_instruction = (
            "You are a world-class CDISC SDTM Principal Statistical Programmer and YAML Schema Architect.\n"
            "Your task is to generate a valid CDISC SDTM domain specification in the 'yamaa' YAML schema format (v1.0).\n"
            "The output must strictly be raw valid YAML (no markdown fences, no explanatory text).\n"
        )
        user_content = f"Generate standard CDISC SDTM '{domain}' domain Yamaa YAML schema from this CRF metadata:\n{json.dumps(crf_summary, indent=2)}\n"
        if custom_prompt:
            user_content += f"\nCustom instructions: {custom_prompt}\n"

        return system_instruction, user_content

    def _clean_llm_yaml(self, text: str) -> str:
        text = re.sub(r"^```(yaml)?\n", "", text.strip())
        text = re.sub(r"\n```$", "", text.strip())
        return text

    def _generate_with_gemini(self, domain: str, matched_items: Optional[List[Dict[str, Any]]], custom_prompt: Optional[str] = None) -> Optional[str]:
        model = self.model_name or "gemini-2.5-flash"
        system_instruction, user_content = self._build_prompt_context(domain, matched_items, custom_prompt)

        endpoint = f"https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={self.api_key}"
        payload = {
            "contents": [{"role": "user", "parts": [{"text": system_instruction + "\n\n" + user_content}]}],
            "generationConfig": {"temperature": 0.1, "maxOutputTokens": 4096}
        }

        resp = httpx.post(endpoint, json=payload, timeout=30.0)
        if resp.status_code == 200:
            data = resp.json()
            cand = data.get("candidates", [])[0]
            text = cand.get("content", {}).get("parts", [])[0].get("text", "")
            return self._clean_llm_yaml(text)
        return None

    def _generate_with_anthropic(self, domain: str, matched_items: Optional[List[Dict[str, Any]]], custom_prompt: Optional[str] = None) -> Optional[str]:
        model = self.model_name or "claude-3-7-sonnet-20250219"
        system_instruction, user_content = self._build_prompt_context(domain, matched_items, custom_prompt)

        headers = {
            "x-api-key": self.api_key,
            "anthropic-version": "2023-06-01",
            "content-type": "application/json"
        }
        payload = {
            "model": model,
            "system": system_instruction,
            "messages": [{"role": "user", "content": user_content}],
            "max_tokens": 4096,
            "temperature": 0.1
        }

        resp = httpx.post("https://api.anthropic.com/v1/messages", json=payload, headers=headers, timeout=30.0)
        if resp.status_code == 200:
            data = resp.json()
            text = data.get("content", [])[0].get("text", "")
            return self._clean_llm_yaml(text)
        return None

    def _generate_with_openai(self, domain: str, matched_items: Optional[List[Dict[str, Any]]], custom_prompt: Optional[str] = None) -> Optional[str]:
        model = self.model_name or "gpt-4o"
        system_instruction, user_content = self._build_prompt_context(domain, matched_items, custom_prompt)

        headers = {
            "Authorization": f"Bearer {self.api_key}",
            "Content-Type": "application/json"
        }
        payload = {
            "model": model,
            "messages": [
                {"role": "system", "content": system_instruction},
                {"role": "user", "content": user_content}
            ],
            "temperature": 0.1
        }

        resp = httpx.post("https://api.openai.com/v1/chat/completions", json=payload, headers=headers, timeout=30.0)
        if resp.status_code == 200:
            data = resp.json()
            text = data.get("choices", [])[0].get("message", {}).get("content", "")
            return self._clean_llm_yaml(text)
        return None
