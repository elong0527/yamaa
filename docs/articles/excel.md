# Translating an Excel specification

## 1. A spec.yaml is one Dataset-sheet row plus a slice of the Variable sheet

Here is the same specification written twice. It is
[`adam-adsl-bmi-compute`](https://github.com/elong0527/yamaa/tree/main/benchmark/adam-adsl-bmi-compute), which reads a subject-level source and
adds one derived variable.

**As you would write it today.** A Dataset sheet row:

| Dataset | Description | Class | Structure | Key Variables |
|---|---|---|---|---|
| ADSL | Subject-Level Analysis Dataset | ADSL | One record per subject | STUDYID, USUBJID |

and five Variable sheet rows:

| Variable Name | Variable Label | Type | Length | Controlled Terms or Format | Origin | Core | Conversion Definition | Variable Type | Variable Order | Comments for Define |
|---|---|---|---|---|---|---|---|---|---|---|
| STUDYID | Study Identifier | Char | 20 | | Predecessor | Req | SOURCE.STUDYID | ADSL | 1 | |
| USUBJID | Unique Subject Identifier | Char | 30 | | Predecessor | Req | SOURCE.USUBJID | ADSL | 2 | |
| HEIGHTCM | Height (cm) | Num | 8 | | Predecessor | Perm | SOURCE.HEIGHTCM | ADSL | 3 | |
| WEIGHTKG | Weight (kg) | Num | 8 | | Predecessor | Perm | SOURCE.WEIGHTKG | ADSL | 4 | |
| BMI | Body Mass Index (kg/m2) | Num | 8 | | Derived | Perm | `BMI = WEIGHTKG / (HEIGHTCM/100)**2` | ADSL | 5 | |

**As yamaa writes it,** annotated with the cell each line replaces:

```yaml
schema_version: "1.0"          # which schema version this spec targets (exact match)
domain: ADSL                   # Dataset sheet -> Dataset
keys: [STUDYID, USUBJID]       # Dataset sheet -> Key Variables, and actually checked
input:                      # <- no cell for this; usually a Comment or a separate sheet
  SOURCE: input/adsl.csv
base: SOURCE                   # Dataset sheet -> Structure, but as the driver of the row count

output:
  path: adsl.csv             # <- no cell for this; the file, and its format
  columns: [STUDYID, USUBJID, HEIGHTCM, WEIGHTKG, BMI]   # what ships, in which order

columns:                       # this section (below) is the Variable sheet
  # rows 1-4 each read one source variable:
  #   - name: STUDYID
  #     type: str
  #     label: Study Identifier
  #     derivation: {source: SOURCE.STUDYID}
  - name: BMI                  #   Variable
    type: float                #   Type
    label: Body Mass Index (kg/m2)   # Label
    derivation:                #   Conversion Definition -- but executable
      compute:
        expr: "WEIGHTKG / POWER(NULLIF(HEIGHTCM, 0) / 100, 2)"

verifications:                 # <- no cell for this either
  - unique:
      columns: [STUDYID, USUBJID]
  - implies:
      id: bmi-missing-only-without-usable-height
      when: "BMI IS NULL"
      then: "HEIGHTCM IS NULL OR HEIGHTCM = 0"
```

### What the two tables above have no cell for

Line up the two versions and the interesting part is not what moved -- it is
what has no Excel counterpart at all. Four things:

1. **Which file the data comes from.** `Origin` says `Predecessor` and
   `Conversion Definition` names a *variable*; neither names a *file*.
2. **`NULLIF(HEIGHTCM, 0)`.** There is no cell that says what happens when
   height is zero -- missing, an error, or `Inf` are all plausible, and each
   programmer picks differently.
3. **Dependency order vs delivery order.** One `Variable Order` column does both
   jobs. yamaa splits them into `columns` order and `output.columns`.
4. **The two verifications.** `Key Variables` looks like it asserts uniqueness,
   but nothing executes it. The `implies` rule -- "BMI is empty only when
   height is unusable" -- normally survives as a sentence in a review email.

Going the other way, two of the eleven columns have no yamaa field:

- **`Core`** is a conformance classification against a CDISC implementation
  guide, not a statement about derivation. It travels in `column.metadata`.
- **`Comments for Define`** is documentation by definition -- also
  `column.metadata`. The template already separates it from `Conversion
  Definition`; yamaa just makes the split executable-vs-not.

---

## 2. The full mapping table

### 2.1 Dataset level

| Excel spec | yamaa | Notes |
|---|---|---|
| Dataset Name | `domain` | One dataset per spec |
| Dataset Label / Class / Structure | free keys under `metadata:` | Never validated; carried along |
| Key Variables | `keys` | Non-missing and unique are enforced |
| Sort Order (submission sort) | `output.order_by` | A presentation order applied after every check |
| Input datasets (usually only in a Comment) | `input:` | Every input is declared and named |
| Structure: "one record per subject per visit" | `base` plus `rows` templates | Row count comes from these, not from a sentence |
| Copy the corporate template and edit | `parents:` | Real layering; a change to the parent flows down (Specification composition) |
| Dataset-level review checks | `verifications:` | `unique`, `row_count`, `all_or_none`, `implies`, `assert` |

### 2.2 Variable level

| Excel column | yamaa | Notes |
|---|---|---|
| `Variable Name` | `column.name` | |
| `Variable Label` | `column.label` | |
| `Type` (Char / Num) | `column.type` | Closed set of five: `str` `int` `float` `date` `datetime` |
| `Length` | a `max_length` verification | It is a constraint, so it becomes an executed one. Add `column.metadata.length` when define.xml needs to show it |
| Significant digits / display format | *project setting* | Decimal places belong to the project, not the spec (Types and conversion) |
| `Controlled Terms or Format` | `mapping` / `lookup` / `allowed_values`, plus `column.metadata.codelist` | Translation and enforcement separate here too |
| `Origin` = Assigned | `literal: DM` | |
| `Origin` = Collected (CRF / eDT) | `source: {variable: ODM.Value, filter: "ODM.ItemOID = 'IT.DM.AGE'"}` | The `filter` says which collected records the value comes from |
| `Origin` = Predecessor | `source: ADSL.TRTSDT` | A qualified cross-dataset name performs a declared-key `lookup` (Lookup and joins) |
| `Origin` = Derived | a specific expression | See [the derivation vocabulary](schema-intro.md#the-derivation-vocabulary) |
| `Core` (Req / Exp / Perm) | `column.metadata` | Conformance classification; it says nothing about derivation |
| `Conversion Definition` | `derivation:` | From a sentence a person reads to an expression a machine runs |
| `Variable Order` | `columns` order **and** `output.columns` | One Excel column doing two jobs |
| `Comments for Define` | `column.metadata` | Free key-value, never validated, for define generation |
| Variable-level review checks | `column.verifications` | `not_missing`, `allowed_values`, `range`, `max_length`, `matches` |
| "if not collected then U" | the `missing:` handler | |
| "if not in codelist then 99" | the `unmapped:` handler | |
| "subject X was corrected to 99" | a `case` branch | |

### 2.3 Codelist splits into three constructs

Excel has one Codelist column. yamaa separates by where the vocabulary lives:

| Situation | yamaa | Example |
|---|---|---|
| Short vocabulary, written in the spec | `mapping` | `M -> M, F -> F` |
| Vocabulary is an external file (MedDRA, WHODrug, a reference-range table) | `lookup` | [`sdtm-ae-coding`](https://github.com/elong0527/yamaa/tree/main/benchmark/sdtm-ae-coding) |
| No translation, only a **check** that the value is one of these | `allowed_values` | `values: [M, F, U]` |
| Numeric banding (AGEGR1, BMI categories) | `cut` | [Example 1](#example-1-direct-mapping-a-codelist-and-numeric-banding) |

### 2.4 Value-level metadata

Value-level metadata (VLM) is what a Variable sheet cannot express: when a
variable's meaning depends on another variable's value, each value needs its
own derivation. `AVAL` is the standard case -- alanine aminotransferase where
`PARAMCD` is `ALT`, systolic blood pressure where it is `SYSBP`.

| Excel spec | yamaa |
|---|---|
| One VLM row (how AVAL is derived when PARAMCD = "ALT") | One row template under `rows:` |
| The VLM Where Clause | `row.filter` (an SQL predicate) |
| "this PARAM is derived from another PARAM" | Another row template with its own `literal` PARAMCD |
| "one collected record yields several analysis records" | Several row templates, appended in order |

See [`adam-adlb-bds`](https://github.com/elong0527/yamaa/tree/main/benchmark/adam-adlb-bds) in [More examples](#more-examples) below.

---

## 3. Worked equivalences

Three full walkthroughs. Each shows the Excel rows, then the YAML, then what
actually differs. All are real directories under `benchmark/` with fixed
expected output.

### Example 1: direct mapping, a codelist, and numeric banding

*Source: [`adam-adsl-demographics`](https://github.com/elong0527/yamaa/tree/main/benchmark/adam-adsl-demographics)*

Excel:

| Variable | Label | Type | Length | Origin | Codelist | Comment |
|---|---|---|---|---|---|---|
| SEX | Sex | Char | 1 | Predecessor: DM.SEX | SEX | Map to M/F/U, case-insensitive; if not collected or unrecognised -> U |
| SEXN | Sex (N) | Num | 8 | Derived | | M=1, F=2, U=0 |
| AGEGR1 | Pooled Age Group 1 | Char | 5 | Derived | AGEGR1 | <18 / 18-64 / >=65; UNKNOWN if AGE missing |

yamaa:

```yaml
  - name: SEX
    type: str
    label: Sex
    verifications:
      - not_missing: {}
      - allowed_values:
          values: [M, F, U]
    derivation:
      mapping:
        source: DM.SEX
        case_sensitive: false
        dict: {M: M, F: F, U: U}
        missing: U
        unmapped: U

  - name: AGEGR1
    type: str
    label: Pooled Age Group 1
    derivation:
      cut:
        source: AGE
        breaks: [18, 65]
        labels: ['<18', '18-64', '>=65']
        missing: UNKNOWN
```

What changed:

- Excel packs "if not collected -> U" and "if unrecognised -> U" into one
  sentence. yamaa splits them into `missing` and `unmapped` and requires
  **both to be written**, even when the answer is the same. Two conditions stay
  two conditions.
- The codelist *name* (`SEX`, `AGEGR1`) has no single home. The translation
  lives in `mapping.dict`, the check lives in `allowed_values`, and the name
  itself goes in `column.metadata.codelist` if you generate define.xml.

### Example 2: a Comment sentence becomes `compute`

*Source: [`adam-adsl-bmi-compute`](https://github.com/elong0527/yamaa/tree/main/benchmark/adam-adsl-bmi-compute)*

| Variable | Type | Origin | Comment |
|---|---|---|---|
| BMI | Num | Derived | BMI = WEIGHTKG / (HEIGHTCM/100)**2, rounded to 1 decimal |

```yaml
  - name: BMI
    type: float
    label: Body Mass Index (kg/m2)
    derivation:
      compute:
        expr: "WEIGHTKG / POWER(NULLIF(HEIGHTCM, 0) / 100, 2)"

verifications:
  - implies:
      id: bmi-missing-only-without-usable-height
      when: "BMI IS NULL"
      then: "HEIGHTCM IS NULL OR HEIGHTCM = 0"
```

What changed:

- `NULLIF(HEIGHTCM, 0)` writes the "what if height is zero" case -- almost never
  stated in an Excel spec -- **into the formula**. Division by zero is a
  failure, so it has to be stated.
- **"rounded to 1 decimal" has no translation, on purpose.** A derivation does
  not round; decimal places are a project rendering setting. Rounding belongs
  to the TFL, not to the ADaM value.
- The `implies` verification turns "BMI is empty exactly when height is
  unusable" -- normally a note to the reviewer -- into an executable assertion.

### Example 3: Predecessor and the declared-key lookup

*Source: [`adam-adae-treatment-emergent`](https://github.com/elong0527/yamaa/tree/main/benchmark/adam-adae-treatment-emergent)*

Excel:

| Variable | Origin | Comment |
|---|---|---|
| TRTSDT | Predecessor: ADSL.TRTSDT | merge by STUDYID USUBJID |
| TRTEMFL | Derived | Y if TRTSDT <= ASTDT <= TRTEDT (both boundaries inclusive), else blank |

```yaml
keys: [STUDYID, USUBJID, AESEQ]

  - name: TRTSDT
    type: date
    derivation:
      source: ADSL.TRTSDT          # <- no merge statement anywhere

  - name: TRTEMFL
    type: str
    derivation:
      case:
        - when: "ASTDT IS NOT NULL AND TRTSDT IS NOT NULL AND TRTEDT IS NOT NULL
                 AND ASTDT >= TRTSDT AND ASTDT <= TRTEDT"
          then:
            literal: Y
```

What changed:

- `source: ADSL.TRTSDT` reads across datasets through the Lookup and joins
  contract's declared-key join: the join keys are the *applicable keys* -- the output `keys` that also
  exist on the right side. So "merge by STUDYID USUBJID" is not written: it is
  a consequence of `keys`. Multiple matches fail by default; relaxing it
  requires an explicit `multiple_matches`.
- `case` has one branch and no `otherwise`, so the result is missing. "else
  blank" needs no statement.

### More examples

The same treatment for the remaining constructs, one line each -- open the
benchmark for the full side-by-side:

| Example | What it shows |
|---|---|
| [`adam-adlb-bds`](https://github.com/elong0527/yamaa/tree/main/benchmark/adam-adlb-bds) | VLM and BDS: one row template per PARAMCD, then `baseline_flag` / `baseline_value` / `row_number` as columns |
| [`adam-adex-cumulative-dose`](https://github.com/elong0527/yamaa/tree/main/benchmark/adam-adex-cumulative-dose) | `aggregate: "SUM(EX.EXDOSE)"` reducing by the applicable keys; a CSV field entering arithmetic must declare its type |
| [`adam-adae-partial-dates`](https://github.com/elong0527/yamaa/tree/main/benchmark/adam-adae-partial-dates) | `date_impute` beside `date_precision` reading the same source; `missing` and `invalid` are separate defects |
| [`sdtm-dm-metadata`](https://github.com/elong0527/yamaa/tree/main/benchmark/sdtm-dm-metadata) | `metadata` vs `verifications`: Length becomes both `metadata.length` (for define.xml) and a `max_length` check |
| [`sdtm-ae-coding`](https://github.com/elong0527/yamaa/tree/main/benchmark/sdtm-ae-coding) | Coding against MedDRA with `lookup`; named `intermediates` when several columns must come from one record |
| [`schema-inheritance`](https://github.com/elong0527/yamaa/tree/main/benchmark/schema-inheritance) | Corporate, compound and study layers via `parents:` -- real layering instead of copying the template |
