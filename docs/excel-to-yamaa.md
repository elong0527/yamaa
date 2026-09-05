---
title: Excel to YAMAA
---

# Translating an Excel specification

> **YAMAA docs:** [Why](why-yamaa.md) | [Excel to YAMAA](excel-to-yamaa.md) | [Schema concepts](schema-concepts.md) | [Examples walkthrough](yaml-examples-walkthrough.md)

> **Read this if** you have SDTM or ADaM specifications in Excel and want to
> know what each cell becomes. It maps every column of a Variable sheet, then
> works nine specifications through in full.

---

## 1. A spec.yaml is one Dataset-sheet row plus a slice of the Variable sheet

Here is the same specification written twice. It is
[`adam-adsl-bmi-compute`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-bmi-compute), which reads a subject-level source and
adds one derived variable.

**As you would write it today.** A Dataset sheet row:

| Dataset | Description | Class | Structure | Key Variables |
|---|---|---|---|---|
| ADSL | Subject-Level Analysis Dataset | ADSL | One record per subject | STUDYID, USUBJID |

and five Variable sheet rows. A typical template gives each variable eleven
cells:

| Variable Name | Variable Label | Type | Length | Controlled Terms or Format | Origin | Core | Conversion Definition | Variable Type | Variable Order | Comments for Define |
|---|---|---|---|---|---|---|---|---|---|---|
| STUDYID | Study Identifier | Char | 20 | | Predecessor | Req | SOURCE.STUDYID | ADSL | 1 | |
| USUBJID | Unique Subject Identifier | Char | 30 | | Predecessor | Req | SOURCE.USUBJID | ADSL | 2 | |
| HEIGHTCM | Height (cm) | Num | 8 | | Predecessor | Perm | SOURCE.HEIGHTCM | ADSL | 3 | |
| WEIGHTKG | Weight (kg) | Num | 8 | | Predecessor | Perm | SOURCE.WEIGHTKG | ADSL | 4 | |
| BMI | Body Mass Index (kg/m2) | Num | 8 | | Derived | Perm | `BMI = WEIGHTKG / (HEIGHTCM/100)**2` | ADSL | 5 | |

**As YAMAA writes it,** annotated with the cell each line replaces:

