"""
CDISC SDTM Trial Design Model (TDM) Domain Generator.
Synthesizes TS (Trial Summary), TA (Trial Arms), TE (Trial Elements),
TV (Trial Visits), and TI (Inclusion/Exclusion) schemas & Polars DataFrames.
"""

from typing import Any, Dict, List, Optional
import polars as pl
import yaml
from .study_designs import get_design_by_id


class TrialDesignBuilder:
    """
    Constructs compliant CDISC SDTM Trial Design Model (TDM) specifications
    and Polars DataFrames from a configured clinical trial design.
    """

    def __init__(self, design_id: str, study_id: str = "ST-001", custom_config: Optional[Dict[str, Any]] = None):
        self.design_id = design_id
        self.study_id = study_id
        self.design_meta = get_design_by_id(design_id) or {}
        self.config = custom_config or {}

    def build_ts_schema(self) -> str:
        """Generates Yamaa YAML specification for TS (Trial Summary)."""
        return f"""domain: TS
description: CDISC SDTM Trial Summary Domain
datasets:
  ODM: input/odm.csv
base: ODM
keys: [STUDYID, DOMAIN, TSPARMCD, TSSEQ]

columns:
  - name: STUDYID
    type: str
    derivation: {{ literal: "{self.study_id}" }}
  - name: DOMAIN
    type: str
    derivation: {{ literal: "TS" }}
  - name: TSSEQ
    type: int
    derivation: {{ row_number: true }}
  - name: TSPARMCD
    type: str
    derivation: {{ source: TSPARMCD }}
  - name: TSPARM
    type: str
    derivation: {{ source: TSPARM }}
  - name: TSVAL
    type: str
    derivation: {{ source: TSVAL }}
"""

    def build_ta_schema(self) -> str:
        """Generates Yamaa YAML specification for TA (Trial Arms)."""
        return f"""domain: TA
description: CDISC SDTM Trial Arms Domain
datasets:
  ODM: input/odm.csv
base: ODM
keys: [STUDYID, DOMAIN, ARMCD, TAETORD]

columns:
  - name: STUDYID
    type: str
    derivation: {{ literal: "{self.study_id}" }}
  - name: DOMAIN
    type: str
    derivation: {{ literal: "TA" }}
  - name: ARMCD
    type: str
    derivation: {{ source: ARMCD }}
  - name: ARM
    type: str
    derivation: {{ source: ARM }}
  - name: TAETORD
    type: int
    derivation: {{ source: TAETORD }}
  - name: ETCD
    type: str
    derivation: {{ source: ETCD }}
  - name: ELEMENT
    type: str
    derivation: {{ source: ELEMENT }}
  - name: EPOCH
    type: str
    derivation: {{ source: EPOCH }}
"""

    def build_te_schema(self) -> str:
        """Generates Yamaa YAML specification for TE (Trial Elements)."""
        return f"""domain: TE
description: CDISC SDTM Trial Elements Domain
datasets:
  ODM: input/odm.csv
base: ODM
keys: [STUDYID, DOMAIN, ETCD]

columns:
  - name: STUDYID
    type: str
    derivation: {{ literal: "{self.study_id}" }}
  - name: DOMAIN
    type: str
    derivation: {{ literal: "TE" }}
  - name: ETCD
    type: str
    derivation: {{ source: ETCD }}
  - name: ELEMENT
    type: str
    derivation: {{ source: ELEMENT }}
  - name: TEDUR
    type: str
    derivation: {{ source: TEDUR }}
"""

    def build_tv_schema(self) -> str:
        """Generates Yamaa YAML specification for TV (Trial Visits)."""
        return f"""domain: TV
description: CDISC SDTM Trial Visits Domain
datasets:
  ODM: input/odm.csv
base: ODM
keys: [STUDYID, DOMAIN, VISITNUM]

columns:
  - name: STUDYID
    type: str
    derivation: {{ literal: "{self.study_id}" }}
  - name: DOMAIN
    type: str
    derivation: {{ literal: "TV" }}
  - name: VISITNUM
    type: float
    derivation: {{ source: VISITNUM }}
  - name: VISIT
    type: str
    derivation: {{ source: VISIT }}
  - name: VISITDY
    type: int
    derivation: {{ source: VISITDY }}
"""

    def build_ts_dataframe(self) -> pl.DataFrame:
        """Builds standardized TS (Trial Summary) dataset."""
        ts_params = self.design_meta.get("cdisc_ts_params", {})
        
        parameters = [
            ("TITLE", "Trial Title", self.design_meta.get("title", "Clinical Study")),
            ("PROTOCOL", "Protocol Identification", self.study_id),
            ("PHASE", "Trial Phase Class", self.design_meta.get("phase", "Phase 3")),
            ("DESIGN", "Trial Design Type", self.design_meta.get("design_type", "Parallel")),
            ("BLIND", "Trial Blinding Schema", self.design_meta.get("blinding", "Double-Blind")),
            ("RANDOM", "Randomized Trial Indicator", ts_params.get("RANDOM", "Y")),
            ("PIND", "Trial Indication", ts_params.get("PIND", "Target Clinical Indication")),
            ("OBJPRIM", "Primary Objective", ts_params.get("OBJPRIM", "Efficacy and Safety Evaluation")),
            ("TRT", "Investigational Intervention", self.design_meta.get("arms", [{}])[0].get("name", "Investigational Agent")),
            ("AGEMIN", "Planned Minimum Age of Subjects", "18"),
            ("AGEMAX", "Planned Maximum Age of Subjects", "85"),
            ("SEXPOP", "Sex of Study Population", "BOTH"),
            ("SSTDTC", "Study Start Date", "2026-01-15")
        ]

        if "NITMETH" in ts_params:
            parameters.append(("NITMETH", "Non-Inferiority Statistical Method", ts_params["NITMETH"]))

        rows = []
        for idx, (p_cd, p_lbl, p_val) in enumerate(parameters, start=1):
            rows.append({
                "STUDYID": self.study_id,
                "DOMAIN": "TS",
                "TSSEQ": idx,
                "TSGRPID": "TRIAL",
                "TSPARMCD": p_cd,
                "TSPARM": p_lbl,
                "TSVAL": str(p_val),
                "TSVALNF": None
            })

        return pl.DataFrame(rows)

    def build_ta_dataframe(self) -> pl.DataFrame:
        """Builds standardized TA (Trial Arms) dataset."""
        arms = self.design_meta.get("arms", [{"code": "ARM_A", "name": "Active Arm", "type": "Active"}])
        epochs = self.design_meta.get("epochs", ["SCREENING", "TREATMENT", "FOLLOWUP"])
        
        rows = []
        for arm in arms:
            arm_cd = arm["code"]
            arm_name = arm["name"]
            for ord_idx, ep in enumerate(epochs, start=1):
                et_cd = f"{arm_cd}_{ep[:4]}" if ep == "TREATMENT" else ep[:4]
                rows.append({
                    "STUDYID": self.study_id,
                    "DOMAIN": "TA",
                    "ARMCD": arm_cd,
                    "ARM": arm_name,
                    "TAETORD": ord_idx,
                    "ETCD": et_cd,
                    "ELEMENT": f"{ep.capitalize()} Element",
                    "TABRANCH": None,
                    "TATRANS": "STANDARD",
                    "EPOCH": ep
                })
        return pl.DataFrame(rows)

    def build_te_dataframe(self) -> pl.DataFrame:
        """Builds standardized TE (Trial Elements) dataset."""
        epochs = self.design_meta.get("epochs", ["SCREENING", "TREATMENT", "FOLLOWUP"])
        rows = []
        durations = {
            "SCREENING": "P28D",
            "TREATMENT": "P180D",
            "PROCEDURE": "P1D",
            "PERIOD_1": "P14D",
            "WASHOUT_7D": "P7D",
            "PERIOD_2": "P14D",
            "RUN_IN": "P14D",
            "SAFETY_FOLLOWUP": "P30D",
            "SURVIVAL_FOLLOWUP": "P720D",
            "FOLLOWUP": "P30D"
        }
        for ep in epochs:
            et_cd = ep[:6]
            rows.append({
                "STUDYID": self.study_id,
                "DOMAIN": "TE",
                "ETCD": et_cd,
                "ELEMENT": f"{ep.replace('_', ' ').title()} Element",
                "TESTRL": "Signed Informed Consent" if "SCREEN" in ep else "Randomization Complete",
                "TEENRL": "Element Completion",
                "TEDUR": durations.get(ep, "P30D")
            })
        return pl.DataFrame(rows)

    def build_tv_dataframe(self) -> pl.DataFrame:
        """Builds standardized TV (Trial Visits) dataset."""
        visits = self.design_meta.get("visits", ["Screening", "Day 1 Baseline", "Week 4", "Week 12", "End of Study"])
        rows = []
        visit_days = {
            "Screening": -14,
            "Baseline": 1,
            "Day 1": 1,
            "Week 4": 28,
            "Week 8": 56,
            "Week 12": 84,
            "Month 6": 180,
            "Month 12": 365,
            "End of Treatment": 180,
            "Exit": 210
        }
        for v_idx, v_name in enumerate(visits, start=1):
            day_val = 1
            for k, d in visit_days.items():
                if k.lower() in v_name.lower():
                    day_val = d
                    break
            rows.append({
                "STUDYID": self.study_id,
                "DOMAIN": "TV",
                "VISITNUM": float(v_idx),
                "VISIT": v_name,
                "VISITDY": day_val,
                "TVSTRL": f"Planned {v_name} visit window start",
                "TVENRL": f"Planned {v_name} visit window end"
            })
        return pl.DataFrame(rows)

    def generate_all_tdm_schemas(self) -> Dict[str, str]:
        """Returns map of domain name to Yamaa YAML schema string."""
        return {
            "TS": self.build_ts_schema(),
            "TA": self.build_ta_schema(),
            "TE": self.build_te_schema(),
            "TV": self.build_tv_schema()
        }

    def generate_all_tdm_dataframes(self) -> Dict[str, pl.DataFrame]:
        """Returns map of domain name to Polars DataFrame."""
        return {
            "TS": self.build_ts_dataframe(),
            "TA": self.build_ta_dataframe(),
            "TE": self.build_te_dataframe(),
            "TV": self.build_tv_dataframe()
        }
