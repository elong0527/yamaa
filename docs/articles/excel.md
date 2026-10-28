# Translating an Excel specification

## 1. A spec.yaml is one Dataset-sheet row plus a slice of the Variable sheet

Here is the same specification written twice. It is
[`adam-adsl-bmi`](https://github.com/elong0527/yamaa/tree/main/benchmarks/adam-adsl-bmi), which reads a subject-level source and
derives `BMI` from height and weight with an inline expression.

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
yamaa: "1.0"                   # the oldest language version the file needs
type: spec                     # a specification: one dataset
model: ADaM                    # <- the workbook: an SDTM or an ADaM spec
domain: ADSL                   # Dataset sheet -> Dataset
keys: [STUDYID, USUBJID]       # Dataset sheet -> Key Variables, and checked
input:                         # <- no cell; often a Comment or another sheet
  SOURCE: input/adsl.csv
base: SOURCE                   # Dataset sheet -> Structure, as the row driver

output:                        # <- no cell: the file, and its format
  path: adsl.csv
  # What ships, in which order:
  columns: [STUDYID, USUBJID, HEIGHTCM, WEIGHTKG, BMI]

columns:                       # this section is the Variable sheet
  - name: STUDYID              # Variable Name
    type: str                  # Type
    label: Study Identifier    # Variable Label
    derivation: SOURCE.STUDYID # Conversion Definition

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: SOURCE.USUBJID

  - name: HEIGHTCM
    type: float
    label: Height (cm)
    derivation: SOURCE.HEIGHTCM

  - name: WEIGHTKG
    type: float
    label: Weight (kg)
    derivation: SOURCE.WEIGHTKG

  # The Conversion Definition, but executable:
  - name: BMI
    type: float
    label: Body Mass Index (kg/m2)
    derivation: "WEIGHTKG / POWER(NULLIF(HEIGHTCM, 0) / 100, 2)"

verifications:                 # <- no cell for this either
  - assert:
      id: bmi-missing-only-without-usable-inputs
      when: "BMI IS NULL"
      require: "HEIGHTCM IS NULL OR HEIGHTCM = 0 OR WEIGHTKG IS NULL"
```

A derivation is an expression or a verb from a closed registry. In an
expression, a bare name such as `SOURCE.STUDYID` or `WEIGHTKG` reads a column,
text is quoted (`"'Y'"`), and YAML `null` is the missing value.

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
4. **The checks.** `Key Variables` looks like it asserts uniqueness, but
   nothing executes it; `keys` does, so no separate uniqueness check is
   written. The `assert` rule -- "BMI is empty only when height or weight is
   unusable" -- normally survives as a sentence in a review email.

Going the other way, every one of the eleven columns has a yamaa field --
three live in governed submission metadata rather than in the derivation:

- **`Core`** is a conformance classification against a CDISC implementation
  guide, not a statement about derivation. It travels in `submission.core`
  (`Req` / `Exp` / `Perm` for SDTM and SEND; `Req` / `Cond` / `Perm` for
  ADaM).
- **`Origin`** is provenance for define.xml: `submission.origins`, a list,
  because Define-XML 2.1 allows more than one origin for a variable. The
  derivation must agree with it: an origin the derivation contradicts is
  rejected (SUB-19).
- **`Comments for Define`** is documentation by definition --
  `submission.comment`. The template already separates it from `Conversion
  Definition`; yamaa just makes the split executable-vs-governed-metadata.

A column that declares `submission` needs `model` at the root, so yamaa knows
which model's rules apply. The free-form `column.metadata` map is
uninterpreted annotation only: it is never merged with submission metadata
and must not carry a governed key such as `core`, `codelist`, `length`, or
`comment`.

---

## 2. The full mapping table

### 2.1 Dataset level

| Excel spec | yamaa | Notes |
|---|---|---|
| Dataset Name | `domain` | One dataset per spec |
| The workbook itself: an SDTM, SEND or ADaM specification | `model` | `SDTM`, `SEND` or `ADaM`; required once the spec declares submission metadata or a project section lists it, and a section lists only specs of its own model (SUB-3) |
| Dataset Label / Class / Structure | `submission.label` / `submission.class` / `submission.structure` | Governed dataset metadata, written under `submission:`; reserved in `metadata:`, which rejects them with `reserved_metadata_key` (SUB-2) |
| Standards and versions (SDTMIG 3.4, ADaMIG 1.3), on a Standards or Study sheet | the project section's `standards` list, and `submission.standard` | A section lists one or more implementation guides, and the first is the default; a dataset bound to another names it in `submission.standard` |
| Key Variables | `keys` | Non-missing and unique are enforced |
| Sort Order (submission sort) | `output.order_by` | A presentation order applied after every check |
| Input datasets (usually only in a Comment) | `input:` | Every input is declared and named; an input read by a join declares the columns it joins on in `key` |
| Structure: "one record per subject per visit" | `base` plus `rows` templates | Row count comes from these, not from a sentence |
| Copy the corporate template and edit | `parent:`, `include:` and `clear:` | Real layering; a change to the parent flows down, and a layer removes an inherited value only by listing its path in `clear` (Specification composition). A layer names one parent and lists the reusable fragments it adds under `include`; see [below](#a-fragment-shared-by-several-templates) |
| Dataset-level review checks | `verifications:` | `unique`, `row_count`, `all_or_none`, `assert` |

### A fragment shared by several templates

A study often builds a dataset from a template plus small reusable
fragments, each adding or changing a few variables: an adverse-event
duration, a treatment-period assignment. The study's file names the template
as its `parent` and lists the fragments under `include`. A fragment names no
parent and no fragments of its own, so one fragment file serves every
template that lists it:

```yaml
# ae-duration.yaml: the duration, on any template that gives ASTDT and AENDT
yamaa: "1.0"
type: spec

columns:
  - name: ADURN
    type: int
    label: Analysis Duration (N)
    derivation:
      diff: {type: date, start: ASTDT, end: AENDT, unit: day, closed: both}
```

```yaml
# adae.yaml: the study's ADAE, on the ADAE template, with the duration
yamaa: "1.0"
type: spec
parent: adae-template.yaml
include:
  - ae-duration.yaml

output:
  path: adae.csv
  columns: [USUBJID, AESEQ, ASTDT, AENDT, ADURN]
```

```yaml
# adce.yaml: the study's ADCE, on the ADCE template, with the same file
yamaa: "1.0"
type: spec
parent: adce-template.yaml
include:
  - ae-duration.yaml

output:
  path: adce.csv
  columns: [USUBJID, CESEQ, ASTDT, AENDT, ADURN]
```

Each file composes its template first, then the fragments it lists, in
order, then its own fields, so a later fragment and the file itself win where
they state the same detail (Specification composition). A fragment's paths
resolve beside the fragment. A fragment's `clear` removes what the template
and the fragments listed before it give, so a fragment that clears a value one
template sets fails as `invalid_clear` on a template that does not set it. A
file joins one specification at most once: listing a fragment that the
template already includes fails as `duplicate_layer`.

**Check a fragment on each template.** A fragment names no template, so a
check of the fragment alone stops at the first value a template would give.
Check it with each template it serves as its parent instead, for instance in
CI before any study file lists it: the check composes the template, then the
fragment, as a file that names the template as its `parent`, lists only that
fragment and writes nothing else would, and reports what that file's check
would report, each issue in the fragment or in a layer of the template:

```sh
yamaa check ae-duration.yaml --parent adae-template.yaml
yamaa check ae-duration.yaml --parent adce-template.yaml
```

In R and Python, `check(parent = )` does the same
([Check a specification without data](api.md#check-a-specification-without-data)).

### 2.2 Variable level

| Excel column | yamaa | Notes |
|---|---|---|
| `Variable Name` | `column.name` | |
| `Variable Label` | `column.label` | |
| `Type` (Char / Num) | `column.type` | Closed set of five: `str` `int` `float` `date` `datetime` |
| `Length` | a `max_length` verification plus `submission.length` | It is a constraint, so it becomes an executed one. `submission.length` is the bound every value must meet; on a `str` column with a `max_length` verification it is derived from `max` and need not be declared separately. define.xml shows it only when the section writes no transport: with a transport, a text column's length is the longest value the section publishes, and a text column may omit `length` when every section listing its specification writes one (SUB-7, SUB-24) |
| Significant digits / display format | `submission.significant_digits` / `submission.display_format` | Presentation metadata, not the stored value; emitted as SignificantDigits / def:DisplayFormat in define.xml (Types and conversion) |
| `Controlled Terms or Format` | `mapping` / `intermediates` / `allowed_values`, plus `submission.codelist` | Translation and enforcement separate here too; `submission.codelist` names a codelist that a codelist source of the project declares |
| `Origin` | `submission.origins` | One entry per origin, each with its `type`; SDTM also names the `source` responsible, SEND may, and ADaM derives it (SUB-15). The rows below give the derivation each type pairs with |
| `Origin` = Assigned | quoted text: `derivation: "'DM'"` | The quotes make it text; a bare `DM` would name a column |
| `Origin` = Collected (CRF / eDT) | `odm: ODM.IT.DM.AGE` | Reads the one record of the item among the ODM records the row was built from; `event`, `form`, `item_group`, and `filter` narrow it (Name binding) |
| `Origin` = Predecessor | a copy entry, `copy: {dataset: ADSL, columns: [TRTSDT]}`, or a read, `derivation: ADSL.TRTSDT` | A copy entry takes each column's type and label, and its Predecessor origin, length and codelist, from the producing spec the input links through `schema`; both forms join on the input's declared `key` (Lookup and joins) |
| `Origin` = Derived | an expression or a verb | See [the derivation vocabulary](schema-intro.md#the-derivation-vocabulary) |
| `Core` (`Req` / `Exp` / `Perm` for SDTM and SEND; `Req` / `Cond` / `Perm` for ADaM) | `submission.core` | Conformance classification; it says nothing about derivation |
| `Conversion Definition` | `derivation:` | From a sentence a person reads to an expression a machine runs |
| A sponsor macro the definition calls | `function: {name: bmi, args: {weight_kg: WEIGHTKG, height_cm: HEIGHTCM}}` | A `function` file declares the routine once, with its parameters, return type, tests and each host's implementation; the project lists the file under `functions`, and a spec that calls it runs with its project |
| `Variable Order` | `columns` order **and** `output.columns` | One Excel column doing two jobs |
| `Comments for Define` | `submission.comment` | Sponsor note carried with the column definition, for define generation |
| Variable-level review checks | `column.verifications` | `not_missing`, `allowed_values`, `range`, `max_length`, `matches` |
| "if not collected then U" | the `missing:` handler | Without `missing:`, a missing input leaves the value missing |
| "if not in codelist then 99" | the `unmapped:` handler | Without `unmapped:`, an unlisted value stops the run |
| "subject X was corrected to 99" | a `case` branch | |

### 2.3 Codelist splits into three constructs

Excel has one Codelist column. yamaa separates by where the vocabulary lives:

| Situation | yamaa | Example |
|---|---|---|
| Short vocabulary, written in the spec | `mapping` | `M -> M, F -> F` |
| Vocabulary is an external file (MedDRA, WHODrug, a reference-range table) | a named intermediate | [`sdtm-ae-coding`](https://github.com/elong0527/yamaa/tree/main/benchmarks/sdtm-ae-coding) |
| No translation, only a **check** that the value is one of these | `allowed_values` | `values: [M, F, U]` |
| Numeric banding (AGEGR1, BMI categories) | `cut` | [Example 1](#example-1-direct-mapping-a-codelist-and-numeric-banding) |

### Where the codelist name lives

One Excel `Controlled Terms or Format` cell collapses two declarations. yamaa
keeps them apart: the spec names the codelist, and a codelist source that the
project lists declares it. Adapted from
[`sdtm-dm-metadata`](https://github.com/elong0527/yamaa/tree/main/benchmarks/sdtm-dm-metadata),
the spec first, shortened to its keys and `SEX`:

```yaml
yamaa: "1.0"
type: spec
model: SDTM
domain: DM
keys: [STUDYID, USUBJID]
input:
  DM_RAW: input/dm_raw.csv

output:
  path: dm.csv
  columns: [STUDYID, USUBJID, SEX]

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: DM_RAW.STUDYID

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: DM_RAW.USUBJID

  - name: SEX
    type: str
    label: Sex
    derivation: DM_RAW.SEX
    submission:
      core: Req
      role: Record Qualifier
      length: 1
      codelist: SEX
      origins:
        - type: Collected
          source: Investigator
```

then the codelist file, one terminology source with its codelists:

```yaml
yamaa: "1.0"
type: codelists
standard:
  name: CDISC/NCI
  publishing_set: SDTM
  version: '2023-12-15'
codelists:
  - id: SEX
    name: Sex
    items:
      - value: F
        decode: Female
      - value: M
        decode: Male
      - value: U
        decode: Unknown
```

and the project file, which lists the codelist file and the spec:

```yaml
yamaa: "1.0"
type: project
codelists: [codelists/sdtm-ct.yaml]
sdtm:
  specs: [dm.yaml]
```

`submission.codelist` carries the `id`, not the values. The codelist file
carries the values once, and every column naming that `id` shares the one
statement. When the bound codelist declares `items`, the run checks every
non-missing value against them, and an `allowed_values` verification on the
same column is rejected as a second statement of the same list -- the check
the run executes and the terminology the document publishes come from one
declaration. The spec names what it needs from its project: run without the
project, it fails validation with `project_binding_missing` instead of
skipping the check. [Codelists and controlled terminology](terminology.md)
covers the sources and their versions.

### 2.4 Value-level metadata

Value-level metadata (VLM) is what a Variable sheet cannot express: when a
variable's meaning depends on another variable's value, each value needs its
own derivation. `AVAL` is the standard case -- alanine aminotransferase where
`PARAMCD` is `ALT`, systolic blood pressure where it is `SYSBP`.

| Excel spec | yamaa |
|---|---|
| One VLM row (how AVAL is derived when PARAMCD = "ALT") | One row template under `rows:` |
| The VLM Where Clause selecting the records to derive | `row.filter` (an SQL predicate) |
| The VLM metadata when PARAMCD = "ALT" (origin, codelist, precision) | One `submission.value_level` entry on the column, with `where: {PARAMCD: [ALT]}` |
| A Where Clause on several values or columns (PARAMCD in ALT, AST; LBSPEC = URINE) | A list of values: one value is EQ, several are IN, and several columns must all hold, as in `where: {LBTESTCD: [GLUC], LBSPEC: [URINE]}` |
| A Where Clause with NE or NOT IN (DTYPE is not AVERAGE) | `where: {PARAMCD: [SYSBP], DTYPE: {comparator: NE, values: [AVERAGE]}}`; `comparator: NOTIN` takes two or more values. A missing value matches no listed value, so NE and NOTIN select it and EQ and IN never do |
| "this PARAM is derived from another PARAM" | Another row template with its own PARAMCD, written as quoted text such as `"'BMI'"` |
| "one collected record yields several analysis records" | Several row templates, appended in order |

See [`adam-adlb-bds`](https://github.com/elong0527/yamaa/tree/main/benchmarks/adam-adlb-bds) in [More examples](#more-examples) below for the derivation, and [`adam-adlb-metadata`](https://github.com/elong0527/yamaa/tree/main/benchmarks/adam-adlb-metadata) for the metadata a define.xml value list carries.

---

## 3. Worked equivalences

Three full walkthroughs. Each shows the Excel rows, then the YAML, then what
actually differs. Each YAML block is a whole specification, cut down from a
real directory under `benchmarks/` with fixed expected output to the columns
its Excel rows describe.

### Example 1: direct mapping, a codelist, and numeric banding

*Source: [`adam-adsl-demographics`](https://github.com/elong0527/yamaa/tree/main/benchmarks/adam-adsl-demographics)*

Excel:

| Variable | Label | Type | Length | Origin | Codelist | Comment |
|---|---|---|---|---|---|---|
| SEX | Sex | Char | 1 | Predecessor: DM.SEX | SEX | Carry through as collected; if not collected -> U |
| SEXN | Sex (N) | Num | 8 | Derived | | M=1, F=2, U=0 |
| AGEGR1 | Pooled Age Group 1 | Char | 5 | Derived | AGEGR1 | <18 / 18-64 / >=65; UNKNOWN if AGE missing |

yamaa:

```yaml
yamaa: "1.0"
type: spec
model: ADaM
domain: ADSL
keys: [STUDYID, USUBJID]
input:
  # CSV fields are text, and cut needs a numeric source.
  DM: {path: input/dm.csv, types: {AGE: int}}

output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, SEX, SEXN, AGEGR1]

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: DM.STUDYID

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: DM.USUBJID

  - name: SEX
    type: str
    label: Sex
    derivation:
      first_available:
        sources: [DM.SEX]
        missing: U
    verifications:
      - not_missing
      - allowed_values:
          values: [M, F, U]
      - matches:
          pattern: '^[MFU]$'

  - name: SEXN
    type: int
    label: Sex (N)
    derivation:
      mapping:
        source: SEX
        dict:
          M: 1
          F: 2
          U: 0
        missing: 0

  - name: AGEGR1
    type: str
    label: Pooled Age Group 1
    derivation:
      cut:
        source: DM.AGE
        breaks: [18, 65]
        labels: ['<18', '18-64', '>=65']
        missing: UNKNOWN