```yaml
schema_version: "1.0"          # which schema version this spec targets (exact match)
domain: ADSL                   # Dataset sheet -> Dataset
datasets:                      # <- no cell for this; usually a Comment or a separate sheet
  SOURCE: input/adsl.csv
base: SOURCE                   # Dataset sheet -> Structure, but as the driver of the row count
keys: [STUDYID, USUBJID]       # Dataset sheet -> Key Variables, and actually checked

output:
  profile: csv-v1              # <- no cell for this; how the artifact is written
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
   `Conversion Definition` names a *variable*; neither names a *file*. In
   practice the input is fixed in a separate sheet or in the program.
2. **`NULLIF(HEIGHTCM, 0)`.** `Conversion Definition` gives the formula for the
   normal case. There is no cell that says what happens when height is zero,
   and the three plausible answers -- missing, an error, or `Inf` -- are exactly
   the semantics-layer divergence described in
   [Why YAMAA](why-yamaa.md).
3. **The distinction between dependency order and delivery order.** There is
   one `Variable Order` column and it does both jobs. They are different
   questions: `BMI` must be computed *after* `HEIGHTCM` and `WEIGHTKG` exist,
   but it could be delivered anywhere in the artifact. YAMAA splits them into
   `columns` order and `output.columns`.
4. **The two verifications.** `Key Variables` looks like it asserts uniqueness,
   but nothing executes it. The `implies` rule -- "BMI is empty only when height
   is unusable" -- normally survives as a sentence in a review email.

Going the other way, two of the eleven columns have no YAMAA field at all:

- **`Core`** (`Req` / `Exp` / `Perm`) is a conformance classification against a
  CDISC implementation guide, not a statement about how a value is derived. It
  travels in `column.metadata`.
- **`Comments for Define`** is documentation by definition. It is
  `column.metadata` too -- and the fact that the template already separates it
  from `Conversion Definition` is the same split YAMAA makes between `metadata`
  and `derivation`. The template got that one right; what it cannot do is stop
  a reader from putting derivation logic in the comment column, because neither
  column is executed.

---

---

## 2. The full mapping table

### 2.1 Dataset level

| Excel spec | YAMAA | Notes |
|---|---|---|
| Dataset Name | `domain` | One dataset per spec |
| Dataset Label / Class / Structure | free keys under `metadata:` | **Never validated**; carried along |
| Standard / IG Version | `metadata.standard` | Same |
| Key Variables | `keys` | Non-missing and unique are enforced |
| Sort Order (submission sort) | `output.order_by` | R005: a presentation order applied after every check, with ties falling back to construction order. A term may name a working column the artifact does not ship |
| Input datasets (usually only in a Comment) | `datasets:` | Every input is declared and named |
| Structure: "one record per subject per visit" | `base` plus `rows` templates | Row count comes from these, not from a sentence |
| Copy the corporate template and edit | `parents:` | Real layering; a change to the parent flows down (R017) |
| Dataset-level review checks | `verifications:` | `unique`, `row_count`, `all_or_none`, `implies`, `predicate`. A `row_count` may take a `group_by` and a `filter`, which is how "exactly one baseline per subject and parameter" is stated |

### 2.2 Variable level

Left column names are from the template in section 1; yours may differ in
wording, but the eleven jobs are the same.

| Excel column | YAMAA | Notes |
|---|---|---|
| `Variable Name` | `column.name` | |
| `Variable Label` | `column.label` | |
| `Type` (Char / Num) | `column.type` | One closed set of five: `str` `int` `float` `date` `datetime` |
| `Variable Type` (SDTM / SUPP) | *no field* -- it is a **second** `spec.yaml` | `domain` fixes one dataset per specification, so SUPP qualifiers are their own spec |
| `Length` | a `max_length` verification | It is a constraint, so it becomes an executed one; **Length is not a type**. Add `column.metadata.length` when define.xml needs to show it |
| Significant digits / display format | *project setting* | R011: decimal places belong to the project, not the spec |
| `Controlled Terms or Format` | `mapping.dict` / `mapping_from` / `allowed_values`, plus `column.metadata.codelist` | See 3.3 -- translation and enforcement separate here too |
| `Origin` = Assigned | `literal: DM` | |
| `Origin` = Collected (CRF / eDT) | `source: ODM.IT.DM.AGE` | |
| `Origin` = Predecessor | `source: ADSL.TRTSDT` | A qualified cross-dataset name performs an **automatic left join** (R003) |
| `Origin` = Derived | a specific expression | See [the verb table](schema-concepts.md) |
| `Core` (Req / Exp / Perm) | `column.metadata` | A conformance classification against an IG; it says nothing about derivation |
| `Conversion Definition` | `derivation:` | From a sentence a person reads to an expression a machine runs |
| `Variable Order` | `columns` order **and** `output.columns` | One Excel column doing two jobs: dependency order and delivery order |
| `Comments for Define` | `column.metadata` | Free key-value, never validated, for define generation |
| Method OID (a reusable define.xml method) | *no separate object* | Algorithms are inline; reuse across specs is `parents` (R017) |
| Variable-level review checks | `column.verifications` | `not_missing`, `allowed_values`, `range`, `max_length`, `matches` |
| "if not collected then U" | the `missing:` handler | See 4.4 |
| "if not in codelist then 99" | the `unmapped:` handler | See 4.4 |
| "subject X was corrected to 99" | `override:` | See 4.4 |

Read the bottom five rows as the punchline: everything below `Comments for
Define` is something a specification has to say and an Excel template has no
column for.

### 2.3 Codelist splits into three constructs

Excel has one Codelist column. YAMAA separates by where the vocabulary lives,
which makes a good teaching moment:

| Situation | YAMAA | Example |
|---|---|---|
| Short vocabulary, written in the spec | `mapping` | `M -> M, F -> F` |
| Vocabulary is an external file (MedDRA, WHODrug, a reference-range table) | `mapping_from` | Example 8 |
| No translation, only a **check** that the value is one of these | `allowed_values` | `values: [M, F, U]` |
| Numeric banding (AGEGR1, BMI categories) | `cut` | Example 1 |

### 2.4 Value-level metadata

Value-level metadata (VLM) is what a Variable sheet cannot express: when a
variable's meaning depends on another variable's value, each value needs its
own derivation. `AVAL` is the standard case -- alanine aminotransferase where
`PARAMCD` is `ALT`, systolic blood pressure where it is `SYSBP`.

| Excel spec | YAMAA |
|---|---|
| One VLM row (how AVAL is derived when PARAMCD = "ALT") | One row template under `rows:` |
| The VLM Where Clause | `row.filter` (an SQL predicate) |
| "this PARAM is derived from another PARAM" | Another row template with its own `literal` PARAMCD |
| "one collected record yields several analysis records" | Several row templates, appended in order |

This is where the two formats line up most directly -- see example 4.

---

---

## 3. Nine worked equivalences

Each one shows the Excel rows first, then the YAML, then what actually
differs. All of them are real directories under `yaml/examples/` with fixed
expected output.

### Example 1: direct mapping, a codelist, and numeric banding

*Source: [`adam-adsl-mapping`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-mapping)*

Excel:

| Variable | Label | Type | Length | Origin | Codelist | Comment |
|---|---|---|---|---|---|---|
| SEX | Sex | Char | 1 | Predecessor: DM.SEX | SEX | Map to M/F/U, case-insensitive; if not collected or unrecognised -> U |
| SEXN | Sex (N) | Num | 8 | Derived | | M=1, F=2, U=0 |
| AGEGR1 | Pooled Age Group 1 | Char | 5 | Derived | AGEGR1 | <18 / 18-64 / >=65; UNKNOWN if AGE missing |

YAMAA:

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

  - name: SEXN
    type: int
    label: Sex (N)
    derivation:
      mapping:
        source: DM.SEX
        case_sensitive: false
        dict: {M: 1, F: 2, U: 0}
        missing: 0
        unmapped: 0

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
  sentence. YAMAA splits them into `missing` and `unmapped` and requires
  **both to be written**, even when the answer is the same. Two conditions stay
  two conditions.
- `SEXN` maps `DM.SEX` again rather than deriving from `SEX`. One collected
  value feeds three output columns, each with its own vocabulary -- in Excel
  this is usually implied by a Comment saying "same as SEX".
- The codelist *name* (`SEX`, `AGEGR1`) has no single home. The translation
  lives in `mapping.dict`, the check lives in `allowed_values`, and the name
  itself goes in `column.metadata.codelist` if you generate define.xml.

### Example 2: a Comment sentence becomes `compute`

*Source: [`adam-adsl-bmi-compute`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-bmi-compute)*

This is the specification from section 1, read again for its formula. The Excel
row now carries the rounding instruction such a Comment usually carries:

| Variable | Type | Origin | Comment |
|---|---|---|---|
| BMI | Num | Derived | BMI = WEIGHTKG / (HEIGHTCM/100)**2, rounded to 1 decimal |

YAMAA:

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
  stated in an Excel spec -- **into the formula**. R010 makes division by zero a
  failure rather than a silent missing value, so it has to be stated.
- **"rounded to 1 decimal" has no translation, on purpose.** A derivation does
  not round; decimal places are a project rendering setting (the example suite
  uses four). This is the point that generates the most discussion: rounding
  belongs to the TFL, not to the ADaM value.
- The `implies` verification turns "BMI is empty exactly when height is
  unusable" -- normally a note to the reviewer -- into an executable assertion.

### Example 3: Predecessor and the automatic left join

*Source: [`adam-adae-treatment-emergent`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-treatment-emergent)*

Excel:

| Variable | Origin | Comment |
|---|---|---|
| TRTSDT | Predecessor: ADSL.TRTSDT | merge by STUDYID USUBJID |
| TRTEDT | Predecessor: ADSL.TRTEDT | merge by STUDYID USUBJID |
| TRTEMFL | Derived | Y if TRTSDT <= ASTDT <= TRTEDT (both boundaries inclusive), else blank |

YAMAA:

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
        branches:
          - when: "ASTDT IS NOT NULL AND TRTSDT IS NOT NULL AND TRTEDT IS NOT NULL
                   AND ASTDT >= TRTSDT AND ASTDT <= TRTEDT"
            then:
              literal: Y
    verifications:
      allowed_values:
        values: [Y]

verifications:
  - all_or_none:
      id: treatment-period-completeness
      columns: [TRTSDT, TRTEDT]
  - implies:
      id: treatment-emergent-event-is-within-treatment
      when: "TRTEMFL = 'Y'"
      then: "ASTDT >= TRTSDT AND ASTDT <= TRTEDT"
```

