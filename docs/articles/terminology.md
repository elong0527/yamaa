# Codelists and controlled terminology

A codelist may come from published controlled terminology or from a sponsor's
own definitions. Both are written the same way: the lists of one source go in
a file that declares `type: codelists`, the project file lists those files,
and a column binds a list by its `id`. A specification uses the lists only
when it is checked or run through its project.

## Select the sources and versions

The programming lead selects the terminology release when preparing the
specification. Review the published [SDTM terminology](https://evs.nci.nih.gov/ftp1/CDISC/SDTM/SDTM%20Terminology.html)
and [ADaM terminology](https://evs.nci.nih.gov/ftp1/CDISC/ADaM/ADaM%20Terminology.html),
choose the lists the study needs, and record each selected release in its
file's `standard.version`. The publishing sets can use different releases,
but a Define-XML document cites one release of each: give every file of one
publishing set the same `standard.version`. Keep the sponsor's own lists in a
file without `standard`.

| Source | How to declare it |
|---|---|
| Published terminology | A codelists file whose `standard` gives `name`, `publishing_set` and `version` once for all its lists. |
| Sponsor-defined lists | A codelists file without `standard`, listing each list and its items. |
| Sponsor addition to a published list | An item marked `extended: true`, with no `alias`, in a published list marked `extensible: true`. |

The terminology release is separate from the language version in the
`yamaa: "1.0"` header and from the implementation guides a project section
lists in `standards`. yamaa reads the declarations the author supplies; it
does not download the publisher's catalogue or verify that the declared items
match that release. Review the selected values, concept aliases and
extensibility against the chosen release before using them in a study.

## Write one file per source

A codelists file holds one source: an optional `standard` naming its
published terminology, and its `codelists`. The published examples below show
selected values from the illustrative release `2023-12-15`; they are not
complete copies of either publishing set. Save the SDTM terminology as
`codelists/sdtm-ct.yaml`:

```yaml
yamaa: "1.0"
type: codelists
standard:
  name: CDISC/NCI
  publishing_set: SDTM
  version: "2023-12-15"
codelists:
  - id: SEX
    name: Sex
    alias: C66731
    format_name: $SEX
    items:
      - {value: F, decode: Female, alias: C16576}
      - {value: M, decode: Male, alias: C20197}
      - {value: U, decode: Unknown, alias: C17998}
  - id: EPOCH
    name: Epoch
    extensible: true
    alias: C99079
    items:
      - {value: SCREENING, alias: C48262}
      - {value: TREATMENT, alias: C101526}
      - {value: EXTENSION, extended: true}
```

the ADaM terminology as `codelists/adam-ct.yaml`:

```yaml
yamaa: "1.0"
type: codelists
standard:
  name: CDISC/NCI
  publishing_set: ADaM
  version: "2023-12-15"
codelists:
  - id: DTYPE
    name: Derivation Type
    extensible: true
    alias: C81224
    items:
      - {value: AVERAGE, alias: C81209}
```

and the sponsor's own list, whose age groups are invented, as
`codelists/sponsor.yaml`:

```yaml
yamaa: "1.0"
type: codelists
codelists:
  - id: AGEGR1
    name: Sponsor Age Group
    items:
      - {value: "<65"}
      - {value: ">=65"}
```

A list's `id` is what a column binds, and its `name` is the human-readable
description. An `id` is declared once in a file, and a later file may
replace a list by its `id` ([below](#replace-a-shared-list)); a `name` is
unique among the lists the project keeps. A repeated `id` in one file, or a
repeated `name`, fails validation with `duplicate_define_identifier`.
`alias` records the published concept identifier of a list or of one item,
independent of the binding `id`. A Define-XML document under
`context: Submission` requires an `alias` on every `CDISC/NCI` list that
declares items and on each of its items except a sponsor addition, as the
[Define-XML rule](../../rules/profiles/define-xml.md#define-4) describes.

Each list declares either `items` or `external`, never both and never
neither. An `external` list defers to a dictionary and names the version
used, such as `external: {dictionary: MedDRA, version: "26.1"}`, with an
optional `href`. Coded values are unique within a list and of its
`data_type`: `text`, the default, `integer` or `float`. `decode` and `rank`
are declared for every item of a list or for none, and items keep their
declared order in a generated document.

## List the files in the project

Save the project file as `project.yaml`. It lists the three codelists files,
and its `adam` section lists the specification of the next step, so the
project can open it by its `domain` name:

```yaml
yamaa: "1.0"
type: project
codelists:
  - codelists/sdtm-ct.yaml
  - codelists/adam-ct.yaml
  - codelists/sponsor.yaml
adam:
  specs: [spec.yaml]
  standards:
    - {name: ADaMIG, version: "1.3", status: Final}
```

Relative paths resolve from the project file. A source may also be written
inline, as an entry of `codelists` that holds `standard` and `codelists`
itself in place of a path; a file lets several studies share one terminology
release. A project may declare lists that no specification binds: a
Define-XML document carries only the lists its datasets bind, so one set of
files serves the `sdtm` and `adam` sections alike.

## Replace a shared list

The submission packages of one study can share the study's codelists files
and declare only the lists a package changes. The package's project lists the
shared files first, then its own file, and a list in a later file replaces
every earlier list of the same `id`:

```yaml
codelists:
  - ../study/codelists/sponsor.yaml
  - codelists/package.yaml # declares only DCSREAS
```

The replacing list is the later declaration alone, its `name`, items and
decodes included; nothing of the list it replaces is kept. It belongs to its
own file, so it is drawn from that file's `standard`, and a Define-XML
document lists it where that file lists it. Every other list of the shared
files is kept as declared. A `name` stays unique among the lists the project
keeps: a later list that reuses a kept list's `name` under another `id`, such
as a replacement written under a misspelled `id`, fails with
`duplicate_define_identifier`. The
[terminology rule](../../rules/profiles/terminology.md#ct-1) states the
composition.

## Terminology standards and implementation guides

A codelists file's `standard` and a section's `standards` name different
things. The section's `standards` lists the implementation guides its
datasets follow, here ADaMIG 1.3: one or more guides of the section's model,
each named once. Each codelists file's `standard` names the terminology
release its lists are drawn from; terminology is never listed in a section's
`standards`.

`standard.name` takes the closed list a guide's name takes, the eleven
standard names Define-XML 2.1 publishes, in their exact case: `ADaMIG`,
`ADaMIG-MD`, `BIMO`, `CDISC/NCI`, `SDTMIG`, `SDTMIG-AP`, `SDTMIG-MD`,
`SENDIG`, `SENDIG-AR`, `SENDIG-DART` and `SENDIG-GENETOX`. CDISC Controlled
Terminology is `CDISC/NCI`. Any other name, such as `CDISC CT`, fails with
`value_not_permitted` at the source's `standard.name` wherever the project's
codelists are read: before any data, whether or not a document is generated.
`publishing_set` is one of `ADaM`, `CDASH`, `DEFINE-XML`, `SDTM` and `SEND`,
and `version` names the release, such as `2023-12-15`.

A Define-XML document lists both kinds of standard: every guide of the
section's `standards` in declaration order, whether or not a dataset binds
it, then the terminology standard of each source whose lists its datasets
bind. Each dataset is bound to the guide its `submission.standard` names, or
to the first when it names none; a name the section does not list fails with
`unknown_standard`. Each list of a published source names its terminology
standard, and a sponsor list with items is marked `def:IsNonStandard="Yes"`.
Sources that declare the same `name`, `publishing_set` and `version` share
one standard, and a document cites one release per publishing set: when two
sources whose lists one document binds declare one `publishing_set` with a
different `name` or `version`, the later source's `publishing_set` fails with
`duplicate_define_identifier`.

## Bind and check a column

Save this minimal synthetic specification as `spec.yaml` beside
`project.yaml`, and the invented input below beside it as `input.csv`. The
example only demonstrates terminology bindings; it is not a complete ADaM
submission specification.

```yaml
yamaa: "1.0"
type: spec
model: ADaM
domain: EXAMPLE
keys: [ID]
input:
  RAW: input.csv

output:
  path: example.csv
  columns: [ID, SEX, EPOCH, DTYPE, AGEGR1]

columns:
  - name: ID
    type: str
    label: Synthetic Identifier
    derivation: RAW.ID

  - name: SEX
    type: str
    label: Sex
    derivation: RAW.SEX
    submission:
      codelist: SEX
      origins:
        - type: Assigned

  - name: EPOCH
    type: str
    label: Epoch
    derivation: RAW.EPOCH
    submission:
      codelist: EPOCH
      origins:
        - type: Assigned

  - name: DTYPE
    type: str
    label: Derivation Type
    derivation: RAW.DTYPE
    submission:
      codelist: DTYPE
      origins:
        - type: Assigned

  - name: AGEGR1
    type: str
    label: Pooled Age Group 1
    derivation: RAW.AGEGR1
    submission:
      codelist: AGEGR1
      origins:
        - type: Assigned
```

```csv
ID,SEX,EPOCH,DTYPE,AGEGR1
SYNTHETIC-001,F,TREATMENT,AVERAGE,<65
SYNTHETIC-002,M,EXTENSION,AVERAGE,>=65
```

=== "R"

    ```r
    library(yamaa)

    spec <- yamaa_project("project.yaml")$spec("EXAMPLE")
    spec$check()$issues  # Checks the bindings; reads no study data.
    run <- spec$run()
    run$issues           # Also checks the completed values.
    run$output
    ```

=== "Python"

    ```python
    import yamaa

    spec = yamaa.project("project.yaml").spec("EXAMPLE")
    print(spec.check().issues)  # Checks the bindings; reads no study data.
    run = spec.run()
    print(run.issues)           # Also checks the completed values.
    print(run.output)
    ```

=== "Command line"

    ```sh
    yamaa check project.yaml --spec EXAMPLE
    yamaa run project.yaml --spec EXAMPLE    # Also saves example.csv.
    ```

`submission.codelist` carries the list's `id`, not its values, and every
column naming that `id` shares the one declaration. A codelist binding checks
a completed value; it does not translate an input code. Use the `mapping`
derivation when source values need translation before the check.

| Bound list | What happens to completed column values |
|---|---|
| List with `items`, extensible or not | Every non-missing value must equal a declared item; missing values pass. Declare `mandatory: true` in the column's submission metadata if absence must fail. |
| `external` dictionary | The binding imposes no value check; yamaa does not read the dictionary. |

In this example, changing `SEX` to `INVALID`, `DTYPE` to `OTHER` or `AGEGR1`
to `OTHER` fails the run's value check with `allowed_values_failed`, which
names the column, the list, the row's key and the value. A text list binds a
`str` column, an integer list an `int` column and a float list a `float`
column; any other pairing fails validation with `codelist_type_mismatch`.

A list with `items` already states its column's value set, so an
`allowed_values` verification on the same column is rejected as a
`redundant_declaration`, even when the two lists agree. Beside an `external`
dictionary, or on a column with no binding, `allowed_values` stays an
ordinary check. A value-level entry may bind its own list for the rows its
`where` selects, and those rows are checked against that list instead.

## Extend a published list

The published epoch list is extensible, so the study adds the period it
lacks, its open-label `EXTENSION`, as an item marked `extended: true`:

- `extended: true` is admitted only in a list marked `extensible: true` whose
  file declares `standard`. Anywhere else it fails validation with
  `codelist_extension_not_admitted`.
- An added value has no published concept identifier, so it declares no
  `alias`, and one that does fails with `codelist_extended_alias`. A
  Define-XML document requires no alias of it and marks it
  `def:ExtendedValue="Yes"`.
- A published term the study uses is an ordinary item with its `alias`, even
  in an extensible list.

Both declaration checks run wherever the project's codelists are read, in
every context and before any data. An extensible list's declared items, the
additions included, are the whole value set the study admits: the published
list's openness is what lets the study declare an addition, and a value the
study never declared fails like any other. Changing an `EPOCH` in the input
to `RUN-IN` fails the run's value check although the list is extensible.

## Format names

`format_name` is optional and names the SAS format a reader applies to a
list's coded values; a Define-XML document writes it as `SASFormatName`. A
`text` list's format is a character format and begins with `$`, as `$SEX`
does above. An `integer` or `float` list's is a numeric format and does not,
such as `AGEGRN` for numbered age groups. A `text` list whose `format_name`
lacks the `$`, or a numeric list's that begins with one, fails validation with
`value_not_permitted` at that `format_name`. The check runs wherever the
project's codelists are read, before any data, so a run refuses it although
only a Define-XML document carries the name.

## Decode a coded value

A codelist's items pair each coded value with its `decode`, so a derivation
reads a parameter or visit name from the list instead of restating it in a
`mapping`. `decode` returns the `decode` of the item whose `value` equals its
`source`, as SAS `put(PARAMCD, $PARAM.)` does:

```yaml
- name: PARAM
  type: str
  derivation:
    decode:
      source: PARAMCD
      codelist: PARAMCD
```

It reads the list by its `id`, never by its `format_name`, and takes
`mapping`'s answers: a missing source gives `missing`, or missing when none
is declared, and a value no item lists gives `unmapped`, and fails as
`unmapped_value` without it. Before any data is read, the source's type must
be the one the list's `data_type` is held in, `str` for a `text` list, and a
list the project does not declare fails with `unknown_codelist`, one without
decodes, or an `external` one, with `codelist_without_decodes`
([EXPR-15](../../rules/expressions.md#expr-15)). The
[parameter names example](https://github.com/elong0527/yamaa/tree/main/benchmarks/adam-adlb-parameter-names)
decodes a text and an integer list.

## A specification checked without its project

A specification that binds a codelist names something only its project
supplies. Checked or run on its own, it fails validation at each binding,
before any input is read:

=== "R"

    ```r
    yamaa_spec("spec.yaml")$check()$issues
    ```

=== "Python"

    ```python
    print(yamaa.spec("spec.yaml").check().issues)
    ```

=== "Command line"

    ```sh
    yamaa check spec.yaml
    ```

Each of the four bound columns gives one `project_binding_missing` issue
([CT-13](../../rules/profiles/terminology.md#ct-13)) at its binding, such as
`columns.SEX.submission.codelist`, with the message
`the codelist at columns.SEX.submission.codelist needs a project file, and none is given`.
Its `context` names the `binding`, here `codelist`, the `column` and the
`codelist`. `run()` reports the same issues and builds nothing. A column that
declares `inventory_vocabulary: true` and a call to a project function fail
the same way, since only a project file supplies them. Through its project,
as above, each binding is checked in full, and one naming a list the project
does not declare fails with `unknown_codelist`.

`yamaa_schema("codelists")`, `yamaa.schema("codelists")` or
`yamaa schema codelists` gives the codelists file's JSON Schema, which an
editor can use to check a file as it is typed. See the
[terminology schema](../../yaml/schema_terminology.yaml) for the fields a
codelists file or an inline source accepts and the
[controlled terminology rule](../../rules/profiles/terminology.md) for
binding, extension and verification requirements. The
[DM metadata example](https://github.com/elong0527/yamaa/tree/main/benchmarks/sdtm-dm-metadata)
shows versioned terminology sources in a complete Define-XML project, and
the [sponsor epoch example](https://github.com/elong0527/yamaa/tree/main/benchmarks/sdtm-ta-sponsor-epoch)
an extended item in its document.
