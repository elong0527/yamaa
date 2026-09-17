import pytest
from cdiscbuilderv2.validator import YamaaSchemaValidator


def test_validator_valid_schema():
    valid_yaml = """
domain: DM
input:
  ODM: input/odm.csv
base: ODM
keys: [STUDYID, USUBJID]

columns:
  - name: STUDYID
    type: str
    derivation: { source: StudyOID }
  - name: DOMAIN
    type: str
    derivation: { literal: DM }
  - name: USUBJID
    type: str
    derivation: { source: SubjectKey }
  - name: SEX
    type: str
    derivation:
      mapping:
        source: IT.DM.SEX
        dict: { M: M, F: F }

verifications:
  - unique:
      columns: [STUDYID, USUBJID]
"""
    validator = YamaaSchemaValidator(valid_yaml)
    is_valid, errors, warnings = validator.validate()
    assert is_valid is True
    assert len(errors) == 0


def test_validator_filtered_source_post_517():
    spec_yaml = """
domain: DM
input:
  ODM: odm.csv
base: ODM
keys: [STUDYID, USUBJID]
columns:
  - name: STUDYID
    type: str
    derivation: { source: StudyOID }
  - name: USUBJID
    type: str
    derivation:
      source:
        variable: ODM.SubjectKey
        filter: "SubjectKey IS NOT NULL"
  - name: SEX
    type: str
    derivation:
      mapping:
        source:
          variable: ODM.Value
          filter: "ODM.ItemOID = 'IT.DM.SEX'"
        dict: { Male: M, Female: F }
  - name: ARM
    type: str
    derivation:
      coalesce:
        sources:
          - variable: ODM.Value
            filter: "ODM.ItemOID = 'IT.DM.ARM'"
          - variable: ODM.DefaultArm
"""
    validator = YamaaSchemaValidator(spec_yaml)
    is_valid, errors, warnings = validator.validate()
    assert is_valid is True
    assert len(errors) == 0


def test_validator_r003_38_prohibited_construct():
    # R003-38: A filter on a source with no records to select among is prohibited
    invalid_yaml = """
domain: DM
input:
  ODM: odm.csv
base: ODM
keys: [STUDYID, USUBJID]
columns:
  - name: K
    type: str
    derivation: { source: ODM.SubjectKey }
  - name: A
    type: str
    derivation:
      source:
        variable: K
        filter: "K = '01'"
"""
    validator = YamaaSchemaValidator(invalid_yaml)
    is_valid, errors, warnings = validator.validate()
    assert is_valid is False
    assert any("R003-38" in e for e in errors)


def test_validator_missing_required_fields():
    invalid_yaml = """
base: ODM
columns:
  - name: STUDYID
    type: str
"""
    validator = YamaaSchemaValidator(invalid_yaml)
    is_valid, errors, warnings = validator.validate()
    assert is_valid is False
    assert any("domain" in e for e in errors)
    assert any("keys" in e for e in errors)


def test_validator_invalid_sql_filter():
    invalid_sql_yaml = """
domain: LB
datasets: {ODM: input/odm.csv}
base: ODM
keys: [STUDYID, USUBJID, LBSEQ]
columns:
  - name: STUDYID
    type: str
  - name: USUBJID
    type: str
  - name: LBSEQ
    type: int
rows:
  - id: test1
    filter: "INVALID SQL SYNTAX HERE @@@ !!!"
    derivations:
      LBTEST: { literal: "Test" }
"""
    validator = YamaaSchemaValidator(invalid_sql_yaml)
    is_valid, errors, warnings = validator.validate()
    assert is_valid is False
    assert any("invalid SQL filter" in e for e in errors)

def test_auto_fix_yamaa_schema():
    from cdiscbuilderv2.validator import auto_fix_yamaa_schema
    
    broken_yaml = """
columns:
  - name: AGE
    type: str
"""
    fixed_yaml, fixes = auto_fix_yamaa_schema(broken_yaml, "DM")
    assert len(fixes) > 0
    assert "input:" in fixed_yaml
    v = YamaaSchemaValidator(fixed_yaml, "DM")
    is_valid, errors, _ = v.validate()
    assert is_valid is True
    assert len(errors) == 0

    # Test migration of legacy datasets
    legacy_yaml = """
domain: DM
datasets:
  ODM: input/odm.csv
keys: [STUDYID, USUBJID]
columns:
  - name: STUDYID
    type: str
"""
    migrated_yaml, fixes2 = auto_fix_yamaa_schema(legacy_yaml, "DM")
    assert any("Migrated legacy" in f for f in fixes2)
    assert "input:" in migrated_yaml
    assert "datasets:" not in migrated_yaml