```

What changed:

- SEX needs no translation at all: `first_available` carries the collected
  value through unchanged, and only the uncollected case needs a handler
  (`missing: U`). The M=1/F=2/U=0 coding is a separate `mapping` on SEXN,
  where the missing case repeats (`missing: 0`) and there is no
  "unrecognised" branch to answer -- SEX is already constrained to three
  values. Where a wording genuinely can surprise you, the same benchmark's
  RACEN answers a race the dictionary does not know with `unmapped: 99`.
  Without that handler an unlisted race stops the run instead of turning
  into a blank, while an empty race needs none: it leaves RACEN missing.
- Handler values, `dict` values and `cut` labels are value fields: `U`,
  `UNKNOWN` and the `cut` labels are text as written, never column names.
  Only an expression, such as a `case` result, quotes its text.
- The codelist *name* (`SEX`, `AGEGR1`) has no single home. The translation
  lives in `mapping.dict`, and the name goes in `submission.codelist` if you
  generate define.xml; a bound list of items is then also the check, so the
  `allowed_values` above is written only when no codelist is bound -- see
  [Where the codelist name lives](#where-the-codelist-name-lives) above.

### Example 2: a Comment sentence becomes an expression

*Source: [`adam-adsl-bmi`](https://github.com/elong0527/yamaa/tree/main/benchmarks/adam-adsl-bmi)*

| Variable | Type | Origin | Comment |
|---|---|---|---|
| BMI | Num | Derived | BMI = WEIGHTKG / (HEIGHTCM/100)**2, rounded to 1 decimal |

```yaml
yamaa: "1.0"
type: spec
model: ADaM
domain: ADSL
keys: [STUDYID, USUBJID]
input:
  SOURCE: input/adsl.csv