What changed:

- `source: ADSL.TRTSDT` triggers R003's **automatic left join**. The join keys
  are the *applicable keys*: the output `keys` that also exist on the right
  side -- here `STUDYID` and `USUBJID`, since ADSL has no `AESEQ`. So "merge by
  STUDYID USUBJID" is not written: it is a consequence of `keys`.
- That join **requires the right side to be unique on those keys.** Multiple
  matches fail by default; relaxing it requires an explicit
  `multiple_matches`. In SAS, a many-to-one merge going wrong is usually
  discovered from a NOTE.
- `case` has one branch and no `otherwise`, so the result is missing. "else
  blank" needs no statement.
- `all_or_none` turns "TRTSDT and TRTEDT are either both present or both
  absent" into a check.

### Example 4: VLM and BDS in one spec

*Source: [`adam-adlb-bds`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adlb-bds)*

This is the closest structural match to an Excel spec and the one worth the
most time. It builds a Basic Data Structure (BDS) dataset, the ADaM shape where
one row is one subject, parameter and visit, and where the value-level metadata
decides what each `PARAMCD` means.

The VLM sheet:

| Where Clause | PARAMCD | PARAM | AVAL | AVALU |
|---|---|---|---|---|
| LBTESTCD = "ALT" | ALT | Alanine Aminotransferase | LB.LBSTRESN | LB.LBSTRESU |
| LBTESTCD = "ALT" | ALTSI | Alanine Aminotransferase (SI) | LBSTRESN * 0.0167 | ukat/L |
| LBTESTCD = "AST" | AST | Aspartate Aminotransferase | LB.LBSTRESN | LB.LBSTRESU |

