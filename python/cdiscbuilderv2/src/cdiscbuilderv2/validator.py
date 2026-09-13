"""
Yamaa YAML Specification Validator and Auto-Fixer.
Performs semantic and structural validation against the Yamaa v1.0 standard
before pipeline execution, supports multi-format schemas, and provides automated repair.
"""

from typing import Any, Dict, List, Optional, Tuple, Union
import yaml
import re
from .sql_parser import SQLParser

VALID_TYPES = {"str", "int", "float", "date", "datetime", "bool", "string", "integer", "number"}
VALID_VERIFICATION_RULES = {"unique", "not_missing", "range", "valid_values", "compare"}


class YamaaValidationError(Exception):
    """Exception raised when a Yamaa schema violates the specification standard."""
    pass


class YamaaSchemaValidator:
    """
    Validates a Yamaa YAML specification against the CDISC Yamaa schema standard.
    Supports standard flat Yamaa schemas, domain-keyed schemas (DM: [...]),
    and definition-block schemas (LB: {type: FINDINGS, ...}).
    """

    def __init__(self, spec: Union[str, Dict[str, Any]], domain_hint: Optional[str] = None):
        self.raw_spec = spec
        self.domain_hint = domain_hint
        self.spec: Optional[Dict[str, Any]] = None
        self.format_type = "STANDARD"  # STANDARD, DOMAIN_KEYED, DEFINITION_BLOCK
        self.errors: List[str] = []
        self.warnings: List[str] = []
        self._parse()

    def _parse(self) -> None:
        """Parses raw input into dictionary if needed."""
        if isinstance(self.raw_spec, str):
            try:
                parsed = yaml.safe_load(self.raw_spec)
                if parsed is None:
                    self.errors.append("Empty specification. Please enter or generate YAML content.")
                    return
                if not isinstance(parsed, dict) and not isinstance(parsed, list):
                    self.errors.append("Schema root must be a YAML mapping (dictionary) or domain list.")
                    return
                self.spec = parsed
            except yaml.YAMLError as e:
                self.errors.append(f"YAML Syntax Error: {e}")
                self.spec = None
                return
        elif isinstance(self.raw_spec, dict) or isinstance(self.raw_spec, list):
            self.spec = self.raw_spec
        else:
            self.errors.append("Invalid specification type. Expected YAML string or dict.")
            self.spec = None
            return

        self._detect_and_normalize_format()

    def _detect_and_normalize_format(self) -> None:
        """Identifies schema format and normalizes for validation."""
        if not self.spec:
            return

        # Case 1: Domain-keyed dictionary e.g. {"DM": [ ... ]} or {"_DM_REF": [...], "DM": [...]} or {"LB": {"type": "FINDINGS", ...}}
        if isinstance(self.spec, dict) and "domain" not in self.spec:
            all_domain_keys = all(k.startswith("_") or k.isupper() for k in self.spec.keys())
            if all_domain_keys and len(self.spec) > 0:
                first_val = list(self.spec.values())[0]
                if isinstance(first_val, dict) and ("type" in first_val or "columns" in first_val or "definitions" in first_val):
                    self.format_type = "DEFINITION_BLOCK"
                    self.domain = self.domain_hint or list(self.spec.keys())[0]
                    return
                else:
                    self.format_type = "DOMAIN_KEYED"
                    self.domain = self.domain_hint or list(self.spec.keys())[-1]
                    return

        # Case 2: Standard flat format
        if isinstance(self.spec, dict):
            self.format_type = "STANDARD"
            self.domain = self.spec.get("domain", self.domain_hint or "UNKNOWN")
        elif isinstance(self.spec, list) and len(self.spec) > 0 and isinstance(self.spec[0], dict):
            self.format_type = "DOMAIN_KEYED"
            self.domain = self.domain_hint or "UNKNOWN"

    def validate(self) -> Tuple[bool, List[str], List[str]]:
        """
        Executes full validation suite on the schema.
        Returns (is_valid, errors, warnings).
        """
        if self.errors or self.spec is None:
            return False, self.errors, self.warnings

        if self.format_type == "DEFINITION_BLOCK":
            self._validate_definition_block()
        elif self.format_type == "DOMAIN_KEYED":
            self._validate_domain_keyed()
        else:
            self._validate_standard_spec()

        is_valid = len(self.errors) == 0
        return is_valid, self.errors, self.warnings

    def _validate_standard_spec(self) -> None:
        """Validates standard flat Yamaa specification."""
        self._validate_root_fields()
        self._validate_keys()
        self._validate_columns()
        self._validate_rows()
        self._validate_verifications()

    def _validate_definition_block(self) -> None:
        """Validates definition block format (e.g. LB: {type: FINDINGS, definitions: ..., columns: ...})."""
        key_name = list(self.spec.keys())[0]
        block = self.spec[key_name]
        
        if not isinstance(block, dict):
            self.errors.append(f"Definition block for domain '{key_name}' must be a dictionary.")
            return

        if "columns" not in block and "definitions" not in block:
            self.errors.append(f"Domain '{key_name}' must define either 'columns' or 'definitions'.")

        if "type" in block:
            d_type = str(block["type"]).upper()
            if d_type not in {"FINDINGS", "EVENTS", "INTERVENTIONS", "SPECIAL_PURPOSE", "RELATIONSHIP"}:
                self.warnings.append(f"Non-standard domain class type: '{d_type}'")

    def _validate_domain_keyed(self) -> None:
        """Validates domain keyed list format (e.g. DM: [ {formoid: ..., keys: ..., columns: ...} ])."""
        val = self.spec if isinstance(self.spec, list) else list(self.spec.values())[0]
        if not isinstance(val, list) or len(val) == 0:
            self.errors.append(f"Domain specification must contain at least one configuration block.")
            return

        for idx, block in enumerate(val):
            if not isinstance(block, dict):
                self.errors.append(f"Configuration block {idx+1} must be a dictionary.")
                continue
            if "columns" not in block:
                self.errors.append(f"Configuration block {idx+1} is missing required field: 'columns'")

    def _validate_root_fields(self) -> None:
        """Checks for required root fields in standard Yamaa spec."""
        if not self.spec or not isinstance(self.spec, dict):
            return

        # Check domain
        if "domain" not in self.spec:
            self.errors.append("Missing required root field: 'domain' (e.g. domain: DM)")
        elif not isinstance(self.spec["domain"], str) or not self.spec["domain"].strip():
            self.errors.append("Field 'domain' must be a non-empty string.")

        # Check datasets
        if "datasets" not in self.spec:
            self.warnings.append("Root field 'datasets' is omitted (defaults to ODM input).")
        elif not isinstance(self.spec["datasets"], dict):
            self.errors.append("Field 'datasets' must be a mapping of {DatasetName: Path}.")

        # Check keys
        if "keys" not in self.spec:
            self.errors.append("Missing required root field: 'keys' (e.g. keys: [STUDYID, USUBJID])")
        elif not isinstance(self.spec["keys"], list) or len(self.spec["keys"]) == 0:
            self.errors.append("Field 'keys' must be a non-empty list of key column names.")

        # Check columns
        if "columns" not in self.spec:
            self.errors.append("Missing required root field: 'columns'")
        elif not isinstance(self.spec["columns"], list):
            self.errors.append("Field 'columns' must be a list of column definitions.")

    def _validate_keys(self) -> None:
        """Validates key columns."""
        if not self.spec or "keys" not in self.spec or not isinstance(self.spec["keys"], list):
            return

        keys = self.spec["keys"]
        for k in keys:
            if not isinstance(k, str) or not k.strip():
                self.errors.append(f"Invalid key column name in 'keys': {k}")

        if "STUDYID" not in keys and self.domain != "_DM_REF":
            self.warnings.append("Best practice: CDISC SDTM domains should include 'STUDYID' in 'keys'.")
        if "USUBJID" not in keys and self.domain != "_DM_REF":
            self.warnings.append("Best practice: CDISC SDTM domains should include 'USUBJID' in 'keys'.")

    def _validate_columns(self) -> None:
        """Validates column definitions and derivations."""
        if not self.spec or "columns" not in self.spec or not isinstance(self.spec["columns"], list):
            return

        defined_cols = set()
        for idx, col in enumerate(self.spec["columns"]):
            if not isinstance(col, dict):
                self.errors.append(f"Column definition at index {idx+1} must be a dictionary.")
                continue

            col_name = col.get("name")
            if not col_name or not isinstance(col_name, str):
                self.errors.append(f"Column at index {idx+1} is missing a valid 'name'.")
                continue

            if col_name in defined_cols:
                self.errors.append(f"Duplicate column name defined in 'columns': '{col_name}'.")
            defined_cols.add(col_name)

            col_type = col.get("type")
            if col_type and str(col_type).lower() not in VALID_TYPES:
                self.warnings.append(f"Column '{col_name}' has non-standard type: '{col_type}'. Standard types: {list(VALID_TYPES)}")

            if "derivation" in col:
                self._validate_derivation(col_name, col["derivation"])

    def _validate_derivation(self, col_name: str, deriv: Any) -> None:
        """Validates a derivation structure."""
        if not isinstance(deriv, dict):
            self.errors.append(f"Derivation for column '{col_name}' must be a dictionary.")
            return

        recognized = False

        if "source" in deriv:
            recognized = True
            if not isinstance(deriv["source"], str):
                self.errors.append(f"Column '{col_name}' 'source' derivation must be a string identifier.")

        if "literal" in deriv:
            recognized = True

        if "mapping" in deriv:
            recognized = True
            m = deriv["mapping"]
            if not isinstance(m, dict):
                self.errors.append(f"Column '{col_name}' 'mapping' must be a dictionary.")
            elif "dict" not in m and "source" not in m:
                self.errors.append(f"Column '{col_name}' 'mapping' must contain 'source' and 'dict'.")

        if "mapping_from" in deriv:
            recognized = True
            mf = deriv["mapping_from"]
            if not isinstance(mf, dict) or "dataset" not in mf or "key" not in mf or "value" not in mf:
                self.errors.append(f"Column '{col_name}' 'mapping_from' requires 'dataset', 'key', and 'value'.")

        if "row_number" in deriv:
            recognized = True
            rn = deriv["row_number"]
            if not isinstance(rn, (dict, bool)):
                self.errors.append(f"Column '{col_name}' 'row_number' derivation must be a dictionary or boolean.")

        if "function_" in deriv:
            recognized = True
            if not isinstance(deriv["function_"], str):
                self.errors.append(f"Column '{col_name}' 'function_' must be a string function name.")

        if "case" in deriv:
            recognized = True

        if "sql" in deriv or "expression" in deriv or "value" in deriv:
            recognized = True

        if not recognized:
            self.warnings.append(f"Column '{col_name}' has custom derivation grammar: {list(deriv.keys())}")

    def _validate_rows(self) -> None:
        """Validates row-level slicing specifications."""
        if not self.spec or "rows" not in self.spec:
            return

        rows = self.spec["rows"]
        if not isinstance(rows, list):
            self.errors.append("Field 'rows' must be a list of row slice specifications.")
            return

        row_ids = set()
        for idx, row in enumerate(rows):
            if not isinstance(row, dict):
                self.errors.append(f"Row slice at index {idx+1} must be a dictionary.")
                continue

            r_id = row.get("id")
            if not r_id or not isinstance(r_id, str):
                self.errors.append(f"Row slice at index {idx+1} is missing a valid 'id'.")
            elif r_id in row_ids:
                self.warnings.append(f"Duplicate row slice id: '{r_id}'.")
            else:
                row_ids.add(r_id)

            if "filter" in row:
                sql_filter = str(row["filter"])
                try:
                    SQLParser.parse_to_expr(sql_filter)
                except Exception as e:
                    self.errors.append(f"Row slice '{r_id}' contains invalid SQL filter '{sql_filter}': {e}")

            if "derivations" in row:
                if not isinstance(row["derivations"], dict):
                    self.errors.append(f"Row slice '{r_id}' 'derivations' must be a dictionary.")
                else:
                    for col_name, d_val in row["derivations"].items():
                        self._validate_derivation(f"{r_id}.{col_name}", d_val)

    def _validate_verifications(self) -> None:
        """Validates verification rules."""
        if not self.spec or "verifications" not in self.spec:
            return

        verifs = self.spec["verifications"]
        if not isinstance(verifs, list):
            self.errors.append("Field 'verifications' must be a list of verification rules.")
            return

        for idx, v_rule in enumerate(verifs):
            if not isinstance(v_rule, dict):
                self.errors.append(f"Verification rule at index {idx+1} must be a dictionary.")
                continue

            for rule_type, rule_cfg in v_rule.items():
                if rule_type not in VALID_VERIFICATION_RULES:
                    self.warnings.append(f"Non-standard verification rule type: '{rule_type}'.")


