---
title: Schema concepts
---

# The schema: class, type, expression, registry

> **YAMAA docs:** [Why](why-yamaa.md) | [Excel to YAMAA](excel-to-yamaa.md) | [Schema concepts](schema-concepts.md) | [Examples walkthrough](yaml-examples-walkthrough.md)

> **Read this if** you are writing or reviewing a specification and want the
> language itself: what the four schema words mean, and every derivation verb
> available.

---

## 1. class, type, expression, registry

These four words are the entry price for reading `schema*.yaml`. R006 defines
all of them.

### 1.1 A class is a header definition

**A class says which fields a mapping may contain and which are required.**

You never write the word `class` in a specification. It appears only in the
schema files, where it decides what you are allowed to write. The relationship
is exactly the one you already know -- **a header row and the rows filled in
under it.**

**`root_class` is the header for the file.** It declares `schema_version`,
`domain`, `datasets`, `base`, `parents`, `record_lookups`, `keys`, `output`,
`columns`, `rows`, `verifications` and `metadata`, of which six are required.
The specification in
[Excel to YAMAA](excel-to-yamaa.md)
is one row written under it, which is why it has
exactly one `domain` and one `keys`.

**`column_class` is the header for one row of the Variable sheet.** Declared
once in `schema.yaml`:

```yaml
column_class:
    - name:   {type: column_name, required: true}
    - type:   {type: column_type, required: true}
    - label:  {type: str, required: false}
    - derivation:    {type: derivation, required: false}
    - verifications: {type: [column_verification, "list[column_verification]"], required: false}
    - metadata:      {type: "dict[str, str]", required: false}
```

and two rows written under it, in a specification:

```yaml
columns:
  - name: BMI                              # a row of column_class
    type: float
    label: Body Mass Index (kg/m2)
    derivation: {compute: {expr: "WEIGHTKG / POWER(HEIGHTCM / 100, 2)"}}

  - name: SEX                              # another row of column_class
    type: str
    label: Sex
    derivation: {source: DM.SEX}
    verifications:
      allowed_values: {values: [M, F, U]}
```

`name` and `type` appear in both rows because the header marks them
`required: true`. The rest are optional, so each row uses what it needs.
Writing `units: mg` in either row fails validation, because the header does not
declare it -- the Excel analogue is adding a `My Note` column that no downstream
program reads, except that Excel ignores it silently and YAMAA rejects it.

Every other class works the same way. These are the ones you meet while writing
a specification:

| class | Where you write one | Read one in |
|---|---|---|
| `output_class` | `output:` | [`adam-adsl-bmi-compute/spec.yaml:8`](https://github.com/elong0527/yamaa/blob/main/yaml/examples/adam-adsl-bmi-compute/spec.yaml#L8) |
| `dataset_class` | each value under `datasets:` | [`adam-adex-cumulative-dose/spec.yaml:5`](https://github.com/elong0527/yamaa/blob/main/yaml/examples/adam-adex-cumulative-dose/spec.yaml#L5) |
| `row_class` | each item of `rows:` | [`adam-adlb-bds/spec.yaml:100`](https://github.com/elong0527/yamaa/blob/main/yaml/examples/adam-adlb-bds/spec.yaml#L100) |
| `record_lookup_class` | each item of `record_lookups:` | [`adam-adae-death-outcome/spec.yaml:10`](https://github.com/elong0527/yamaa/blob/main/yaml/examples/adam-adae-death-outcome/spec.yaml#L10) |
| `record_lookup_between_class` | `record_lookup.between:` | [`adam-advs-analysis-window-table/spec.yaml:16`](https://github.com/elong0527/yamaa/blob/main/yaml/examples/adam-advs-analysis-window-table/spec.yaml#L16) |
| `handled_expression_class` | a `derivation:` that handles failure | [`adam-adsl-mapping/spec.yaml:98`](https://github.com/elong0527/yamaa/blob/main/yaml/examples/adam-adsl-mapping/spec.yaml#L98) |
| `override_rule_class` | each item of `override:` | [`adam-adae-severity-override/spec.yaml:37`](https://github.com/elong0527/yamaa/blob/main/yaml/examples/adam-adae-severity-override/spec.yaml#L37) |
| `source_binding_class` | a `source:` that needs a handler | [`sdtm-dm-basic/spec.yaml:52`](https://github.com/elong0527/yamaa/blob/main/yaml/examples/sdtm-dm-basic/spec.yaml#L52) |
| `multiple_matches_class` | `source.multiple_matches:` | [`adam-adsl-treatment-selection/spec.yaml:31`](https://github.com/elong0527/yamaa/blob/main/yaml/examples/adam-adsl-treatment-selection/spec.yaml#L31) |
| `case_branch_class` | each item of `case.branches:` | [`adam-adae-treatment-emergent/spec.yaml:62`](https://github.com/elong0527/yamaa/blob/main/yaml/examples/adam-adae-treatment-emergent/spec.yaml#L62) |
| `order_term_class` | each item of any `order_by:` | [`adam-adae-severity-rank/spec.yaml:58`](https://github.com/elong0527/yamaa/blob/main/yaml/examples/adam-adae-severity-rank/spec.yaml#L58) |
| `aggregate_class` | a full-form `aggregate:` | [`adam-adlb-mean/spec.yaml:46`](https://github.com/elong0527/yamaa/blob/main/yaml/examples/adam-adlb-mean/spec.yaml#L46) |
| `str_template_class` | a `str_template:` with a handler | [`adam-adsl-identifier-parsing/spec.yaml:55`](https://github.com/elong0527/yamaa/blob/main/yaml/examples/adam-adsl-identifier-parsing/spec.yaml#L55) |

Paths are relative to `yaml/examples/`, and each line number is where that
class is introduced -- the key above it, or the first line of the entry itself
for a list member.

The bundle declares **28 classes** in total. Eight belong to
`schema_environment.yaml` rather than to a specification -- `environment_class`
and the contract, parameter, binding and conformance headers under it, which
[`adam-adsl-bmi-function`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-bmi-function) shows in full. The rest are small headers reached from
the ones above, such as `aggregate_between_class` and the three function
literal forms.

Two of these have a shorthand, which is why you rarely see them written out.
`derivation: {source: X}` is a `handled_expression_class` row carrying only its
required field, and a bare `order_by: [ADT]` entry is an `order_term_class` row
taking both defaults. R006 owns those expansions.

### 1.2 `type` lives in three namespaces

R011 devotes a section to this, because the word appears in three unrelated
roles:

| Role | Written where | Vocabulary |
|---|---|---|
| **Schema descriptor keyword** | inside a descriptor in `schema*.yaml` | `str` `int` `float` `bool` `"null"` `list` `dict` plus named types |
| **Declared column type** | `column.type` in your spec | `str` `int` `float` `date` `datetime` (closed) |
| **Runtime value type** | **never written** | the type a value carries while it is evaluated |

All three appear in the life of one column. Take `AGE`, written as
[`adam-adsl-mapping`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-mapping) writes it.

**Role 1 -- the descriptor keyword.** In `schema.yaml`, `column_class` declares
that a column has a field called `type`, and that whatever you write there must
be a `column_type`:

```yaml
column_class:
    - type: {type: column_type, required: true}
#     ^^^^    ^^^^
#     the field name            the R006 descriptor keyword
```

The two are unrelated words that happen to be spelled the same, and R006 says
so explicitly.

**Role 2 -- the declared column type.** In your specification you fill that
field in with one value from the closed vocabulary:

```yaml
  - name: AGE
    type: int              # <- the declared column type
    derivation:
      value:
        source: DM.AGE
      conversion_failure: null
```

**Role 3 -- the runtime type,** which is never written anywhere. `dm.csv` is a
typeless container, so R014 gives `DM.AGE` the type `str`, and `source` keeps
the type it read. The value flowing out of the expression is therefore the
string `"45"`. Only then does R005's stage 2 convert it to the declared `int`.

### 1.3 An expression is the executable derivation column

Two words that are easy to conflate. **A `derivation` is the column field that
says how a value is produced. An `expression` is the one registered keyword
that produces it.** A derivation is an expression plus what happens when it
goes wrong:

```text
- name: AGE                       <- a row of column_class
  type: int
  derivation:                     <- the field: how this column is produced
    value:                        <-   exactly one expression
      source: DM.AGE              <-     the registered keyword doing the work
    conversion_failure: null      <-   what if the result will not convert
    override: [...]               <-   the final manual correction
```

So what is an expression? **A mapping with exactly one entry**: the key is a
keyword from the registry, the value is that keyword's payload (R006 with
R007). Above, all of `source: DM.AGE` is the expression.

`source` and `literal` are the tree's leaves, and take a scalar payload:

```yaml
source: DM.AGE                 # keyword `source`,  payload: one variable name
literal: DM                    # keyword `literal`, payload: one scalar value
```

Every other keyword takes named parameters:

```yaml
mapping:                       # keyword `mapping`
  source: DM.SEX               #   parameter: which variable to look up
  dict: {M: M, F: F}           #   parameter: the dictionary
  missing: U                   #   parameter: what to use when the input is missing
```

**That second `source` is not the `source` expression.** It is the name of one
of `mapping`'s parameters, and its declared type is `variable` -- a name, not a
nested expression. The registry keyword is always the outermost key, and
nothing below it is an expression unless the nesting table below says so.

This section is about that keyword; 4.4 is about the two layers around it.

A free-text derivation column can say "Map collected sex to M/F/U, otherwise
U". It can equally say "same as SDTM", "see protocol section 9.2", or "ask
stats". YAMAA admits only the verbs in section 2. The trade is
**checkability for portability**: the R and the Python implementation must
produce the same output *and the same error* from the same input.

Across several steps that looks like this. [`adam-adsl-identifier-parsing`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-identifier-parsing) reads
the site out of `USUBJID`, falls back to the collected site, and builds a
display reference:

```yaml
  - name: SITEIDP                        # step 1: parse
    type: str
    derivation:
      str_extract:                       # one keyword
        source: USUBJID
        pattern: '^CATH-([^-]+)-[0-9]{4}$'
        group: 1
        missing: null
        no_match: null

  - name: SITEID                         # step 2: fall back
    type: str
    derivation:
      coalesce:                          # one keyword
        sources: [SITEIDP, DM.SITEID]
        default: UNKNOWN

  - name: SUBJREF                        # step 3: compose
    type: str
    derivation:
      str_template:                      # one keyword
        template: "{SITEID}:{SUBJID}"
        missing: UNKNOWN
```

In an Excel spec this is one Conversion Definition cell reading something like
*"site parsed from USUBJID, else collected SITEID, else UNKNOWN; display as
SITEID:SUBJID"*. Here it is three columns, because each step is one keyword and
**a keyword's operands are named variables, not nested expressions**. The cost
is two extra declarations; what it buys is that `SITEIDP` is a real column, and
this specification publishes it deliberately so a reviewer can see which
subjects fell back.

**Nesting is permitted in exactly three places** (R007's nesting policy):

| Field that nests | Why |
|---|---|
| `case.branches[].then` and `case.otherwise` | Selecting among expressions is what `case` is for |
| `str_concat.sources` | Concatenation puts literals beside sources |
| `override[].value` | A final correction may select any expression |

`case` is the one you write most often:

```yaml
  - name: TRTEMFL
    type: str
    derivation:
      case:
        branches:
          - when: "ASTDT >= TRTSDT AND ASTDT <= TRTEDT"
            then:
              literal: Y                 # <- a nested expression, permitted here
```

Everywhere else, bind the value to a named column and reference the name. The
payoff is that **dependencies are always visible**: nobody has to unfold an
expression tree to see what a column reads.

### 1.4 A derivation has three layers

This is where the "if ... then ..." footnotes of an Excel spec belong.

```yaml
derivation:
  value:                                  # layer 1: the expression from 4.3
    source: RAW.AGE
  conversion_failure: null                # layer 2: conversion to the declared type failed
  override:                               # layer 3: the final manual correction
    - when: "USUBJID = 'SPECIAL-01'"
      value: {literal: 99}
```

Writing `derivation: {source: RAW.AGE}` is shorthand for the same thing with
only `value`.

Each expression additionally carries its own **local handlers** (R008), which
are the small print of an Excel spec:

| Handler | Typical Excel wording | Fires when |
|---|---|---|
| `missing` | "if not collected, set to U" | The input value is missing (or the source variable does not exist) |
| `unmapped` | "if not in codelist, set to 99" | The value is present but the dictionary has no entry |
| `no_match` | "if the pattern does not match, leave blank" | A regular expression found nothing |
| `invalid` | "if the date is not a valid ISO date, leave blank" | The value is present but unusable |
| `multiple_matches` | "if multiple, take the earliest" | Several right-side records matched |
| `conversion_failure` | "if not numeric, leave blank" | Conversion to the declared type failed |
| `override` | "per the data review meeting, subject X is corrected to ..." | A final replacement applies |

**The discipline: omit a handler and its condition is fatal.** Nothing quietly
produces a `.` and a NOTE in the log. This turns silent missing values into
loud failures, which is the single most useful thing to say about handlers.

`missing` and `unmapped` are **disjoint**. `unmapped`, `no_match` and `invalid`
fire only when every input is present, so "not collected" and "collected but
unrecognised" are always two questions and may be answered differently.

### 1.5 A registry is the list of permitted verbs

```yaml
expressions:            # <- a registry
    mapping: ...
    compute: ...
    aggregate: ...
```

Each `schema_expression_*.yaml` contributes entries to the `expressions`
registry, and `schema_derivation.yaml` exposes it as the `expression` type. The
cost of a new verb is **one complete registry entry plus one rule**. That cost
is deliberate; it is what keeps the vocabulary from growing without end.

Two more registries work the same way: `column_verifications` and
`dataset_verifications`.

---

---

## 2. The verb table

### 2.1 Selection and retrieval

| Expression | What it does | Typical Excel wording | SAS / R analogue |
|---|---|---|---|
| `source` | Read a variable, in this dataset or another | Origin = CRF / Predecessor | assignment, or a merge |
| `literal` | A fixed value | Origin = Assigned | `DOMAIN = "DM";` |
| `coalesce` | First non-missing, in order | "use A, else B" | `coalesce()` |
| `greatest` / `least` | Largest or smallest across variables on one row | "the later of X and Y" | `max(of a b)` |
| `case` | Conditional branches | "if ... then ... else ..." | `if / else if` |

### 2.2 Vocabulary mapping

| Expression | What it does | Notes |
|---|---|---|
| `mapping` | Look up an inline dictionary | Has a `case_sensitive` option |
| `mapping_from` | Look up a declared dataset | Keys are declared explicitly; output `keys` are **not** consulted |
| `cut` | Numeric banding | `breaks` plus `labels`; exactly one more label than breaks |

### 2.3 Strings

| Expression | What it does |
|---|---|
| `str_extract` | Return one regular-expression group (e.g. the site inside USUBJID) |
| `str_concat` | Concatenate in order -- **the only string operation that may hold literals inline** |
| `str_template` | Interpolate, as in `"{SITEID}:{SUBJID}"` |
| `str_upper` / `str_lower` | ASCII case conversion; other scalars are unchanged |

R019 keeps language source ASCII while allowing Unicode scalar values in data.
It applies no implicit normalization, compares strings by their exact scalar
sequences, and orders them lexicographically by scalar value.

### 2.4 Arithmetic: one `compute`

```yaml
compute:
  expr: "100 * (AVAL - BASE) / NULLIF(BASE, 0)"
```

R010 closes the grammar: `+ - * /`, parentheses, and exactly thirteen
functions --

`ABS` `CEIL` `FLOOR` `TRUNC` `SQRT` `POWER` `EXP` `LN` `MOD` `GREATEST`
`LEAST` `NULLIF` `COALESCE`

Explicitly **not** permitted: comparison operators, Boolean operators, `CASE`,
aggregate functions, window functions, subqueries, string literals, and
host-language calls.

Two points that always come up:

- **There is no `ROUND`.** R010 says it is absent rather than discouraged:
  rounding is a reporting decision, not a derivation one.
- **There is no `LOG`,** because its base differs between dialects. Write
  `LN(x)`, or `LN(x) / LN(b)`.

### 2.5 Dates (R016 owns both temporal types)

| Expression | What it does | ADaM variable |
|---|---|---|
| `date_diff` | Whole calendar units between two dates; `bounds` declares which endpoints count | AAGE, durations |
| `study_day` | CDISC study day -- the reference date is day 1 and **there is no day zero** | ADY, ASTDY |
| `date_impute` | Complete a truncated ISO date | ASTDT |
| `date_precision` | Report how much of the date the **collected text** carried: `D`, `M` or `Y` | Feeds ASTDTF |

`date_impute` and `date_precision` read the same source; that pairing is the
standard way to derive an imputation flag. See
[example 6](excel-to-yamaa.md#example-6-partial-dates-and-the-imputation-flag).

### 2.6 Window expressions (over constructed output rows)

| Expression | What it does | Variable |
|---|---|---|
| `row_number` | Number rows from 1 within a partition | ASEQ, AESEQ |
| `rank` | The same, but ties share a number (`competition` or `dense`) | Severity ordering |
| `row_value` | Read the value from a row at a given offset in the partition | The previous visit's value |
| `previous_non_missing` | Read the closest strictly earlier non-missing value | Carry a collected result through later planned gaps |
| `baseline_flag` | Flag `Y` on the **unique** latest eligible row at or before a reference date | ABLFL |
| `baseline_value` | Broadcast the flagged row's value to the whole partition | BASE |

A tie for the latest baseline date is an **error** in `baseline_flag`; it does
not pick one.

`previous_non_missing` is ordered propagation within constructed output rows.
It differs from `row_value`, which reads one fixed offset and does not skip
gaps, and from `baseline_value`, which broadcasts one flagged record to the
whole partition. It neither reduces a group like `aggregate` nor reads another
dataset like a record lookup. The current row is not a candidate; coalesce its
source with the earlier result when the artifact should retain a collected
current value.

Ordering carries two rules that are easy to miss (R007):

- `nulls` defaults to `last` and **does not flip with `direction`** -- `last`
  means last under both `asc` and `desc`. SQL engines disagree here, so an
  implementation must apply the declared placement.
- **Non-missing values use the order their type owns**: numeric under R010,
  **code-point sequence** for `str` under R004, chronological for `date` and
  `datetime` under R016. **Host locale collation must not be substituted.** For
  values with accents, mixed case, or non-Latin script, this is what stops R
  and Python from producing two different orders.

### 2.7 Reduction: one `aggregate`

```yaml
aggregate: "SUM(EX.EXDOSE)"           # shorthand
aggregate:                            # full form
  group_by: [STUDYID, USUBJID, PARAMCD]
  filter: "EX.EXDOSE > 0"
  expr: "MEAN(AVAL)"
```

R013 closes the reducer table to seven: `SUM`, `COUNT`, `MIN`, `MAX`, `MEAN`,
`ONLY`, and `COUNT(D.*)` for a record count.

- **`AVG` is not an alias; the portable name is `MEAN`.** A median is not
  registered, because its interpolation rule would have to be fixed first for
  two runtimes to agree.
- **`ONLY(x)` is for "there must be exactly one source record here".** More
  than one fails rather than choosing. It is the executable form of the Excel
  sentence "should be unique per subject".
- **Reductions do not nest.** `MAX(SUM(EX.EXDOSE))` is an error. Two levels of
  summarization means two specifications, with the intermediate grain stored as
  a real artifact.

There is also a grain rule: **every identifier must sit inside a reduction
unless it is a `group_by` column.** `SUM(a) + b` is an error unless `b` is
grouped on, because a value that varies within the group has no single answer.

### 2.8 `function`: the single extension point

This is the part that changed most in v1.0, and R018 defines it completely.
**The spec says what to call; what implements it lives entirely outside the
spec.**

The spec side ([`adam-adsl-bmi-function/spec.yaml`](https://github.com/elong0527/yamaa/blob/main/yaml/examples/adam-adsl-bmi-function/spec.yaml)):

```yaml
  - name: BMI
    type: float
    derivation:
      function:
        name: bmi                  # a logical name, not an R or Python callable
        contract_version: "1.0.0"  # the exact contract required
        args:
          weight_kg: WEIGHTKG      # a bare string is a variable name
          height_cm: HEIGHTCM
          cm_per_m: 100            # a numeric literal
```

The project side -- `environment.yaml` at the project root, validated
**separately** against `schema_environment.yaml`:

```yaml
schema_version: "1.0"
version: "1.0.0"
runtime:
  language: r                                  # one language for the whole project
  artifact:
    reference: org.example/yamaa/bmi-r:1.0.0   # an immutable runtime
    digest: sha256:912ca795a4ab...             # verified before activation
functions:
  bmi:
    contract_version: "1.0.0"          # language-neutral behavior
    implementation_version: "1.0.0"    # this project's binding
    comparison_decimals: 4
    may_return_missing: false
    params:
      - {name: weight_kg, type: float, accepts_missing: false}
      - {name: height_cm, type: float, accepts_missing: false}
      - {name: cm_per_m, type: int, required: false, default: 100, accepts_missing: false}
    returns: float
    binding:
      call: projectbmi::bmi            # a statically written qualified callable
      args: {weight_kg: weight_kg, height_cm: height_cm, cm_per_m: cm_per_m}
    conformance: conformance/bmi.yaml  # vectors that must pass at activation
```

`function` is the company macro library. What R018 adds is a clean separation
between *what the macro is called* and *which version it is, where it lives,
what language it is written in, and whether it was validated*:

| Excel spec plus a macro library | What R018 does |
|---|---|
| A Comment saying `%bmi(wt, ht)` | The spec carries only a logical name and `contract_version` |
| Macro versions managed by SOP and folder naming | `implementation_version` plus a SHA-256 digest of the runtime artifact |
| Resolution depends on `SASAUTOS` | **Only code inside the verified artifact resolves.** A global library, search path, working directory or ambient install is not a fallback |
| Macro validation lives in another Word document | `conformance` vectors that **must all pass before any spec executes** |
| R and SAS mixed in one project | `runtime.language` is `r` or `python`, **project-wide**, never per function |

Semantics worth stating out loud:

- **Signatures are closed and named** -- no positional parameters, no varargs,
  no arbitrary keyword bag. Every optional parameter declares a `default` in
  the environment.
- **`args` cannot nest an expression.** An argument is a variable name; an
  `int`, `float`, `bool` or missing scalar; or one of three explicit literal
  forms: `{literal: text}`, `{date: YYYY-MM-DD}`, `{datetime: ...}`. To pass a
  computed value, declare it as an internal column and pass it by name.
- **`accepts_missing` defaults to `false`:** when such an argument is missing
  the function **is not invoked** and the result is missing. That short circuit
  is not a returned value, so it does not require `may_return_missing: true`.
- **`comparison_decimals` (default 4) is used only for cross-project
  conformance comparison and never mutates a value.** Calculations use the
  unrounded result -- the same position R010 takes on `ROUND`.
- Without a project environment a spec can still be **structurally validated**
  (shape, logical name, contract version, argument leaf forms). Whether the
  contract exists, whether the signature matches, and what it returns are
  deferred until an implementation is supplied -- which makes "write the spec
  first, the code later" a legitimate workflow.

---