output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, BMI]

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: SOURCE.STUDYID

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: SOURCE.USUBJID

  # The formula reads columns, so height and weight are declared; they are
  # not in output.columns, so they are not published.
  - name: HEIGHTCM
    type: float
    label: Height (cm)
    derivation: SOURCE.HEIGHTCM

  - name: WEIGHTKG
    type: float
    label: Weight (kg)
    derivation: SOURCE.WEIGHTKG

  # The Excel row's "rounded to 1 decimal" is display-only here: the source
  # benchmark stores the unrounded value, so the rounding is a display
  # format in submission metadata, not part of the formula.
  - name: BMI
    type: float
    label: Body Mass Index (kg/m2)
    derivation: "WEIGHTKG / POWER(NULLIF(HEIGHTCM, 0) / 100, 2)"
```

What changed:

- The sentence becomes the derivation itself: an expression in which each
  bare name reads a column declared above it.
- `NULLIF(HEIGHTCM, 0)` writes the "what if height is zero" case -- almost never
  stated in an Excel spec -- **into the formula**. Division by zero is a
  failure, so it has to be stated.
- **"rounded to 1 decimal" translates to `round_half_away_from_zero`** (with
  `digits: 1`) -- the one rounding the language provides (FORMULA-16). Use it
  when the rounded value is what you store and validate; when only the
  *display* is rounded (a TFL or listing), leave the stored value unrounded
  and declare `submission.display_format` (here `8.1`) with
  `submission.significant_digits` among the column's submission metadata.

### Example 3: Predecessor and the declared-key join

Excel:

| Variable | Origin | Comment |
|---|---|---|
| TRTSDTM | Predecessor: ADSL.TRTSDTM | merge by STUDYID USUBJID |
| TRTEDTM | Predecessor: ADSL.TRTEDTM | merge by STUDYID USUBJID |
| EOTSTT | Predecessor: ADSL.EOTSTT | merge by STUDYID USUBJID |
| TEENDDTM | Derived | window end: TRTEDTM + 30 days if AESER = N, else + 90 days; blank while treatment is ongoing |
| TRTEMFL | Derived | Y if onset is on or after TRTSDTM and, unless treatment is ongoing, on or before TEENDDTM; Y if onset is missing and the subject was treated; else blank |

```yaml
yamaa: "1.0"
type: spec
model: ADaM
domain: ADAE
keys: [STUDYID, USUBJID, AESEQ]
input:
  AE: input/ae.csv               # <- AESTDTC stays text, as collected
  ADSL:
    path: input/adsl.csv
    types: {TRTSDTM: datetime, TRTEDTM: datetime}
    key: [STUDYID, USUBJID]      # <- "merge by STUDYID USUBJID", once