The `rows:` section:

```yaml
rows:
  - id: alt
    filter: "LB.LBTESTCD = 'ALT' AND LB.LBSTRESN IS NOT NULL"
    derivations:
      PARAMCD: {literal: ALT}
      PARAM:   {literal: Alanine Aminotransferase}
      AVAL:    {source: LB.LBSTRESN}
      AVALU:   {source: LB.LBSTRESU}

  - id: alt_si
    filter: "LB.LBTESTCD = 'ALT' AND LB.LBSTRESN IS NOT NULL"
    derivations:
      PARAMCD: {literal: ALTSI}
      PARAM:   {literal: Alanine Aminotransferase (SI)}
      AVAL:
        compute:
          expr: "LB.LBSTRESN * 0.0167"
      AVALU:   {literal: ukat/L}

  - id: ast
    filter: "LB.LBTESTCD = 'AST' AND LB.LBSTRESN IS NOT NULL"
    derivations:
      PARAMCD: {literal: AST}
      PARAM:   {literal: Aspartate Aminotransferase}
      AVAL:    {source: LB.LBSTRESN}
      AVALU:   {source: LB.LBSTRESU}
```

The standard BDS derivations are written once, in `columns:`, for every
parameter:

```yaml
  - name: ABLFL
    type: str
    derivation:
      baseline_flag:
        group_by: [STUDYID, USUBJID, PARAMCD]
        date: ADT
        reference_date: TRTSDT

  - name: BASE
    type: float
    derivation:
      baseline_value:
        group_by: [STUDYID, USUBJID, PARAMCD]
        value: AVAL
        flag: ABLFL

  - name: CHG
    type: float
    derivation:
      compute: {expr: "AVAL - BASE"}

  - name: PCHG
    type: float
    derivation:
      compute: {expr: "100 * (AVAL - BASE) / NULLIF(BASE, 0)"}

  - name: ASEQ
    type: int
    derivation:
      row_number:
        group_by: [STUDYID, USUBJID]
        order_by: [PARAMCD, ADT]
```

Take this one slowly:

- **`rows` sets the row count; `columns` cannot change it.** That is R001's
  two-phase model. In an Excel spec, which step adds records is usually
  inferred from the Structure sentence.
- `alt` and `alt_si` share a filter, so each ALT record produces **two** rows.
  A derived parameter is just another row template -- no new concept is needed.
- `PARAMCD` and `AVAL` are **declared without a derivation** in `columns:`,
  because the row templates supply them. R005 requires a column to be derived
  either at column level or in **every** row template -- never in some of them.
  That turns the classic "one blank VLM cell" into a hard error.
- `TRTSDT` and `TRT01A` arrive from ADSL through the example-3 join, without
  changing the row count.
- `NULLIF(BASE, 0)` in `PCHG`: a zero baseline yields a change but no percent
  change. Excel specs frequently omit that sentence.
