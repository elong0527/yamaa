# Card text beyond D78

Part of the [1.0 interface RFC](../interface-1.0.md). The owner approved the
first card page, [Card text beyond D71](card-text.md), as D77, and the 46
one-line swaps of its last section as D78. D78 asks for a second page for the
twelve cards of that section whose 1.0 title or statement needs real drafting,
and for a new card for the ODM source profile (D26, P-ODMIN-1). This page
drafts them for the owner's approval (#3193), in the first page's format.
Nothing here changes a card: each Phase 1 pull request writes its cards, and
their detail and evidence get ordinary review.

Every proposed title is one sentence without a final period, and every
statement has at most 90 words, counted as `tools/rules/model.py` counts them;
each card gives its counts. Today's text is quoted from `rules/` on main at
`6429f127c`, with links shown as their text and rule IDs written without
links. Today's wording stays wherever no decision requires a change; where the
90-word limit forces a cut, the card says what went. A new card is named by its
file, topic and the next free number of its series, because a rule ID cannot be
cited before its card exists.

The cards describe the 1.0 surface. Under D75, from P1-3 until P2-14
`rules/README.md` carries one transitional sentence: a file that declares
`schema_version` is read under the rules at the `yamaa-v0.5.0` tag, and until
P2-13 a card keeps on its Fails line the conditions the 0.5 path still raises.
Each card below gives its 1.0 Fails line, then what it keeps until P2-13.