base: AE

output:
  path: adae.csv
  columns: [STUDYID, USUBJID, AESEQ, AESER, ASTDTM, TRTSDTM, TRTEDTM, EOTSTT,
            TEENDDTM, TRTEMFL]

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: AE.STUDYID

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: AE.USUBJID

  - name: AESEQ
    type: int
    label: Sequence Number
    derivation: AE.AESEQ

  - name: AESER
    type: str
    label: Serious Event
    derivation: AE.AESER

  # A date alone takes midnight; a start without its day gives none.
  - name: ASTDTM
    type: datetime
    label: Analysis Start Date/Time
    derivation:
      impute:
        type: datetime
        source: AE.AESTDTC
        time: first

  - name: TRTSDTM
    type: datetime
    label: Datetime of First Exposure to Treatment
    derivation: ADSL.TRTSDTM         # <- joins on the input's key

  - name: TRTEDTM
    type: datetime
    label: Datetime of Last Exposure to Treatment
    derivation: ADSL.TRTEDTM

  - name: EOTSTT
    type: str
    label: End of Treatment Status
    derivation: ADSL.EOTSTT

  - name: TEENDDTM
    type: datetime
    label: End of Treatment-Emergent Window
    derivation:
      case:
        - when: "EOTSTT = 'ONGOING'"
          then: null
        - when: "AESER = 'N'"
          then:
            shift: {type: datetime, source: TRTEDTM, amount: 30, unit: day}
        - otherwise:
            shift: {type: datetime, source: TRTEDTM, amount: 90, unit: day}

  - name: TRTEMFL
    type: str
    label: Treatment Emergent Analysis Flag
    derivation:
      case:
        - when: "TRTSDTM IS NULL"
          then: null
        - when: "ASTDTM IS NULL"
          then: "'Y'"
        - when: >-
            ASTDTM >= TRTSDTM
            AND (EOTSTT = 'ONGOING' OR ASTDTM <= TEENDDTM)
          then: "'Y'"
        - otherwise: null
    verifications:
      - allowed_values:
          values: [Y]
