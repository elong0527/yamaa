# Schema introduction

The schema says what a yamaa file may contain. A field it does not declare,
or a value its type does not admit, fails before any data is read. The schema
owns each field's shape, kind, default and structural constraints; the
[rules](rules.md) own what each written item means.

## Four file types

Every yamaa file is YAML whose first two fields are its header: `yamaa`, the
oldest language version the file needs, always quoted, and `type`, which names
one of four file types. Each type has a schema of its own
([Schema files and JSON Schemas](schema.md)).

| `type` | One file holds |
|---|---|
| `spec` | One dataset: its inputs, keys, rows, columns, checks and output. A specification may inherit from a parent and include fragments, which are `spec` files too. |
| `project` | A study: the specifications of each section, `sdtm`, `adam` or `send`, the function and codelists files they share, the lock of each host language, and the metadata Define-XML needs. |
| `codelists` | The codelists of one terminology source ([Codelists and terminology](terminology.md)). |
| `function` | One project function: its parameters, return type and tests, and the callable that implements it in each host language. |

This specification builds a small ADSL:

```yaml
yamaa: "1.0"
type: spec
model: ADaM
domain: ADSL
keys: [STUDYID, USUBJID]
input:
  DM: input/dm.csv

output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, AGE, ADULTFL]

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: DM.STUDYID

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: DM.USUBJID

  - name: AGE
    type: int
    label: Age
    derivation: DM.AGE

  - name: ADULTFL
    type: str
    label: Adult Flag
    derivation:
      flag:
        condition: AGE >= 18
        false_value: N
        missing: N
```