- `ASEQ` uses `row_number` **after every row exists**, so it is unique by
  construction.

### Example 5: one-to-many summarization with `aggregate`

*Source: [`adam-adex-cumulative-dose`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adex-cumulative-dose)*

Excel:

| Variable | Origin | Comment |
|---|---|---|
| DOSECUM | Derived | Sum of EX.EXDOSE for the subject and treatment |
| NCYCLES | Derived | Number of EX records for the subject and treatment |
| RDI | Derived | 100 * DOSECUM / (PLANDOSE * PLANCYC) |

YAMAA:

```yaml
datasets:
  TRT: input/subject_treatment.csv
  EX: {path: input/ex.csv, types: {EXDOSE: float}}
base: TRT
keys: [STUDYID, USUBJID, EXTRT]

  - name: DOSECUM
    type: float
    derivation:
      aggregate: "SUM(EX.EXDOSE)"

  - name: NCYCLES
    type: int
    derivation:
      aggregate: "COUNT(EX.EXSEQ)"

  - name: PLANDOSE           # internal; not in output.columns
    type: float
    derivation:
      source: TRT.PLANDOSE

  - name: RDI
    type: float
    derivation:
      compute:
        expr: "100 * DOSECUM / NULLIF(PLANDOSE * PLANCYC, 0)"
```

What changed:

- `aggregate: "SUM(EX.EXDOSE)"` declares no `group_by`, so it reduces **by the
  applicable keys** -- `STUDYID`, `USUBJID`, `EXTRT`. Omission does not mean
  "reduce all of EX as one group", and the example README says so explicitly.
- `EX: {path: ..., types: {EXDOSE: float}}` -- a CSV is a typeless container, so
  R014 makes **every field `str` by default**. A field entering arithmetic must
  declare its type. This is the `input`/`put` conversion an Excel spec never
  mentions but a programmer always writes.
- `PLANDOSE` and `PLANCYC` are declared but excluded from `output.columns`.
- `compute` accepts only **unqualified** identifiers (current output columns),
  so `SUM(...)` is bound to `DOSECUM` first and the arithmetic reads that
  column. This is the "no nested expressions, use named intermediates" design
  in practice.

### Example 6: partial dates and the imputation flag

*Source: [`adam-adae-partial-dates`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-partial-dates)*

In Excel this is normally a paragraph:

> ASTDT: impute from AESTDTC. If only year is collected, do not impute (leave
> blank). If year and month are collected, impute day = 15. ASTDTF = "D" when
> the day was imputed.

YAMAA:

```yaml
  - name: ASTDT
    type: date
    derivation:
      date_impute:
        source: AESTDTC
        month: 6
        day: 15
        minimum_source_precision: month     # year only -> leave blank
        missing: null
        invalid: null

  - name: ASTDTPR                            # internal: collected precision
    type: str
    derivation:
      date_precision:
        source: AESTDTC
        missing: null
        invalid: null

  - name: ASTDTF
    type: str
    derivation:
      case:
        branches:
          - when: "ASTDT IS NOT NULL AND ASTDTPR = 'M'"
            then: {literal: "D"}
    verifications:
      allowed_values:
        values: ["D"]

verifications:
  - all_or_none:
      id: analysis-date-completeness
      columns: [ASTDTC, ASTDT]
  - implies:
      id: imputation-flag-requires-an-analysis-date
      when: "ASTDTF IS NOT NULL"
      then: "ASTDT IS NOT NULL"
```

What changed:

- The imputation flag does not depend on remembering what was imputed. It
  re-reads the same source with `date_precision`, gets `D`/`M`/`Y`, and decides
  with `case`. Two expressions read one variable and the logic is fully
  explicit.
- `minimum_source_precision: month` *is* the sentence "year only -> do not
  impute".
- `missing` and `invalid` are separate: not collected, versus collected but not
  a valid ISO date, are different defects and may get different answers. Excel
  specs usually cover only one.
- `AESTDTC` stays `str` (the raw ISO text) while `ASTDT` is a `date`. That
  distinction is what makes `ASTDT >= TRTSDT` a date comparison rather than a
  string comparison.

### Example 7: define.xml metadata versus executable checks

*Source: [`sdtm-dm-metadata-contract`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-dm-metadata-contract)*

