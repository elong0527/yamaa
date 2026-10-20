# Codelists and controlled terminology

A codelist may come from published controlled terminology or from a sponsor's
own definitions. Both use the same list and column-binding fields. Declare
the lists in the project file, and open a specification through that project
to use them.

## Select the sources and versions

The programming lead selects the terminology release when preparing the
specification. Review the published [SDTM terminology](https://evs.nci.nih.gov/ftp1/CDISC/SDTM/SDTM%20Terminology.html)
and [ADaM terminology](https://evs.nci.nih.gov/ftp1/CDISC/ADaM/ADaM%20Terminology.html),
choose the lists needed for the study, and record each selected release in its
source's `standard.version`. These publishing sets can use different versions,
but a Define-XML document cites one release of each: give every source of one
publishing set the same `standard.version`.
Keep sponsor-defined lists in a source that omits `standard`.

| Source | How to declare it |
|---|---|
| Published terminology | Set `standard.name`, `standard.publishing_set`, and `standard.version` once for all lists in that source. |
| Sponsor-defined list | Omit `standard`; supply the list and its items. |
| Sponsor extension of a published list | Keep the published `standard`, set the list's `extensible: true`, and mark added items `extended: true` with no `alias`: an added value has no NCI code. |

The terminology release is separate from `schema_version` and from an
implementation-guide version such as `sdtm.standard.version` (`SDTMIG`).
yamaa reads the declarations supplied by the author; it does not download the
publisher's catalogue or verify that the declared items match that release.
Review the selected values, concept aliases, and extensibility against the
chosen release before using them in a study.

## Declare published and sponsor lists together

Save this as `project.yaml`. The published examples show selected values
and use the illustrative release `2023-12-15`; they are not complete copies of
either publishing set. The age groups are invented sponsor definitions. The
`adam` section lists the specification of the next step, so the project can
open it by its `domain` name.

```yaml
schema_version: "1.0"
codelists:
  - standard:
      name: CDISC/NCI
      publishing_set: SDTM
      version: "2023-12-15"
    codelists:
      - id: SEX
        name: Sex
        data_type: text
        alias: C66731
        items:
          - {value: F, decode: Female, alias: C16576}
          - {value: M, decode: Male, alias: C20197}
          - {value: U, decode: Unknown, alias: C17998}
  - standard:
      name: CDISC/NCI
      publishing_set: ADaM
      version: "2023-12-15"
    codelists:
      - id: DTYPE
        name: Derivation Type
        data_type: text
        alias: C81224
        extensible: true
        items:
          - {value: AVERAGE}
  - codelists:
      - id: AGEGR1
        name: Sponsor Age Group
        data_type: text
        items:
          - {value: "<65"}
          - {value: ">=65"}
adam:
  specs: [spec.yaml]
```

List `id` and `name` must each be unique across all sources. A column binds
the `id`, while `name` is the human-readable description. `alias` records a
published concept identifier and is independent of the binding ID. Define-XML
composition has additional alias requirements described in the
[Define-XML rule](https://github.com/elong0527/yamaa/blob/main/rules/profiles/define-xml.md).

Sources may also live in local files: a project entry such as
`codelists: [sdtm-ct.yaml, adam-ct.yaml, sponsor-codelists.yaml]` reads each file's
`standard` and `codelists` mapping. Relative source paths resolve from the
project file.

## Bind and check a column

Save this minimal synthetic specification as `spec.yaml` beside `project.yaml`
and place the invented input below beside it as `input.csv`. The example only demonstrates terminology
bindings; it is not a complete SDTM or ADaM submission specification.

```yaml
schema_version: "1.0"
domain: EXAMPLE
keys: [ID]
input:
  RAW: input.csv

output:
  path: example.csv
  columns: [ID, SEX, DTYPE, AGEGR1]

columns:
  - name: ID
    type: str
    label: Synthetic identifier
    derivation: RAW.ID
  - name: SEX
    type: str
    label: Sex
    derivation: RAW.SEX
    submission: {codelist: SEX, origin: {type: Assigned}}
  - name: DTYPE
    type: str
    label: Derivation type
    derivation: RAW.DTYPE
    submission: {codelist: DTYPE, origin: {type: Assigned}}
  - name: AGEGR1
    type: str
    label: Sponsor age group
    derivation: RAW.AGEGR1
    submission: {codelist: AGEGR1, origin: {type: Assigned}}
```

```csv
ID,SEX,DTYPE,AGEGR1
SYNTHETIC-001,F,AVERAGE,<65
```

=== "R"

    ```r
    library(yamaa)

    spec <- yamaa_project("project.yaml")$spec("EXAMPLE")
    spec$check()$issues  # Static binding and type checks; no study data is read.
    run <- spec$run()
    run$issues           # Also checks completed column values.
    run$output
    ```

=== "Python"

    ```python
    import yamaa

    spec = yamaa.project("project.yaml").spec("EXAMPLE")
    print(spec.check().issues)  # Static binding and type checks; no study data is read.
    run = spec.run()
    print(run.issues)           # Also checks completed column values.
    print(run.output)
    ```

A codelist binding can check a completed value; it does not translate an input
code. Use the `mapping` derivation when source values need translation before
checking.

| Bound list | What happens to completed column values |
|---|---|
| List with `items`, extensible or not | Every non-missing value must match a declared item; missing values pass. Declare `mandatory: true` in the column's submission metadata if absence must fail. |
| `external` dictionary with a required version | The binding imposes no value check; yamaa does not read the dictionary. Declare `external` instead of `items`. |

In this example, changing `SEX` to `INVALID`, `AGEGR1` to `OTHER` or `DTYPE` to
`OTHER` fails the run's value check. `DTYPE` is extensible, which lets the
study add a value: declare it as an item marked `extended: true`, and the
Define-XML document marks it as an extended value. An added value has no
published concept identifier, so it declares no `alias`, and a submission
document requires none of it. A text list binds to `str`, an integer list to
`int`, and a float list to `float`.

A list with `items` already states its column's value set, so an
`allowed_values` verification on the same column is rejected as a redundant
declaration, even when the two lists agree. Beside an `external` dictionary, or
on a column with no binding, `allowed_values` stays an ordinary check.

See the [project schema](https://github.com/elong0527/yamaa/blob/main/yaml/schema_project.yaml)
for the accepted fields and the [controlled terminology rule](https://github.com/elong0527/yamaa/blob/main/rules/profiles/terminology.md)
for binding, extension, and verification requirements. The
[DM metadata example](https://github.com/elong0527/yamaa/tree/main/benchmarks/sdtm-dm-metadata)
shows versioned terminology sources in a complete Define-XML project.