An engine of language version 1.M reads a file that declares 1.M or an older
1.x version, and refuses a newer one as `unsupported_language_version`,
naming the version the file needs
([SCHEMA-2](../../rules/reference/schema-language.md#schema-2)). The version
is text: an unquoted `1.0` is a YAML number and fails as
`invalid_field_type`. A field or a listed value added after 1.0 names the
version that introduced it, and a file that declares an older version cannot
write it: there it fails as `unknown_field` or `value_not_permitted`, naming
that version. Every field of the 1.0 schema exists from 1.0.

A field that names another file checks that file's header too. A `parent`,
an `include` entry and an input's `schema` name specifications, and a
project's `functions`, `codelists` and section `specs` name function,
codelists and specification files. A named file of another type fails there
as `unexpected_file_type`, and one of a version the engine does not read as
`unsupported_language_version`. Each layer of a specification, a fragment
included, is checked at the version it declares, and the composed
specification needs the newest version any layer declares
([LAYER-4](../../rules/inheritance.md#layer-4)).

## Field kinds

Every field has one kind, which says what a bare scalar written in it is. The
schema gives each field its kind, and every field of a JSON Schema carries it
as `x-yamaa-kind`.

| Kind | A bare scalar in the field is |
|---|---|
| `expression` | One expression of the closed grammar: a bare name reads a variable, quoted text such as `'DM'` is text, and a YAML number, Boolean or null is a literal ([EXPR-5](../../rules/expressions.md#expr-5)). |
| `name` | One variable name, bare for an output column or qualified, as in `DM.AGE`; never an expression and never text ([NAME-5](../../rules/names.md#name-5)). |
| `identifier` | A declared name, such as an input dataset, a column or a named intermediate, or a reference to one. |
| `value` | A constant: text is text, and YAML null is the missing value where the field's type admits it. |
| `path` | A local path. A relative one is relative to the file that writes it, unless `relative_to: entry` selects the entry file ([FILE-2](../../rules/reference/files.md#file-2)) or the path is a link that Define-XML writes, which is relative to the generated document. |
| `structure` | Not a scalar of its own: the field holds a class, a list or mapping of classes, or a verb or verification. |

In the specification above, each `derivation` and the flag's `condition` are
expression fields, so `DM.AGE` and `AGE` read variables, while `false_value`
and `missing` are value fields, so `N` is the text N; the true value takes
its default, `Y`. An expression field spells the same text quoted, as in
`derivation: "'N'"`, and a forgotten quote fails as an unknown name instead
of becoming text. A short form stands for its long form with one field filled,
and its scalar is read as that field: an input written as its path fills its
`path`, and `flag: AGE >= 18` fills the flag's `condition`.

YAML null is the missing value wherever a value or an expression may be
missing, and it never removes anything: `unmapped: null` answers a value its
mapping does not list with missing, where leaving `unmapped` out fails the
run. An identifier, name, path or structure field does not admit null, nor
does a Boolean, a number or a field of listed values: there null fails as
`invalid_field_type`, and a field is left absent by leaving it out
([LAYER-9](../../rules/inheritance.md#layer-9)). In an expression field,
missing and a number may also be written as grammar text, such as `"NULL"`
or `"5"`; the YAML scalar is canonical, and the lint reports the text as
`scalar_as_text` ([Style guide](style.md)).

The [table at the end of this page](#every-fields-kind) gives every field's
kind, and [Schema fields](../reference/schema-fields.md), generated from the
schema files, gives each field's type, default and rule as well.

## The derivation vocabulary

A derivation holds one expression: text of the closed grammar, a YAML number,
Boolean or null, or a mapping that holds one verb of the closed `expressions`
registry ([EXPR-1](../../rules/expressions.md#expr-1),
[EXPR-5](../../rules/expressions.md#expr-5)). Text covers what
needs no verb: a reference such as `DM.AGE`, quoted text, a temporal literal
such as `DATE '2024-03-10'`, `NULL`, and arithmetic with the numeric
functions ([FORMULA-27](../../rules/formulas.md#formula-27)). The verbs live
in one schema module per family:

| Family | Module | Verbs |
|---|---|---|
| Core selection | `schema_expression_core.yaml` | `source`, `first_available`, `greatest`, `least`, `case`, `flag` |
| Vocabulary | `schema_expression_mapping.yaml` | `mapping`, `decode`, `cut` |
| Strings | `schema_expression_str.yaml` | `str_pad`, `str_substr`, `str_length`, `str_extract`, `str_replace`, `str_template`, `str_case` |
| Rounding | `schema_expression_numeric.yaml` | `round_half_away_from_zero`, `str_decimal` |
| Aggregation | `schema_expression_aggregate.yaml` | `aggregate` |
| Dates | `schema_expression_date.yaml` | `diff`, `impute`, `precision`, `imputation_flag`, `to_date`, `date_text`, `study_day`, `to_epoch_day`, `dtc`, `time_of_day`, `shift`, `duration_value`, `duration_text` |
| Windows | `schema_expression_window.yaml` | `row_number`, `rank`, `row_value`, `locf`, `baseline_flag` |
| Collected data | `schema_expression_odm.yaml` | `odm` |
| Extension | `schema_expression_function.yaml` | `function` |

A verb's operand, such as the `source` of `mapping`, is a name field that
holds one variable, never a nested expression, so operations compose through
named columns: derive a value into a column, then read the column
([NAME-5](../../rules/names.md#name-5)). Expressions nest only in `case`
results, in key match values and in a `nearest` selection's `to`, which is
read as a match value is ([EXPR-3](../../rules/expressions.md#expr-3),
[LOOKUP-26](../../rules/lookup.md#lookup-26)).

## Layers

A specification may name a parent, a `spec` file it inherits from, which may
name a parent of its own, up to a root layer that names none. Any layer may
also list fragments under `include`: `spec` files holding declarations that
several specifications share, such as a column derived the same way in two
datasets. At every layer the order is the same: its parent chain, then its
fragments from left to right, then its own fields, each contribution taking
precedence over the ones before it
([LAYER-3](../../rules/inheritance.md#layer-3)).

A layer need not be complete. Each layer is checked for what it writes, and
the composed specification for required fields. A root field replaces what
came before it whole, while `input`, `intermediates`, `columns`, `rows` and
`windows` compose member by member, matched by their key, `name` or `id`,
and `output` and `submission` compose field by field, so a layer that changes
only `output.columns` or `submission.artifact` writes that field alone
([LAYER-5](../../rules/inheritance.md#layer-5)). This study entry inherits
an adverse-event template and includes two fragments:

```yaml
yamaa: "1.0"
type: spec
model: ADaM
parent: ../templates/ae.yaml
include:
  - ../fragments/duration.yaml
  - ../fragments/severity.yaml
domain: ADAE
keys: [USUBJID, AESEQ]

output:
  path: adae.csv
  columns: [USUBJID, AESEQ, ASTDT, AENDT, ADURN, ASEVN]
```

A fragment declares neither `parent` nor `include`, so it composes on
whichever layer includes it; one that does fails as `prohibited_construct`
([LAYER-4](../../rules/inheritance.md#layer-4)). A relative path a fragment
writes stays relative to the fragment
([LAYER-10](../../rules/inheritance.md#layer-10)). A file contributes once or
not at all: an `include` entry that reaches a file the composition already
holds fails as `duplicate_layer`, and one that reaches the layer listing it,
or a layer below that one on the chain, as `inheritance_cycle`
([LAYER-3](../../rules/inheritance.md#layer-3)). The duration fragment
derives one column from two that the template declares:

```yaml
yamaa: "1.0"
type: spec

columns:
  - name: ADURN
    type: int
    label: Analysis Duration (N)
    derivation:
      diff:
        type: date
        start: ASTDT
        end: AENDT
        unit: day
        closed: both
```

[Excel to yamaa](excel.md#a-fragment-shared-by-several-templates) uses one
fragment on two templates.

A layer removes an inherited value only by listing its spec path in `clear`.
Composition removes each listed value before the layer's own fields compose,
and a field with a default takes the default again
([LAYER-9](../../rules/inheritance.md#layer-9)). This layer drops a
column's inherited checks and one entry of an inherited dictionary:

```yaml
yamaa: "1.0"
type: spec
parent: adsl-template.yaml
clear:
  - columns.AGE.verifications
  - columns.RACEN.derivation.mapping.dict['NOT REPORTED']
```

A path names a column by its `name`, an input or a window by its key, and a
named intermediate or a row template by its `id`; it writes a key with other
characters than letters, digits, `_` and `-` quoted in brackets, as in
`['NOT REPORTED']`, and holds no list index, since a list replaces whole. A
path that names nothing the layer inherits, a required or identity field,
`yamaa`, `type` or a whole member fails as `invalid_clear`.

A project file layers the same way: it names one `parent` project file and
may `clear` what it inherits, `study`, `environments` and each section
composing field by field, and it includes no fragment
([PROJECT-16](../../rules/profiles/project.md#project-16)).

## Column order

A single file keeps its columns in the order they are written, and that order
must be a dependency order: each column reads only columns written before it
([COLUMN-5](../../rules/columns.md#column-5)). A layered specification, one
that names a parent or includes a fragment, is sorted instead: each column
comes after every column it reads, and of the columns that may come next, the
one contributed earliest comes first
([LAYER-13](../../rules/inheritance.md#layer-13)). Layers cannot place a
column between another layer's columns, so composition orders them.
`output.columns` is never reordered: it alone sets which columns the output
holds, and in what order.

## Every field's kind

Each row lists, by kind, the fields of one place in a file. A field written
in its short form is listed under the field it fills.

**A specification:**

| Where | `expression` | `name` | `identifier` | `value` | `path` | `structure` |
|---|---|---|---|---|---|---|
| Root | `filter` | | `domain`, `keys`, `base` | `yamaa`, `type`, `model`, `clear`, `metadata` | `parent`, `include` | `input`, `output`, `windows`, `intermediates`, `columns`, `rows`, `verifications`, `submission` |
| An input written in full, `input.<name>` | | | `ordinal`, `key` | `relative_to`, `types`, `encoding`, `byte_order_mark`, `empty_string`, `require_match` | `path`, `schema` | |
| `output` | | `order_by` | `columns` | `relative_to`, `decimals` | `path`, `report` | |
| An order term written in full | | `variable` | | `direction`, `nulls` | | |
| A window: `windows.<name>`, or a verb's `window` written in full | `filter` | `group_by`, `order_by` | | | | |
| A named intermediate | `key`, `filter`, `derivations` | `order_by` | `id`, `dataset`, `inner_join`, `columns` | `keep`, `absent`, `require_match` | | `between`, `nearest`, `verifications` |
| `between`, in a named intermediate, `source` or `aggregate` | | `value` | `lower`, `upper` | | | |
| `nearest`, in a named intermediate | `to` | | `source` | `unit` | | |
| A named intermediate's derivation written in full | `derivation` | | | `type`, `unconvertible` | | |
| A column | `derivation` | | `name` | `type`, `label`, `metadata` | | `verifications`, `submission` |
| A derivation written in full, in a column, a row template or an `aggregate` derivation | `value` | | | `unconvertible` | | |
| A copy entry in `columns` | | | | | | `copy` |
| `copy` | | | `dataset`, `columns` | | | `submission` |
| `copy.submission` | | | | `core`, `mandatory`, `role` | | |
| A row template | `filter`, `derivations` | `group_by` | `id`, `dataset` | | | |

**The verbs**, which have no path fields:

| Verb | `expression` | `name` | `identifier` | `value` | `structure` |
|---|---|---|---|---|---|
| `source` | `filter` | `variable`, `order_by` | | `keep`, `absent`, `require_match` | `between` |
| `first_available` | | `sources` | | `missing` | |
| `greatest`, `least` | | `sources` | | | |
| `case`, in each item | `when`, `then`, `otherwise` | | | | |
| `flag` | `condition` | | | `true_value`, `false_value`, `missing` | |
| `mapping` | | `source` | | `dict`, `case_sensitive`, `missing`, `unmapped` | |
| `decode` | | `source` | `codelist` | `missing`, `unmapped` | |
| `cut` | | `source` | | `breaks`, `labels`, `closed`, `missing` | |
| `str_pad` | | `source` | | `width` | |
| `str_substr` | | `source` | | `start`, `length`, `piece`, `missing` | |
| `str_length` | | `source` | | `missing` | |
| `str_extract` | | `source` | | `pattern`, `group`, `missing`, `no_match` | |
| `str_replace` | | `source` | | `pattern`, `replacement`, `missing` | |
| `str_template` | `template` | | | `missing` | |
| `str_case` | | `source` | | `to`, `delimiters`, `missing` | |
| `round_half_away_from_zero` | | `source` | | `digits` | |
| `str_decimal` | | `source` | | `decimals`, `width`, `missing` | |
| `aggregate` | `key`, `filter`, `expr` | `group_by`, `order_by` | | `separator`, `distinct`, `quantile` | `between`, `derivations` |
| An `aggregate` derivation | `derivation` | | | `type` | |
| `diff` | | `start`, `end` | | `type`, `unit`, `closed` | |
| `impute` | | `source`, `not_before`, `not_after`, `fallback` | | `type`, `month`, `day`, `time`, `invalid` | |
| `precision` | | `source` | | `type`, `invalid` | |
| `imputation_flag` | | `source` | | `part`, `seconds_collected` | |
| `to_date`, `to_epoch_day`, `time_of_day` | | `source` | | | |
| `date_text` | | `source` | | `forms`, `invalid` | |
| `study_day` | | `source`, `reference` | | | |
| `dtc` | | `year`, `month`, `day`, `hour`, `minute`, `second` | | `invalid` | |
| `shift` | | `source`, `days`, `amount` | | `type`, `unit` | |
| `duration_value` | | `source` | | `unit`, `invalid` | |
| `duration_text` | | `source` | | `unit` | |
| `row_number` | | | | | `window` |
| `rank` | | | | `method` | `window` |
| `row_value` | | `source` | | `offset` | `window` |
| `locf` | | `source` | | `include_current` | `window` |
| `baseline_flag` | | `date`, `reference`, `source` | | `inclusive` | `window` |
| `odm` | `filter` | | `item`, `event`, `form`, `item_group` | | |
| `function` | `args` | | `name` | | |

**The verifications:** a column's `not_missing`, `allowed_values`, `range`,
`max_length` and `matches`, the dataset's `unique`, `all_or_none`, `assert`
and `row_count`, and a named intermediate's `unique`.

| Verification | `expression` | `name` | `identifier` | `value` |
|---|---|---|---|---|
| Every verification | | | `id` | `severity` |
| `not_missing` | `when` | | | `max_fraction` |
| `allowed_values` | | | | `values` |
| `range` | | | | `min`, `max` |
| `max_length` | | | | `max` |
| `matches` | | | | `pattern` |
| `unique`, `all_or_none` | | `columns` | | |
| `assert` | `when`, `require` | | | |
| `row_count` | `filter`, `when` | `group_by` | | `min`, `max`, `min_fraction`, `max_fraction` |

**Submission metadata**, which has no expression or name fields:

| Where | `identifier` | `value` | `path` | `structure` |
|---|---|---|---|---|
| The root's `submission` | `domain`, `standard` | `mode`, `has_no_data`, `label`, `class`, `subclass`, `structure`, `repeating`, `reference_data`, `non_standard`, `comment` | `artifact` | |
| A column's `submission` | `codelist` | `key_sequence`, `core`, `mandatory`, `role`, `data_type`, `length`, `significant_digits`, `display_format`, `inventory_vocabulary`, `method`, `comment` | | `origins`, `value_level` |
| A `value_level` entry | `id`, `codelist` | `where`, `label`, `key_sequence`, `core`, `mandatory`, `role`, `data_type`, `length`, `significant_digits`, `display_format`, `inventory_vocabulary`, `method`, `comment` | | `origins` |
| A `where` condition written in full | | `comparator`, `values` | | |
| An `origins` entry | `documents` | `type`, `source`, `description` | | |
| A `method` written in full | `documents` | `name`, `type`, `description` | | `expression` |
| `method.expression` | | `context`, `code` | | |
| A `comment` written in full | `documents` | `text` | | |
| A document reference written in full | `document` | | | `pages` |
| `pages` | | `type`, `refs`, `title` | | |

**Project, codelists and function files**, which have no expression or name
fields:

| Where | `identifier` | `value` | `path` | `structure` |
|---|---|---|---|---|
| A project's root | | `yamaa`, `type`, `clear` | `parent`, `functions` | `environments`, `codelists`, `study`, `sdtm`, `adam`, `send` |
| `environments` | | | | `python`, `r` |
| `environments.python`, `environments.r` | | | `lock` | |
| `study` | `id` | `name`, `description`, `protocol_name` | | |
| A section, `sdtm`, `adam` or `send` | | `define_version`, `transport`, `context`, `xml_lang`, `file_oid`, `originator`, `source_system_version` | `specs`, `define`, `stylesheet` | `standards`, `documents`, `metadata_version` |
| A `standards` entry | | `name`, `version`, `status` | | |
| A `documents` entry | `id` | `kind`, `title` | `link` | |
| `metadata_version` | `id` | `name`, `description` | | |
| A codelists file's root | | `yamaa`, `type` | | `standard`, `codelists` |
| An inline codelist source in a project's `codelists` | | | | `standard`, `codelists` |
| `standard` | | `name`, `publishing_set`, `version` | | |
| A codelist | `id` | `name`, `data_type`, `extensible`, `alias`, `format_name` | | `items`, `external` |
| An `items` entry | | `value`, `decode`, `rank`, `alias`, `extended` | | |
| `external` | | `dictionary`, `version`, `href` | | |
| A function file's root | `name`, `implementations` | `yamaa`, `type`, `description`, `returns`, `may_return_missing`, `comparison_decimals` | | `params`, `tests` |
| A `params` entry | `name` | `type`, `required`, `default`, `accepts_missing` | | |
| A `tests` entry | `id`, `covers` | `args`, `result` | | |

## What the schema does not do

The schema never defines behavior. It says that `cut` takes `source`,
`breaks`, `labels`, `closed` and `missing`, and which of them are values; the
[rules](rules.md) say what `cut` computes and when it fails. A schema
description links to the rule that owns its field and adds no meaning of its
own.

That split is the whole reading order: the schema tells you what you can
write, the rules tell you what it means, and `benchmarks/` shows a runnable
specification that proves both.
