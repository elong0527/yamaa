# CDISC Builder v2 (Yamaa Schema Standard)

[![PyPI version](https://img.shields.io/pypi/v/cdiscbuilderv2.svg)](https://pypi.org/project/cdiscbuilderv2/)
[![Python 3.9+](https://img.shields.io/badge/python-3.9+-blue.svg)](https://www.python.org/downloads/)
[![Polars](https://img.shields.io/badge/Polars-0.20+-CD792C.svg)](https://pola.rs)
[![CDISC SDTM](https://img.shields.io/badge/CDISC-SDTM%20v1.7%2F3.3-orange.svg)](https://www.cdisc.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**CDISC Builder v2** is a fast, lightweight clinical trial data transformation engine and library. It ingests raw Electronic Data Capture (EDC) ODM XML (OpenClinica, Medidata Rave, Castor) and creates submission-ready CDISC SDTM datasets using declarative **Yamaa YAML specifications** and an ultra-fast **Polars transformation engine**.

---

## Installation

Install via `pip`:

```bash
pip install cdiscbuilderv2
```

Or using `uv`:

```bash
uv pip install cdiscbuilderv2
```

---

## Key Features

1. **EDC ODM XML Ingestion**: High-performance parsing of `MetaDataVersion` (Forms, Items, CodeLists) and `ClinicalData` directly into long/wide format Polars DataFrames.
2. **Declarative Yamaa YAML Engine**: Direct mapping, column derivations, SQL row-level slicing, sequence numbering, and cross-domain reference resolution.
3. **Medical Coding Engine**: Built-in exact, synonym, and fuzzy medical coding resolution for MedDRA and WHO Drug Global.
4. **Study Design & Trial Design Domains**: Built-in catalog of 16 clinical study design archetypes (Oncology, Crossover, Rare Disease, Adaptive, Platform Trials) and automated Trial Design Model (TS, TA, TE, TI, TV) dataset builder.
5. **Quality Verifications & Rule Engine**: Automated conformance checks for uniqueness and missingness constraints.
6. **Multi-Format Export**: One-click generation of **CSV**, **Apache Parquet**, and **SAS Transport (.XPT)** datasets.
7. **Clinical Document PDF Generation**: Generate publication-ready, ICH GCP E6 (R2) and ICH E9 compliant PDF Protocols and Statistical Analysis Plans (SAP).

---

## Quick Start (Python API)

### Ingest ODM XML and Build SDTM Pipeline

```python
from cdiscbuilderv2 import SDTMPipeline

# Initialize the pipeline
pipeline = SDTMPipeline(
    xml_path="path/to/study_odm.xml",
    specs_dir="path/to/yaml_specs_dir",
    output_dir="./sdtm_output"
)

# Ingest raw ODM XML
pipeline.ingest_odm()

# Run the transformation engine and export datasets
results = pipeline.run(export_formats=["csv", "parquet", "xpt"])

for log in pipeline.execution_logs:
    print(f"Domain {log['domain']} -> {log['status']} ({log.get('rows', 0)} rows)")
```

### Direct Domain Engine

```python
import yaml
import polars as pl
from cdiscbuilderv2 import CDISCEngine

# Sample raw source data
raw_df = pl.DataFrame({
    "STUDYID": ["STUDY01", "STUDY01"],
    "SUBJID": ["001", "002"],
    "SEX": ["M", "F"],
    "AGE": [45, 52]
})

# Yamaa Domain Specification
spec_yaml = """
domain: DM
datasets:
  RAW: memory
base: RAW
keys: [STUDYID, USUBJID]
columns:
  - name: STUDYID
    type: str
    derivation: { source: STUDYID }
  - name: DOMAIN
    type: str
    derivation: { literal: DM }
  - name: USUBJID
    type: str
    derivation: { expression: "STUDYID + '-' + SUBJID" }
  - name: AGE
    type: int
    derivation: { source: AGE }
  - name: SEX
    type: str
    derivation: { source: SEX }
"""

engine = CDISCEngine(spec=yaml.safe_load(spec_yaml), datasets={"RAW": raw_df})
sdtm_dm = engine.build()
print(sdtm_dm)
```

---

## CLI Usage

`cdiscbuilder` provides a headless command-line interface:

### 1. Headless Batch SDTM Generation
```bash
cdiscbuilder build \
  --xml /path/to/odm.xml \
  --specs /path/to/yaml_specs_dir \
  --output ./sdtm_output \
  --formats csv,parquet,xpt
```

### 2. Parse Raw ODM XML to Long-Format Table
```bash
cdiscbuilder parse-odm --xml /path/to/odm.xml --output long_data.parquet
```

### 3. List & Export Built-in Yamaa Domain Templates
```bash
# List available domain templates
cdiscbuilder schemas

# Print specific template
cdiscbuilder schemas --template DM

# Export all built-in templates to a directory
cdiscbuilder schemas --export-dir ./my_study_specs
```

---

## Pre-Packaged Schema Standards & Templates

All standard Yamaa definitions and domain templates are bundled inside the package:
* `schemas/standards/`: Yamaa Schema Meta-Specifications (`schema.yaml`, `schema_derivation.yaml`, `schema_expression_*.yaml`, `schema_verification.yaml`).
* `schemas/templates/`: CDISC SDTM domain templates (`DM`, `VS`, `LB`, `AE`, `EX`, `DS`, `MH`, `RS`, `RP`, `CM`, `PE`, `QS`, `SUPPDM`, `SUPPEX`, `SUPPMH`).
* `schemas/examples/cath/`: Complete benchmark study reference specifications from the CATH study.

---

## Testing

Run tests using pytest:

```bash
pytest
```

---

## License

This project is licensed under the MIT License - see the LICENSE file for details.