```yaml
keys: [STUDYID, USUBJID]
metadata:
  dataset_label: Demographics
  dataset_class: SPECIAL PURPOSE
  dataset_structure: One record per subject
  standard: SDTMIG 3.4

columns:
  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation:
      str_concat:
        sources:
          - source: DM_RAW.STUDYID
          - literal: '-'
          - source: DM_RAW.SITEID
          - literal: '-'
          - source: DM_RAW.SUBJID
    metadata:
      origin: Derived
      length: "30"
    verifications:
      - not_missing: {}
      - max_length:
          max: 30
```

The point that gets challenged most often:

- **`metadata` and `verifications` are different things.**
  `metadata.length: "30"` is text for define.xml and **nothing validates it**;
  `max_length: {max: 30}` actually runs and fails the whole run when exceeded.
- So the Excel Length column often becomes **two** statements in YAMAA: one for
  humans and define.xml, one for the machine. That is not redundancy -- it
  separates documentation from contract. The Excel failure mode is precisely
  that the two live in one cell, look like a contract, and are enforced by
  nobody.
- `str_concat` is the one string operation whose `sources` may mix `source` and
  `literal`, because putting literals between sources is what concatenation is.

### Example 8: coding against an external dictionary

*Source: [`sdtm-ae-dictionary-coding`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-ae-dictionary-coding)*

```yaml
datasets:
  AE_RAW: input/ae_raw.csv
  MEDDRA: input/meddra_26_1.csv
metadata:
  dictionary: MedDRA
  dictionary_version: "26.1"

  - name: AEDECOD
    type: str
    label: Dictionary-Derived Term
    derivation:
      mapping_from:
        source: AE_RAW.AETERM
        dataset: MEDDRA
        key: LLTNAME
        value: PTNAME
        missing: NOT CODED
        unmapped: NOT CODED

  - name: AEBODSYS
    type: str
    derivation:
      mapping_from:
        source: AE_RAW.AETERM
        dataset: MEDDRA
        key: LLTNAME
        value: SOCNAME
        missing: NOT CODED
        unmapped: NOT CODED
```

- `mapping_from` is **not** the R003 join. Its keys are declared explicitly
  (`source` and `key` pair by position) and output `keys` are never consulted,
  which is exactly why it can reach a table keyed on something else.
- It requires the key combination to be unique in the dictionary; a duplicate
  fails ([`negative-mapping-from-duplicate-key`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-mapping-from-duplicate-key) pins that error).
- The dictionary version travels with the spec in `metadata`.

**A related contrast.** `AEDECOD` and `AEBODSYS` above each perform their own
lookup, and nothing guarantees they reached the same record. When several
columns must come from **one** record, the construct is `record_lookups`:

```yaml
record_lookups:
  - id: DEATHEV
    dataset: AE
    source: [STUDYID, USUBJID]
    key: [STUDYID, USUBJID]
    filter: "AE.AEOUT = 'FATAL'"
    unmatched: missing

  - name: DTHCAUS
    derivation: {source: DEATHEV.AEDECOD}
  - name: EVDTHDT
    derivation: {source: DEATHEV.ASTDT}
```

R015 argues this directly: an expression returns one value, so every column
reading another dataset does its own match. Two columns that are supposed to
describe one record -- a date and the sequence number identifying it, a value
and its unit -- therefore state the match twice and agree **only by
construction**. A reviewer cannot see the agreement, and editing one statement
and not the other breaks it silently. A record lookup states the match once and
names the record. Excel has no such concept, but it has the bug.

### Example 9: organization, compound and study layers

*Source: [`adam-adlb-standardized-result`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adlb-standardized-result)*

The Excel approach is to save a copy of the corporate spec and edit it. Once
the parent changes, every copy that has already diverged stays diverged. R017
turns this into real layers.

```yaml
# layers/organization.yaml -- corporate layer: data contract and common columns
schema_version: "1.0"
datasets:
  LB: ../input/lb.csv
  UNUSED: ../input/not-used.csv
base: LB
record_lookups:
  - id: unused_reference
    dataset: UNUSED
columns:
  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: {source: LB.USUBJID}
  - name: AVAL
    type: float
    label: Standardized Numeric Result
    derivation: {source: LB.LBSTRESN}
  - name: AVALU
    type: str
    label: Analysis Value Unit
    derivation: {source: LB.LBSTRESU}
  - name: TEMP
    type: str
    label: Unused Intermediate Value
    derivation: {literal: unused}
verifications:
  unique:
    columns: [USUBJID, PARAMCD]
metadata: {scope: organization}
```