Drafting SCHEMA-1 found one more card whose statement 1.0 changes and that
neither page lists, SCHEMA-11; it is drafted in the
[last section](#a-card-neither-page-lists).

## Summary

| Card | Changes | What changes | Pull request |
| --- | --- | --- | --- |
| FORMULA-27 | statement | text in an expression field replaces `compute.expr`, adding quoted text and temporal literals (D18, P-KIND-1) | P1-2, #3163 |
| FORMULA-1 | statement | expression fields written as text replace `compute.expr` (D18) | P1-2 |
| FORMULA-9 | statement | expression-field text resolves names as `compute.expr` did, and text holding only a name is a reference (D18) | P1-2 |
| FORMULA-12 | statement | the root of `compute.expr` leaves the operand rule (D18) | P1-2 |
| SCHEMA-1 | statement | the four 1.0 entry points, selected by `type`, replace `schema.yaml` and `schema_environment.yaml` (D4, D8, P-TYPE-1) | P1-3 |
| EXPR-3 | statement | text, YAML literals and the structured `source` are the leaves; `literal` and `numeric_expression` go (D18) | P1-5 |
| EXPR-4 | title, statement | a reference or a structured `source` copies, and a literal returns its value (D18) | P1-5 |
| COLUMN-11 | title, statement | `absent` answers only a lookup with no record; a variable that does not exist fails (D67, P-HND-1) | P1-5 |
| DEFINE-6 | title, statement | a `standards` list of the section's model, each dataset bound to the guide it selects, the first by default (D16, D25, P-STD-1) | P1-13 |
| SUB-12 | statement | `NE` and `NOTIN` select a row holding none of the listed values, missing included (D66, P-VLM-1) | P1-14 |
| New ODM card | new card | the ODM source profile: snapshots read, transactional content and documents without clinical data refused, audit records skipped (D26, D73, P-ODMIN-1) | P1-15 |
| NAME-8 | statement | an `.xml` input's fields come from the ODM source profile (D26) | P1-15 |
| CSV-10 | title, statement | a declared byte-order mark is skipped (D15); its spelling is a question | the pull request for #3157 |
| SCHEMA-11 | statement | `kind` and `since` join the descriptor keywords (D31, P-VER-1) | P1-3 |

The first page planned FORMULA-1, FORMULA-9 and FORMULA-12 for P1-2 with
FORMULA-27. This page keeps that plan: the four cards name `compute.expr`
together, and they change with the grammar's new context.

## Questions for the owner

Each question names its recommendation, and the proposed text follows it.

1. **FORMULA-27: `TRUE` and `FALSE` in an expression field's text (Q1).** The
   RFC gives the derivation context as the constructs of `compute` plus quoted
   text, temporal literals and `NULL`, without comparisons or reducers
   (P-KIND-1), which leaves out `TRUE` and `FALSE`; its next paragraph says
   that missing, numbers and Booleans each have two spellings in an expression
   field, which needs `"TRUE"` admitted. FORMULA-3 makes `TRUE` and `FALSE`
   truth values, and FORMULA-1 refuses a truth value where a value belongs.
   **Recommendation:** refuse them there as `prohibited_construct` naming
   `boolean`, as `compute` does today. A Boolean in an expression field is
   then written only as YAML `true` or `false`, FORMULA-1 and FORMULA-3 stay as
   they are, and the lint finding `scalar_as_text` covers missing and numbers.
   The alternative admits them as Boolean values: FORMULA-27's third sentence
   ends "plus every other literal" (83 words), and FORMULA-3's statement changes
   too.
2. **The FORMULA cards and D75 (Q2).** Their 1.0 statements drop
   `compute.expr`, which a file declaring `schema_version` keeps until P2-13.
   D75's transitional sentence covers that file from P1-3, and the plan does
   not order P1-2 and P1-3. **Recommendation:** P1-2 merges after P1-3. The
   alternative is for P1-2 to add D75's sentence itself if it merges first.
3. **SCHEMA-1's pull request (Q3).** The first page left it to P1-3, or to
   P2-13 when `yaml/v1/` moves up. **Recommendation:** P1-3, where `type` starts
   selecting a 1.0 bundle and SCHEMA-2's closing sentence moves into SCHEMA-1's
   detail; the detail says where the bundles sit until P2-13. Waiting for P2-13
   would leave SCHEMA-1 naming the environment file through Phase 2, after
   P1-8 removes it from the 1.0 surface.
4. **DEFINE-6: a repeated standard name, and an empty list (Q4).** P-STD-1
   makes standard names unique within a section, so generated OIDs stay
   `STD.<name>`, and D16 makes the first entry the default; no condition is
   approved for either fault. **Recommendation:** two entries with one `name`
   fail as `duplicate_identifier` at the second, as a naming list does
   (SCHEMA-8), and an empty `standards` fails as DEFINE-28's
   `missing_required_field` when Define-XML composes the section, as an
   omitted one does. The alternative is `value_not_permitted` for both.
5. **CSV-10: the byte-order-mark declaration (Q5, #3157).** D15 admits a
   declared mark, and the approved rename table has no field for it.
   **Recommendation:** an optional Boolean `byte_order_mark: true` on an input
   the csv profile reads. When it is declared, the source must begin with one
   mark, which is skipped, and a source without one fails as
   `source_byte_order_mark`, so the declaration, never the file, decides; on an
   input of another profile the declaration fails as `value_not_permitted`. The
   field is an addition to the rename table (D27). The alternatives are a
   CSV-only `encoding` value, which P-UNU-1 rules out by keeping `encoding`
   SAS-only (SAS-2), and a declaration that admits a mark without requiring
   one.
6. **The ODM card's file (Q6).** The allocation rules put format rules in
   `rules/reference/`, one file per source format: CSV, Parquet and SAS.
   **Recommendation:** a new file `rules/reference/odm.md`, "ODM source
   profile", with the prefix `ODM`, whose first card is number 1 of that series
   under the topic Reading. The alternatives keep an existing series: number 12
   of the INPUT series beside INPUT-1 in `rules/inputs.md`, or number 14 of the
   NAME series beside NAME-8 in `rules/names.md`; neither file holds a format's
   reading rules today.
7. **SCHEMA-11, which neither page lists (Q7).** Its closed keyword list lacks
   `kind`, which `yaml/v1/` declares on its named value types (D31), and
   `since`, which P1-3 adds for the version rule (P-VER-1).
   **Recommendation:** approve the text in the
   [last section](#a-card-neither-page-lists), written by P1-3.

## P1-2: the derivation context

### FORMULA-27

`rules/formulas.md`; changes the statement; the title stays.

Today (title 15 words, statement 84 words):

> The field an expression is written in fixes its root and the constructs it
> admits
>
> A predicate has a truth-valued root and admits comparisons, Boolean logic,
> every literal, `str_contains`, and arithmetic and the numeric functions in
> an operand. `compute.expr` has a numeric root over numbers, `NULL`, names,
> arithmetic and the numeric functions. `aggregate.expr` admits what
> `compute.expr` admits and the reducers. A reduction is refused outside
> `aggregate.expr`, and a refused construct fails wherever it is written. A
> call of a function or reducer its field does not admit fails, as does an
> admitted function called with another number of arguments.

Proposed (title unchanged, statement 85 words):

> The field an expression is written in fixes its root and the constructs it
> admits
>
> A predicate has a truth-valued root and admits comparisons, Boolean logic,
> every literal, `str_contains`, and arithmetic and the numeric functions in
> an operand. `aggregate.expr` admits numbers, `NULL`, names, arithmetic, the
> numeric functions and the reducers. Text in an expression field (EXPR-5)
> admits the same except the reducers, plus quoted text and temporal literals.
> A refused construct fails wherever it is written. A call of a function or
> reducer its field does not admit fails, as does an admitted function called
> with another number of arguments.

- **Follows:** D18, D31; P-KIND-1 (D58), whose derivation field holds the
  constructs of `compute` plus quoted text, temporal literals and `NULL`,
  without comparisons or reducers.
- **Fails:** `prohibited_construct`, `prohibited_function`, as today.
- **Title:** it stays: the field still fixes the root, a truth value in a
  predicate and any value in an expression field.
- In the statement, expression-field text takes the place of `compute.expr`,
  with the literals D18 moves into the grammar, and `aggregate.expr` keeps what
  it admits today, now listed instead of named after `compute.expr`. "A
  reduction is refused outside `aggregate.expr`" goes to stay within 90 words:
  the predicate and expression-field lists already leave the reducers out, and
  the table keeps the refusal. `TRUE` and `FALSE` stay refused there (Q1).
- **Detail:** the table's `compute.expr` row becomes the expression-field row:
  `derivation`, a `derivations` entry, `then`, `otherwise`, a handled wrapper's
  `value` and a key match value, any value as root; it refuses comparisons,
  `IN`, `BETWEEN`, `LIKE`, `IS NULL`, `AND`, `OR`, `NOT`, `TRUE`, `FALSE`,
  `str_contains` and a reduction, and a function argument admits one leaf
  (FUNC-6). The `aggregate.expr` row keeps refusing quoted text and temporal
  literals, which `prohibited_construct` names `string` and `temporal`.
  "The numeric semantics of `compute`" become those of expression-field text,
  and the closing paragraph says that text holding only a name or a literal
  returns it (EXPR-4), and other text one value per current row.
- **Evidence:** `negative-formula-flag` and `negative-row-aggregate`, converted
  to `derivation: "AVAL > ANRHI"` and `derivation: "SUM(AVAL)"`, keep
  `prohibited_construct` and `prohibited_function`. The grammar file gains a
  `derivation` context with vectors both hosts replay; its `compute` context
  stays for the 0.5 path until P2-13.
- **Written by:** P1-2 (#3163), after P1-3 (Q2).

### FORMULA-1

`rules/formulas.md`; changes the statement; the title stays.

Today (title 10 words, statement 83 words):

> An expression is one text in the closed expression grammar
>
> Fields typed `predicate`, `compute.expr` and `aggregate.expr` each hold one
> text in the grammar below. Comparisons, Boolean logic, arithmetic, the
> closed functions and the reducers are the whole language: `CASE`, windows,
> subqueries, host-language calls and `!=` are not in it. The whole text is
> read, and checked against its field, before any name is resolved or value
> read. Text that does not parse as one expression fails, as does a truth
> value where a value belongs or a value where a truth value belongs.

Proposed (title unchanged, statement 88 words):

> An expression is one text in the closed expression grammar
>
> Fields typed `predicate`, `aggregate.expr` and expression fields written as
> text (EXPR-5) each hold one text in the grammar below. Comparisons, Boolean
> logic, arithmetic, the closed functions and the reducers are the whole
> language: `CASE`, windows, subqueries, host-language calls and `!=` are not
> in it. The whole text is read, and checked against its field, before any
> name is resolved or value read. Text that does not parse as one expression
> fails, as does a truth value where a value belongs or a value where a truth
> value belongs.

- **Follows:** D18, D31; P-KIND-1 (D58).
- **Fails:** `invalid_expression`, as today.
- In the statement, expression fields written as text replace `compute.expr`.
  A YAML number, Boolean or null in such a field is a literal rather than text
  (EXPR-5), and a mapping there is a registered expression.
- **Detail:** the grammar block is unchanged, since `literal` already holds
  quoted text, temporal literals, `TRUE`, `FALSE` and `NULL`; its cases gain
  the expression-field context of FORMULA-27, and the grammar file's header
  stops naming `compute.expr`.
- **Evidence:** unchanged, plus the new context's vectors.
- **Written by:** P1-2, with FORMULA-27 (Q2).

### FORMULA-9

`rules/formulas.md`; changes the statement; the title stays.

Today (title 14 words, statement 72 words):

> An identifier names a value visible at its site, and any other name fails
>
> Each site sees the names the table below gives, under Column derivation, and
> `compute.expr` resolves a name as a predicate does in the same phase. An
> identifier in a right-side predicate is qualified by the right-side
> dataset's ID, and one over a completed or candidate output row is
> unqualified. No expression reaches an undeclared relation. Identifiers are
> collected for dependency ordering, so an unresolved name fails rather than
> leaving an expression dependency-free.

Proposed (title unchanged, statement 87 words):

> An identifier names a value visible at its site, and any other name fails
>
> Each site sees the names the table below gives, under Column derivation, and
> text in an expression field resolves a name as a predicate does in the same
> phase, except that text holding only a name is a reference (EXPR-4). An
> identifier in a right-side predicate is qualified by the right-side
> dataset's ID, and one over a completed or candidate output row is
> unqualified. No expression reaches an undeclared relation. Identifiers are
> collected for dependency ordering, so an unresolved name fails rather than
> leaving an expression dependency-free.

- **Follows:** D18; P-KIND-1 (D58); D60, which names the only three negatives
  whose fault changes, and the conversion gate of P1-9.
- **Fails:** `unresolvable_name`, `unknown_field`, `qualified_identifier`, as
  today.
- In the statement, expression-field text takes the place of `compute.expr`,
  and the exception carries D18's merge of the `source` string:
  `derivation: DM.SEX` reads through the implicit join, as `source: DM.SEX`
  does today (LOOKUP-1), while text with an operator resolves names as
  `compute.expr` does. `negative-direct-dose-read` therefore keeps
  `qualified_identifier` for `derivation: "EX.EXDOSE * 2"`, at the path the
  rename table rewrites to `columns.DOSEDBL.derivation`. Reading a joined value
  inside arithmetic would be an addition, which a 1.x release may still make.
- **Detail:** the table's two `compute.expr` rows become expression-field rows,
  the paragraphs say expression-field text for `compute.expr`, the example
  becomes `derivation: "WEIGHTKG / POWER(HEIGHTCM / 100, 2)"`, and the last
  sentence says that expression-field text takes part in dependency ordering
  as a predicate does.
- **Evidence:** unchanged, converted in Phase 2.
- **Written by:** P1-2, with FORMULA-27 (Q2).

### FORMULA-12

`rules/formulas.md`; changes the statement; the title stays.

Today (title 12 words, statement 63 words):

> Every operand of arithmetic or of a numeric function must be numeric
>
> An operand of `+`, `-`, `*`, `/`, a unary sign or a numeric function, and
> the root of `compute.expr`, must have runtime type `int` or `float`, in
> every field; an operand of any other type fails. VALUE-2 forbids implicit
> conversion between operation inputs, and expressions do not relax that rule:
> a collected string is converted by binding it to a numeric column first.

Proposed (title unchanged, statement 58 words):

> Every operand of arithmetic or of a numeric function must be numeric
>
> An operand of `+`, `-`, `*`, `/`, a unary sign or a numeric function must
> have runtime type `int` or `float`, in every field; an operand of any other
> type fails. VALUE-2 forbids implicit conversion between operation inputs,
> and expressions do not relax that rule: a collected string is converted by
> binding it to a numeric column first.

- **Follows:** D18.
- **Fails:** `incompatible_input_type`, as today.
- In the statement, "and the root of `compute.expr`" goes. Expression-field
  text needs no numeric root: text holding only a name or a literal returns that
  value with its own type (EXPR-4), and arithmetic and the numeric functions
  return numbers. A `{compute: {expr: X}}` over a `str` column `X`, which fails
  today, converts to the reference `derivation: X`, which reads; no negative
  benchmark depends on that failure.
- **Detail:** unchanged; the report, the `ASTDT <= TRTEDT + 30` example and
  the pointer to FORMULA-23 hold in every field.
- **Evidence:** `negative-review-arithmetic`, unchanged.
- **Written by:** P1-2, with FORMULA-27 (Q2).

## P1-3: schema bundles

### SCHEMA-1

`rules/reference/schema-language.md`; changes the statement; the title stays.

Today (title 14 words, statement 77 words):

> A schema bundle is one entry point and every document it includes, loaded
> whole
>
> `schema.yaml` is the specification-bundle entry point, `schema_project.yaml`
> is the independent project-bundle entry point of the Project file, and
> `schema_environment.yaml` is the independent environment-bundle entry point
> of Project functions. Every schema document is a YAML 1.2 mapping containing
> `version`, an optional `includes` and declarations. `includes` is an ordered
> list of filenames matching `schema_[a-z0-9_]+.yaml`, resolved relative to
> the including file. The complete transitive bundle is loaded before names
> resolve, and every document in a bundle declares the same version.

Proposed (title unchanged, statement 83 words):

> A schema bundle is one entry point and every document it includes, loaded
> whole
>
> `schema_spec.yaml` is the specification-bundle entry point, and
> `schema_project.yaml`, `schema_codelists.yaml` and `schema_function.yaml`
> are the independent entry points of a project file, a codelist file and a
> function file; a file's `type` selects its bundle (SCHEMA-2). Every schema
> document is a YAML 1.2 mapping containing `version`, an optional `includes`
> and declarations. `includes` is an ordered list of filenames matching
> `schema_[a-z0-9_]+.yaml`, resolved relative to the including file. The
> complete transitive bundle is loaded before names resolve, and every
> document in a bundle declares the same version.

- **Follows:** D4, D8; P-TYPE-1 (D58); the entry points P1-1 wrote (#3160),
  which `yaml/README.md` lists.
- **Fails:** none, as today.
- In the statement, the four entry points replace `schema.yaml`,
  `schema_project.yaml` and `schema_environment.yaml`, and `type` selects among
  them: the environment file goes (D8), and the codelist file gets its own
  bundle (P-TYPE-1). The rest stays.
- **Detail:** gains SCHEMA-2's closing sentence, which the first page moved
  here: an invalid include or an inconsistent bundle version fails. Until
  P2-13 the 1.0 bundles sit in `yaml/v1/`, beside the 0.5 bundles in `yaml/`
  that govern a file declaring `schema_version` (D75); P2-13 moves `yaml/v1/`
  up. A 1.0 schema document also declares `kind` on its named value types, and
  P1-3 adds `since`, which SCHEMA-11 lists.
- **Evidence:** the three Rust tests stay; `tools/tests/test_schema_v1.py`
  already checks the four entry modules against the rename table, and P1-3
  adds a test that each `type` selects its bundle.
- **Written by:** P1-3 (Q3).

## P1-5: expressions and handlers

### EXPR-3

`rules/expressions.md`; changes the statement; the title stays.

Today (title 10 words, statement 68 words):

> Expressions nest only in case results and key match values
>
> `source` and `literal` are expression leaves. Every other expression names
> its input variables directly, except in fields whose declared type contains
> `expression`, which are evaluated recursively: each `case` `then` and
> trailing `otherwise`, and a `key` match value under LOOKUP-6. A derivation's
> own top-level expression, in `derivation` or
> `handled_expression_class.value`, is not restricted by this rule. Fields
> typed `numeric_expression`, `string_template` and `aggregate_expression` are
> leaves whose identifiers their own rules resolve.

Proposed (title unchanged, statement 82 words):

> Expressions nest only in case results and key match values
>
> Text in an expression field, a YAML number, Boolean or null, and a
> structured `source` are expression leaves. Every other expression names its
> input variables directly, except in fields whose declared type contains
> `expression`, which are evaluated recursively: each `case` `then` and
> trailing `otherwise`, and a `key` match value under LOOKUP-6. A derivation's
> own top-level expression, in `derivation` or
> `handled_expression_class.value`, is not restricted by this rule. That text
> and fields typed `string_template` and `aggregate_expression` are leaves
> whose identifiers their own rules resolve.

- **Follows:** D18, D31.
- **Fails:** none, as today.
- **Title:** it stays: a registered expression still nests only in a case
  result or a key match value, and a function argument is one leaf (FUNC-6).
- In the statement, the grammar's text, the YAML literals and the structured
  `source` replace `source` and `literal` as the leaves, and that text takes
  the place of `numeric_expression` (`compute.expr`) in the last sentence.
- **Detail:** FORMULA-9 resolves the identifiers of expression-field text and
  of `aggregate_expression`. The paragraph on plain strings becomes: a plain
  string's meaning follows its field's kind (EXPR-5), grammar text in an
  expression field, one variable in a name field (NAME-5) and text in a value
  field; a function argument is one leaf (FUNC-6), and the tagged leaf forms go
  (D18).
- **Evidence:** `schema-expressions`, converted in Phase 2.
- **Written by:** P1-5.

### EXPR-4

`rules/expressions.md`; changes the title and statement.

Today (title 12 words, statement 20 words):

> source copies a named variable, and literal returns its declared scalar
> unchanged
>
> `source` copies a named source or derived variable, and `literal` returns
> the declared scalar value unchanged. Both are expression leaves.

Proposed (title 17 words, statement 26 words):

> A reference or a structured source copies a named variable, and a literal
> returns its value unchanged
>
> A reference or a structured `source` copies a named source or derived
> variable, and a literal returns its value unchanged (EXPR-5). Each is an
> expression leaf.

- **Follows:** D18; the rename table's `literal` and `source` entries (D58).
- **Fails:** none, as today.
- **Title:** `literal` goes and `source` remains only as the structured
  binding, so the title names a reference, a structured source and a literal,
  the words of D78's swaps for EXPR-6 and SUB-19. "Its declared scalar" becomes
  "its value", since a literal may also be quoted text or a `DATE` or
  `DATETIME` literal.
- **Detail:** "A dataset-qualified `source` reads through the implicit join"
  becomes "a dataset-qualified reference". EXPR-14's table replaces its
  `literal` and `compute` rows with one row for expression-field text: a
  reference's variable type, a literal's type (FORMULA-3 for a grammar literal;
  `int`, `float` or `bool` for a YAML scalar, and missing for null), or
  VALUE-10 over arithmetic.
- **Evidence:** `schema-expressions` and `schema-source-binding`, converted in
  Phase 2.
- **Written by:** P1-5.

### COLUMN-11

`rules/columns.md`; changes the title and statement.

Today (title 17 words, statement 84 words):

> `absent` answers a variable or record that does not exist, and `missing`
> answers a missing input value
>
> `absent` on a `source` binding applies when the variable or ODM item does
> not exist in context, and when its implicit join finds no record; never when
> a variable or record exists and holds a missing value. `missing` applies
> when the named input value is missing, on every expression that declares it;
> where an operation has several inputs, such as a `str_template` with several
> placeholders, it fires when any input is missing. `invalid`, `unmapped` and
> `str_extract`'s `no_match` fire only when every input is present.

Proposed (title 15 words, statement 82 words):

> `absent` answers a lookup that finds no record, and `missing` answers a
> missing input value
>
> `absent` on a `source` binding or a named intermediate applies when its
> lookup finds no record, never when a record exists and holds a missing
> value; a variable that does not exist fails under NAME-3. `missing` applies
> when the named input value is missing, on every expression that declares it;
> where an operation has several inputs, such as a `str_template` with several
> placeholders, it fires when any input is missing. `invalid`, `unmapped` and
> `str_extract`'s `no_match` fire only when every input is present.

- **Follows:** D67; P-HND-1 and P-HND-5 (D58).
- **Fails:** none, as today; a variable that does not exist fails with NAME-3's
  `unknown_field` or `unresolvable_name`.
- **Title:** "a variable or record that does not exist" becomes "a lookup that
  finds no record": under D67 a variable that does not exist always fails, and
  P-HND-1 names the intermediate's answer `absent` too.
- In the statement, "the variable or ODM item does not exist in context" goes:
  an `odm` read takes no `absent`, and one that identifies no record gives
  missing (NAME-13). "Its implicit join" becomes "its lookup", which covers the
  intermediate (LOOKUP-25).
- **Detail:** an intermediate's `no_match` becomes `absent` (P-HND-1), and "on
  the `impute` stage" becomes "in the `impute` phase" (P-PHASE-1); the
  paragraph on aggregates stays. COLUMN-10's bind row goes with this change
  (D67, first page).
- **Evidence:** the `absent_variable` and `explicit_null` cases of
  `original_source_bindings.json` answer a variable that does not exist; they
  keep their 0.5 answers until P2-13, and their 1.0 conversions fail under
  NAME-3.
- **Written by:** P1-5, which builds D67.

## P1-13: standards

### DEFINE-6

`rules/profiles/define-xml.md`; changes the title and statement.

Today (title 13 words, statement 48 words):

> The section's implementation guide binds every dataset and belongs to the
> section's family
>
> The selected section's `standard` binds every represented dataset to the
> same implementation guide of type `IG`, and its name must belong to the
> selected section's family. The `sdtm`, `adam` and `send` sections produce
> separate documents, and each document carries the controlled-terminology
> standards cited by its bound codelists' sources.

Proposed (title 18 words, statement 66 words):

> The section's implementation guides belong to its model, and each dataset is
> bound to the one it selects
>
> The selected section's `standards` lists one or more implementation guides
> of type `IG` with distinct names, each belonging to the selected section's
> model. Each represented dataset is bound to the guide its specification's
> `submission.standard` names, or to the first when it names none (DEFINE-8).
> The `sdtm`, `adam` and `send` sections produce separate documents, and each
> document carries the controlled-terminology standards cited by its bound
> codelists' sources.

- **Follows:** D16, D25; P-STD-1 and P-MODEL-1 (D58); the rename of
  `unknown_standard_family` to `standard_model_mismatch` (D58).
- **Fails:** `standard_model_mismatch`, `duplicate_identifier` (Q4),
  `value_not_permitted`. Kept until P2-13: nothing on this card; the 0.5
  path's `unknown_standard_family` stays on SUB-3's Fails line, as the first
  page's SUB-3 keeps it.
- **Title:** the one guide that binds every dataset becomes a list from which
  each dataset takes one (D16), and "family" becomes "model" (P-MODEL-1).
- In the statement, "its name must belong to the selected section's family"
  becomes the rule for every listed guide. A section guide outside the
  section's family fails today as `unknown_standard_family`, cited to SUB-3;
  under its 1.0 name the condition joins this card, which owns the section's
  list, while SUB-3 checks the bound guide against the specification's model.
  `value_not_permitted` stays, as today, for a requested section other than
  `sdtm`, `adam` or `send`; D73's condition for an unknown section, modelled
  on PROJECT-6, may take its place under rewrite 3.
- **Detail:** "SUB-3 reads each dataset's family from the bound standard's
  name" becomes "SUB-3 checks the bound guide against the model the
  specification declares". The document lists every standard of the section in
  declaration order, as today, and each dataset's `def:StandardOID` names its
  bound guide; OIDs stay `STD.<name>` (P-STD-1).
- **Evidence:** `sdtm-dm-metadata`, converted to a one-entry `standards`, and
  new negatives for a guide of another model, a repeated name and an unknown
  `submission.standard`, authored independently.
- **Written by:** P1-13.

## P1-14: value-level comparators

### SUB-12

`rules/profiles/submission.md`; changes the statement; the title stays.

Today (title 14 words, statement 81 words):

> A column's `value_level` entries give the rows their where clause selects
> their own metadata
>
> A column's `submission` may declare `value_level` entries. Each entry has an
> `id` unique within the column, a `where` clause, and metadata fields that
> override the column's on the rows the clause selects; a field it leaves
> absent inherits the column's. `where` maps declared `str` output columns,
> other than the column itself, to nonempty lists of distinct values, and
> selects a row when every listed column holds one of its values. Two entries
> whose where clauses can select one row fail validation.

Proposed (title unchanged, statement 85 words):

> A column's `value_level` entries give the rows their where clause selects
> their own metadata
>
> A column's `submission` may declare `value_level` entries. Each entry has an
> `id` unique within the column, a `where` clause, and metadata fields that
> override the column's on the rows the clause selects. `where` maps declared
> `str` output columns, other than the column itself, to nonempty lists of
> distinct values, and selects a row when every listed column holds one of its
> values or, under the comparator `NE` or `NOTIN`, none of them, missing
> included. Two entries whose where clauses can select one row fail
> validation.

- **Follows:** D66, widening D17; P-VLM-1 (D58), with the spelling
  `{comparator: NE | NOTIN, values: [...]}` that `yaml/v1/` already declares.
- **Fails:** `value_level_where_invalid`, `duplicate_identifier`,
  `value_level_overlap`, as today.
- In the statement, the comparators join the where rule, and "a field it
  leaves absent inherits the column's" goes to stay within 90 words: "override"
  already says it, and the detail's field-by-field merge states it.
- **Detail:** `NE` takes one value and `NOTIN` two or more, and either selects
  the rows whose value is none of them, a missing value included; a list
  without a comparator stays EQ for one value and IN for several, and
  `{values: [...]}` without a comparator restates the list and fails as
  `redundant_long_form` (SCHEMA-15). `value_level_where_invalid` gains a reason
  for a value count the comparator does not take, which P1-14 names. The
  overlap test gains the complement: `{comparator: NE, values: [X]}` and `[X]`
  on one column share no value, so the observed and derived DTYPE rows of
  #3048 can be described, while two `NE` or `NOTIN` clauses on one column
  always share the missing value. DEFINE-27's detail writes the comparator as
  `RangeCheck/@Comparator`.
- **Evidence:** today's evidence stays; new benchmarks, authored
  independently: a DTYPE slice with `NE`, a `NOTIN` positive, and negatives for
  a wrong value count and an overlap.
- **Written by:** P1-14.

## P1-15: ODM input

### New ODM card: the ODM source profile

Proposed (title 16 words, statement 82 words):

> An `.xml` input is read as an ODM snapshot, refusing transactional content
> and skipping audit records
>
> An `.xml` input is read as an ODM 1.3 or 2.0 snapshot into the item table of
> NAME-8: one record per `ItemData` of its `ClinicalData`, in document order.
> A `FileType` of `Transactional`, or a `TransactionType` of `Remove` on any
> element, fails as `source_odm_transactional` before any record is read, and
> a document without `ClinicalData` fails as `source_odm_no_clinical_data`.
> Any other transaction type is read as the present value. `AuditRecord`,
> `Signature`, `Annotation`, `ReferenceData` and `AdminData` elements are
> skipped. `read_odm` gives the same table and refusals.

- **Follows:** D26, D73; P-ODMIN-1 (D58); the conditions
  `source_odm_transactional` and `source_odm_no_clinical_data` (D69).
- **Fails:** `source_odm_transactional`, `source_odm_no_clinical_data`, and the
  `source_odm_*` conditions P1-15 registers for the other `read_odm` refusals
  (D73). Kept until P2-13: none; the card is new.
- **File and number:** `rules/reference/odm.md`, a new rule file titled "ODM
  source profile" with the prefix `ODM`, topic Reading, as number 1 of the new
  series; no `ODM` card or file has existed (Q6). The Rule files
  table of `rules/README.md` gains its row, and INPUT-1's table names it as the
  owner of the `odm` profile.
- **Detail:** `ClinicalData` gives `StudyOID` and `MetaDataVersionOID`,
  `SubjectData` gives `SubjectKey`, `StudyEventData`, `FormData` and
  `ItemGroupData` give their OIDs and repeat keys, and `ItemData` gives
  `ItemOID` and `Value`; the other columns of the `read_odm` table are vendor
  fields, which an `odm` read never sees (NAME-8). A `FileType` of `Snapshot`,
  or none, is read, and `Insert`, `Update`, `Upsert` and `Context` read as the
  present value. The document is UTF-8 without a DOCTYPE, in the ODM 1.3 or 2.0
  namespace with a matching `ODMVersion`, as `read_odm` requires today. An
  archive is not an input in 1.0: `.tar` selects no profile (INPUT-1),
  `read_odm` keeps reading archives, and `transactions: replay` and
  `archive_member` stay reserved. P1-15 settles in detail the names of the
  other `source_odm_*` conditions, the read limits, and what an `.xml` input
  that no `odm` read names may declare; one that an `odm` read names declares
  no `types`, `schema` or `ordinal` (NAME-8, INPUT-4).
- **Evidence:** no benchmark reads ODM XML; the positive and the two refusals
  of the first page's INPUT-1 entry, authored independently. The two
  repository ODM fixtures declare no `FileType` and stay readable.
- **Written by:** P1-15, with INPUT-1, NAME-8 and the `source_odm_*`
  registrations (D73).

### NAME-8

`rules/names.md`; changes the statement; the title stays.

Today (title 15 words, statement 80 words):

> An ODM input binds its eleven fixed text fields by case-folded name, checked
> at validation
>
> An ODM input is an input dataset an `odm` expression names. Its schema is
> fixed: eleven fields, each of type `str`. A stored field binds to the schema
> field whose name it equals under ASCII case folding. At validation, from the
> CSV header or the Parquet schema and without reading a record, exactly one
> stored field must bind to each schema field, and each bound field must be
> stored as text. An ODM input declares no `types` and no `schema`.

Proposed (title unchanged, statement 90 words):

> An ODM input binds its eleven fixed text fields by case-folded name, checked
> at validation
>
> An ODM input is an input dataset an `odm` expression names. Its schema is
> fixed: eleven fields, each of type `str`. A stored field binds to the schema
> field whose name it equals under ASCII case folding. At validation, from the
> CSV header or the Parquet schema and without reading a record, exactly one
> stored field must bind to each schema field, and each bound field must be
> stored as text; an `.xml` input's fields come from the ODM source profile.
> An ODM input declares no `types` and no `schema`.

- **Follows:** D26; P-ODMIN-1 (D58).
- **Fails:** `odm_schema_field_ambiguous`, `odm_schema_field_missing`,
  `odm_schema_field_type`, as today.
- **Title:** it stays: the profile gives an `.xml` input the schema's exact
  names, which bind under case folding like any stored name.
- In the statement, one clause names the `.xml` input, which has no header to
  check: the profile builds its table with these fields as text. The statement
  reaches the 90-word limit.
- **Detail:** the field list stays, and the profile's other fields are vendor
  fields, as in a stored item table. Today's statement names the CSV header
  and the Parquet schema; the same binding also runs on SAS and Transport item
  tables (`project_odm` in `rust/crates/yamaa-adapters/src/source_decoding.rs`).
  Naming them is outside D26 and is not proposed here.
- **Evidence:** the three negatives stay, and P1-15's `.xml` positive binds the
  profile's fields.
- **Written by:** P1-15, with the new card and INPUT-1.

## Not yet scheduled

### CSV-10

`rules/reference/csv.md`; changes the title and statement.

Today (title 9 words, statement 30 words):

> A csv source is UTF-8 without a byte-order mark
>
> A delimited source is UTF-8. Ill-formed encoded bytes fail under Values
> rather than being replaced, skipped or decoded under a machine default. A
> byte-order mark is rejected rather than skipped.

Proposed (title 15 words, statement 47 words):

> A csv source is UTF-8, with a byte-order mark only where its input declares
> one
>
> A delimited source is UTF-8. Ill-formed encoded bytes fail under Values
> rather than being replaced, skipped or decoded under a machine default. A
> byte-order mark is rejected rather than skipped, unless the input declares
> `byte_order_mark: true`: then the source must begin with one mark, which is
> skipped.

- **Follows:** D15; #3157. The spelling and the rule for a declared mark that
  is absent are Q5's recommendation, and the field is an addition to the
  rename table, which D27 leaves to the owner.
- **Fails:** `invalid_text` and `source_byte_order_mark`, as today, the latter
  also for a declared mark that is absent; `value_not_permitted` for the
  declaration on another profile (Q5).
- **Title:** "without a byte-order mark" becomes conditional on the declaration.
- **Detail:** a declared mark is the three bytes `EF BB BF` that begin the
  file, and the first record starts after it; `source_byte_order_mark` reports
  whether a mark was declared. CSV-1's artifacts still carry no mark.
- **Evidence:** `negative-source-bad-encoding` and the `bom` fixture stay;
  #3157 asks for positive and negative benchmarks, a CRLF file among them,
  authored independently, which #3061's Windows lane runs.
- **Written by:** the pull request for #3157. The plan schedules none, and the
  spelling must be settled before the 1.0 schema freezes.

## A card neither page lists

### SCHEMA-11

`rules/reference/schema-language.md`; changes the statement; the title stays.

Today (title 12 words, statement 63 words):

> A descriptor uses only the closed descriptor keywords, each where it applies
>
> Only these descriptor keywords are supported: `type`, `required`,
> `description`, `pattern`, `min_length`, `size`, `values` and `default`.
> `type` is required and contains one type expression or a union. Every other
> keyword is allowed only where the table below admits it. A `default` is
> allowed only when `required` is false or absent, and it must satisfy the
> same descriptor. An invalid descriptor keyword or default fails.

Proposed (title unchanged, statement 65 words):

> A descriptor uses only the closed descriptor keywords, each where it applies
>
> Only these descriptor keywords are supported: `type`, `required`,
> `description`, `pattern`, `min_length`, `size`, `values`, `default`, `kind`
> and `since`. `type` is required and contains one type expression or a union.
> Every other keyword is allowed only where the table below admits it. A
> `default` is allowed only when `required` is false or absent, and it must
> satisfy the same descriptor. An invalid descriptor keyword or default fails.

- **Follows:** D31, which `yaml/v1/` carries in `kind` (P1-1, `yaml/README.md`);
  P-VER-1 (D58), for which the plan's P1-3 adds a `since` key to the schema
  notation.
- **Fails:** none, as today.
- Both pages missed it because neither keyword is a rename: the table lists
  fields, not notation. The engine's bundle admission refuses `kind` today,
  and it gains both keywords when it reads 1.0 schema files.
- **Detail:** the table gains `kind`, allowed on a named value type only,
  holding `expression`, `name`, `identifier`, `value`, `path` or `structure`,
  with the rules of `yaml/README.md` for the kind a field takes; and `since`,
  allowed where P1-3 admits it, holding the language version that introduced a
  field or value, which the version rule reads (SCHEMA-2). If P1-3 places
  `since` outside the descriptors, only `kind` joins the list.
- **Evidence:** `tools/tests/test_schema_v1.py` checks `kind` today, and P1-3
  adds admission tests for both keywords.
- **Written by:** P1-3 (Q7).

## Approval

Please approve this page on #3030, as a whole or item by item: by card ID and
by question (Q1 to Q7). Approval covers each proposed title and statement, the
new card's file and number, and, through Q5, the CSV input field it adds to the
rename table. Under the standing instruction recorded with D77 and D78, the
1.0 session may approve wording that only follows a recorded decision; Q1 and
Q5 choose between readings of the decisions or add a spelling, so they need
the owner.