```

What changed:

- "merge by STUDYID USUBJID" is written once, as the input's `key`. Each
  qualified read, such as `ADSL.TRTSDTM`, joins ADSL to the current row on
  those columns, each matched against the row's column of the same name
  (Lookup and joins). A read of an input that declares no `key` fails
  validation with `no_applicable_keys` instead of guessing the join from the
  columns the two datasets share. Multiple matches fail by default; relaxing
  it requires a `source` binding or a named intermediate with explicit
  `order_by` and `keep`.
- When the input is linked to the spec that produces it (`schema: adsl.yaml`
  beside its `path`), the three Predecessor columns become one copy entry,
  `copy: {dataset: ADSL, columns: [TRTSDTM, TRTEDTM, EOTSTT]}`. Each copied
  column takes its type, label and Predecessor origin from the producer, so
  none of it is written twice. Here ADSL is a file that no spec produces, so
  the columns are read one by one.
- AESTDTC is read as text, as a CSV field is unless its input types it, and
  `impute` makes the analysis datetime in a derivation. An SDTM `--DTC` may
  hold a date alone or a partial date such as `2025-03`, and typing it
  `datetime` at ingest would fail the whole input on the first such value
  (`field_parse_failed`, INPUT-7). Here a date alone takes midnight under
  `time: first`, and a start without its day gives a missing ASTDTM, which
  the flag below treats as a missing onset. A study that completes partial
  starts declares a `day` rule and a treatment-start bound, as
  [`adam-adae-partial-dates`](https://github.com/elong0527/yamaa/tree/main/benchmarks/adam-adae-partial-dates)
  does.
- "TRTEDTM + 30 days" becomes a `shift` of `TRTEDTM` by an elapsed amount
  in days, where a day is 24 hours. A missing `TRTEDTM` gives a missing
  window end, as for an untreated subject. The 30 and the 90 are written
  once, in the one column a protocol with other windows changes.
- The flag is a `case`, not a bare `flag` predicate, because the Excel
  comment's rule has edges a single comparison gets wrong. The window has
  an end (`TEENDDTM`): an onset after it is not treatment-emergent, unless
  treatment is still ongoing and the window has no end yet. And a
  comparison with a missing moment is unknown, never true -- so a bare
  `flag: "ASTDTM >= TRTSDTM"` would leave a missing onset blank, where this
  study's convention flags it `Y` for a treated subject (a missing onset
  cannot be ruled pre-treatment) and blank only for an untreated one. The
  `case` states each branch, with `"'Y'"` as quoted text and `null` as the
  missing value, and the `allowed_values` check pins the flag to the one
  value it may carry.

### More examples

The same treatment for the remaining constructs, one line each -- open the
benchmark for the full side-by-side:

| Example | What it shows |
|---|---|
| [`adam-adlb-bds`](https://github.com/elong0527/yamaa/tree/main/benchmarks/adam-adlb-bds) | VLM and BDS: one row template per PARAMCD, then `baseline_flag` / `aggregate`-with-`filter` / `row_number` as columns |
| [`adam-adex-cumulative-dose`](https://github.com/elong0527/yamaa/tree/main/benchmarks/adam-adex-cumulative-dose) | `aggregate: "SUM(EX.EXDOSE)"`, matching each row on the input's declared `key`; a CSV field entering arithmetic must declare its type |
| [`adam-adae-partial-dates`](https://github.com/elong0527/yamaa/tree/main/benchmarks/adam-adae-partial-dates) | `impute` with a treatment-start bound beside `imputation_flag` reading the completed date; a missing start gives no date and `invalid` answers non-date text |
| [`sdtm-dm-metadata`](https://github.com/elong0527/yamaa/tree/main/benchmarks/sdtm-dm-metadata) | `submission` vs `verifications`: Length becomes both `submission.length` and a `max_length` check |
| [`sdtm-ae-coding`](https://github.com/elong0527/yamaa/tree/main/benchmarks/sdtm-ae-coding) | Coding against MedDRA with a named intermediate matched on its own `key`; several columns read the one record it selects |
| [`schema-inheritance`](https://github.com/elong0527/yamaa/tree/main/benchmarks/schema-inheritance) | Corporate, compound and study layers chained by `parent:` -- real layering instead of copying the template |
| [`schema-inheritance-clear`](https://github.com/elong0527/yamaa/tree/main/benchmarks/schema-inheritance-clear) | A study layer removes one inherited `mapping` entry by naming its path in `clear:` |