```yaml
# layers/compound.yaml -- compound layer: add types, retitle one column
schema_version: "1.0"
parents: organization.yaml
datasets:
  LB:
    types: {USUBJID: str, LBTESTCD: str, LBSTRESN: float, LBSTRESU: str}
columns:
  - name: AVAL
    label: Compound Standardized Result
metadata: {scope: compound}
```

```yaml
# layers/study.yaml -- study layer
schema_version: "1.0"
parents: [organization.yaml, compound.yaml]
columns:
  - name: AVALU
    label: Standardized Analysis Unit
metadata: {scope: study}
```

```yaml
# spec.yaml -- the deliverable, and the entry file
schema_version: "1.0"
parents: layers/study.yaml
domain: ADLB
keys: [USUBJID, PARAMCD]
output:
  columns: [USUBJID, PARAMCD, AVAL, AVALU]
columns:
  - name: AVAL
    label: Analysis Value
```

What to point out:

- **Resolution is depth-first, left to right, and later wins**:
  `organization -> compound -> study -> spec`. So `AVAL.label` ends as
  `Analysis Value`. A difference between two parents is settled by their order;
  it is **not** a conflict error.
- **Composition is shallow -- the most commonly misread rule.** Only the four
  keyed collections (`datasets`, `record_lookups`, `columns`, `rows`) merge
  member fields by identifier. Every other root field is **replaced whole**. So
  a child writing `AVAL.label` changes only the label, but a child writing
  `AVAL.derivation` replaces the **entire** derivation, even if both use the
  same expression keyword. Likewise a child `metadata` replaces the inherited
  `metadata` completely.
- **A YAML null at a composition boundary clears** an inherited optional field.
  It is not "set to a missing value" -- `derivation: {literal: null}` is a null
  *inside* a field value and still means "derive a missing value".
- **The entry file must declare a complete `output`.** An inherited layer
  cannot decide the final artifact's membership or order.
- **Pruning.** This example deliberately leaves an `UNUSED` dataset, an
  `unused_reference` lookup and a `TEMP` column in the corporate layer. Nothing
  reachable references them, so they are removed during resolution. **A
  corporate layer can therefore be generous, and a study carries only what it
  actually uses** -- which is exactly what an Excel template cannot do, since
  the rows it brings never leave your copy.
- Semantic validation runs **after** pruning. An unresolved reference that
  survives only inside a discarded declaration is not an error.

Together with `environment.yaml`
(project functions, [schema concepts](schema-concepts.md)),
this gives the complete
three-tier reuse story: **structure is reused through `parents`, algorithms
through project functions.**

---

---

## Appendix: a skeleton to copy

```yaml
schema_version: "1.0"
parents: layers/study.yaml                         # optional: inherit shared layers
domain: ADXX
datasets:
  SRC:  input/src.csv                              # typeless container: fields default to str
  ADSL: {path: input/adsl.csv, types: {AVAL: float}}
base: SRC
keys: [STUDYID, USUBJID, PARAMCD]

metadata:                       # for define.xml; not validated
  dataset_label: Example Analysis Dataset
  dataset_structure: One record per subject per parameter

record_lookups:                 # optional: only when columns must share one record
  - id: FIRSTEX
    dataset: EX
    order_by: [EX.EXSTDTC, EX.EXSEQ]
    keep: first
    unmatched: missing

output:                         # the entry file must declare this completely
  columns: [STUDYID, USUBJID, PARAMCD, PARAM, AVAL, ABLFL, BASE, CHG]

columns:                        # dependency order; anything not in output.columns is internal
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation:
      source: SRC.STUDYID
    metadata: {origin: Protocol, length: "20"}
    verifications:
      not_missing: {}

  - name: PARAMCD               # supplied by rows; declared without a derivation
    type: str
    label: Parameter Code

rows:                           # optional: only when one source record becomes several rows
  - id: template_a
    filter: "SRC.TESTCD = 'AAA' AND SRC.STRESN IS NOT NULL"
    derivations:
      PARAMCD: {literal: AAA}

verifications:
  - unique:
      columns: [STUDYID, USUBJID, PARAMCD]
  - implies:
      id: change-requires-a-baseline
      when: "CHG IS NOT NULL"
      then: "BASE IS NOT NULL"
```