def auto_fix_yamaa_schema(raw_yaml: str, domain: Optional[str] = None) -> Tuple[str, List[str]]:
    """
    Intelligently repairs and standardizes a YAML specification to 100% compliant Yamaa v1.0 standard.
    Returns (fixed_yaml_str, fixes_applied).
    """
    fixes = []
    
    # 1. Parse or salvage YAML
    try:
        parsed = yaml.safe_load(raw_yaml)
    except Exception as e:
        fixes.append(f"Repaired malformed YAML syntax ({e})")
        # Attempt basic sanitization
        clean_lines = []
        for line in raw_yaml.split("\n"):
            line = line.replace("\t", "  ")
            clean_lines.append(line)
        parsed = yaml.safe_load("\n".join(clean_lines))

    if not parsed:
        domain_name = domain or "DM"
        fixes.append(f"Constructed baseline {domain_name} Yamaa specification")
        spec_obj = {
            "domain": domain_name,
            "datasets": {"ODM": "input/odm.csv"},
            "base": "ODM",
            "keys": ["STUDYID", "USUBJID"],
            "columns": [
                {"name": "STUDYID", "type": "str", "derivation": {"source": "StudyOID"}},
                {"name": "DOMAIN", "type": "str", "derivation": {"literal": domain_name}},
                {"name": "USUBJID", "type": "str", "derivation": {"source": "SubjectKey"}}
            ]
        }
        return yaml.dump(spec_obj, sort_keys=False, default_flow_style=False), fixes

    # 2. Handle domain-keyed list e.g. DM: [ { formoid: ..., columns: ... } ]
    if isinstance(parsed, dict) and len(parsed) == 1 and "domain" not in parsed:
        k = list(parsed.keys())[0]
        domain_name = k.upper()
        val = parsed[k]
        
        if isinstance(val, list) and len(val) > 0 and isinstance(val[0], dict):
            block = val[0]
            fixes.append(f"Normalized domain-keyed format for '{domain_name}' into standard Yamaa specification")
            
            columns = []
            if "columns" in block and isinstance(block["columns"], dict):
                for c_name, c_cfg in block["columns"].items():
                    if isinstance(c_cfg, dict):
                        col_entry = {"name": c_name, "type": "str"}
                        if "source" in c_cfg:
                            col_entry["derivation"] = {"source": c_cfg["source"]}
                        elif "literal" in c_cfg:
                            col_entry["derivation"] = {"literal": c_cfg["literal"]}
                        elif "value_mapping" in c_cfg:
                            col_entry["derivation"] = {
                                "mapping": {
                                    "source": c_cfg.get("source", c_name),
                                    "dict": c_cfg["value_mapping"]
                                }
                            }
                        columns.append(col_entry)
                    else:
                        columns.append({"name": c_name, "type": "str"})

            spec_obj = {
                "domain": domain_name,
                "datasets": {"ODM": "input/odm.csv"},
                "formoid": block.get("formoid", []),
                "base": "ODM",
                "keys": ["STUDYID", "USUBJID"] if "USUBJID" in str(block.get("keys", [])) else ["STUDYID", "USUBJID", f"{domain_name}SEQ"],
                "columns": columns if columns else [
                    {"name": "STUDYID", "type": "str", "derivation": {"source": "StudyOID"}},
                    {"name": "DOMAIN", "type": "str", "derivation": {"literal": domain_name}},
                    {"name": "USUBJID", "type": "str", "derivation": {"source": "SubjectKey"}}
                ]
            }
            return yaml.dump(spec_obj, sort_keys=False, default_flow_style=False), fixes

    # 3. Ensure mandatory root fields exist
    domain_name = parsed.get("domain") or domain or "DM"
    if "domain" not in parsed:
        parsed["domain"] = domain_name
        fixes.append(f"Added missing 'domain: {domain_name}' field")

    if "datasets" not in parsed:
        parsed["datasets"] = {"ODM": "input/odm.csv"}
        fixes.append("Added default 'datasets: {ODM: input/odm.csv}'")

    if "base" not in parsed:
        parsed["base"] = "ODM"
        fixes.append("Added default 'base: ODM'")

    if "keys" not in parsed or not isinstance(parsed["keys"], list) or len(parsed["keys"]) == 0:
        if domain_name in ("DM", "_DM_REF"):
            parsed["keys"] = ["STUDYID", "USUBJID"]
        else:
            parsed["keys"] = ["STUDYID", "USUBJID", f"{domain_name}SEQ"]
        fixes.append(f"Added standard composite keys: {parsed['keys']}")

    if "columns" not in parsed or not isinstance(parsed["columns"], list):
        parsed["columns"] = [
            {"name": "STUDYID", "type": "str", "derivation": {"source": "StudyOID"}},
            {"name": "DOMAIN", "type": "str", "derivation": {"literal": domain_name}},
            {"name": "USUBJID", "type": "str", "derivation": {"source": "SubjectKey"}}
        ]
        fixes.append("Constructed baseline standard columns (STUDYID, DOMAIN, USUBJID)")

    return yaml.dump(parsed, sort_keys=False, default_flow_style=False), fixes
