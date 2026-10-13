# Closed grammars

The language admits four closed grammars. Each one is defined once, here:

| Contract | Rule | Written in |
|---|---|---|
| [`predicate.yaml`](predicate.yaml) | [R004](../rules/operations/predicates.md) | a field typed `predicate` |
| [`numeric.yaml`](numeric.yaml) | [R010](../rules/operations/computation.md) | `compute.expr` |
| [`string-template.yaml`](string-template.yaml) | [R012](../rules/operations/text.md) | `str_template.template` |
| [`aggregate.yaml`](aggregate.yaml) | [R013](../rules/operations/aggregation.md) | `aggregate.expr` |

These files are the single authored grammar truth for the shared Rust parser
used by Python and R. Repository validation renders each rule's grammar block
from `productions` and validates the structure and vocabulary of every contract.
The typed core replay checks every case's exact parse shape, collected identifier
set and decision, and expands the declared function arities, reducers, reserved
words and comparison operators into independent acceptance boundaries. Installed
Python and R public suites put the same texts into real specification fields;
later binding failures remain separate from grammar decisions. Empty expression
fields fail public schema admission under REQ-0287 before reaching the parser.

Changing a grammar therefore starts here. A change that is not carried into
every consumer fails validation rather than producing two runtimes that
accept different specifications.

## File format

| Key | Meaning |
|---|---|
| `contract` | the grammar's name, matching the file name |
| `contract_version` | the version of this contract, `MAJOR.MINOR.PATCH` |
| `rule` | the rule that owns the grammar |
| `start` | the production a text must derive from |
| `productions` | the ordered productions, each a `name` and a `definition` |
| `imports` | each non-terminal another contract defines, and that contract |
| `vocabulary` | each non-terminal that names tokens rather than deriving them |
| `reserved` | the spellings a bare identifier may not take |
| `prohibited` | each spelling reserved for a construct the grammar refuses |
| `cases` | the vectors every implementation must reproduce |

A `definition` is EBNF: a quoted terminal is literal, `[x]` is optional, `x*`
and `{x}` repeat, `(x | y)` groups alternatives, and `...` spans a scalar
range. A production marked `prose: true` states its definition in words
because its terminal set is an R019 scalar range rather than an enumeration;
only such a definition may name something other than a terminal or a
non-terminal. `imports` may also name a closed set, and names `schema` as the
source of a non-terminal the schema language rather than another grammar
defines.

A definition that does not fit one line continues on the next. A continuation
that opens an alternative begins with `|`.

## The rule's grammar block

Each rule's `## Grammar` block is the rendering of its `productions`: the
name column is as wide as the longest production name, `:=` follows it, a
continuation that opens an alternative aligns its bar under the `=`, and
every other continuation aligns under the definition. Validation reports the
expected block when a rule carries anything else, so the fix is to paste what
it prints.

## Vectors

Each case in `cases` is one text and the decision every implementation must
make about it:

| Key | Meaning |
|---|---|
| `id` | a stable name for the case |
| `covers` | the behavior the case is evidence for |
| `text` | the text to read |
| `parse` | `accept` or `reject` |
| `condition` | the failure a rejected text produces |
| `identifiers` | the identifier names an accepted text binds, sorted |
| `shape` | the parse an accepted text produces |

A vector pins what the grammar and its closed vocabulary decide. Which names
are visible, and what they are typed, belongs to R001, R002, and R007, so a
replay resolves every identifier rather than importing a binding context the
vector does not declare.

`identifiers` is what R001 collects for dependency ordering, so a vector also
pins that an implementation sees every name a text depends on.

### Shapes

A `shape` is the parse written as a fully parenthesized prefix form, because
an accept-or-reject outcome cannot distinguish two parsers that disagree
about precedence, associativity, or which names are case-insensitive. A
parenthesis in the source text groups and leaves no trace in the shape.

| Form | Node |
|---|---|
| `(and X Y)`, `(or X Y)`, `(not X)` | predicate logic |
| `true`, `false` | a Boolean literal |
| `(= X Y)`, `(<> X Y)`, `(< X Y)`, `(<= X Y)`, `(> X Y)`, `(>= X Y)` | a comparison |
| `(is-null X)`, `(is-not-null X)` | a null test |
| `(in X A B)`, `(not-in X A B)` | list membership |
| `(between X A B)`, `(not-between X A B)` | a range |
| `(like X P)`, `(not-like X P)`, with `(escape 'c')` when declared | a pattern match |
| `(+ X Y)`, `(- X Y)`, `(* X Y)`, `(/ X Y)` | arithmetic |
| `(neg X)`, `(pos X)` | a unary sign |
| `(call NAME A B)` | a function call |
| `(reduce NAME X)` | a reduction |
| `(star DATASET)` | the record star a `COUNT` may take |
| `(id NAME)` | an identifier |
| `(int 42)`, `(float 1.5)` | a numeric literal, in its written form |
| `(str 'text')`, `(date '2025-06-01')`, `(datetime '2025-06-01T08:30')` | a literal value |
| `null` | the missing literal |
| `(template (text 'x') (placeholder NAME))` | a string template |

A function, reducer, or keyword is written in upper case because its spelling
is case-insensitive; an identifier is written as the text spells it because
its spelling is not. A quoted value doubles an interior quote, as R004 does.

## Running the vectors

From the repository root, with Rust 1.90.0 and `py-yaml12==0.2.0` installed:

```bash
python3 .github/scripts/yaml-validation/validate_repository.py --root .
python3 rust/tools/check_shared_grammar.py
```

The first checks the rule blocks and contract structure. The second replays
all authored vectors and expanded vocabulary boundaries directly through the
shared typed core, including shapes that acceptance alone cannot distinguish.
Neither command imports the reference evaluator.

The `R shared grammar` workflow stages, builds and installs the standalone
`R/yamaa` source package and runs `R/yamaa/tests/public_grammar.R` against its
installed copies of these documents. The native workflow runs that suite and
`rust/tests/installed_public_grammar.py` outside the checkout across the complete
supported installed matrix. There is no R parser or public grammar probe API.
