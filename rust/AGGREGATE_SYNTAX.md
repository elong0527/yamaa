# Shared aggregate syntax

`aggregate-syntax/1` is a compiler service for the complete closed R013 grammar.
Python and R call the same Rust parser through `analyze_aggregate(request)`.
It does not select records, bind names, evaluate reducers or enable additional
execution capabilities. The optional normalized-specification frontend still
uses Python aggregate parsing; routing its admission and planning through this
service is a subsequent integration gate. Python remains the default backend and
`execution_supported` remains false.

A request contains exactly `protocol` and `expression`:

```json
{"protocol":"aggregate-syntax/1","expression":"COUNT(D.*)"}
```

The response contains `protocol` and `outcome`. A `parsed` outcome contains an
`ast`, ordered distinct `identifiers`, ordered distinct `star_datasets`, and
ordered distinct `ungrouped_identifiers`. Record-star dependencies remain
separate from fields. Identifiers retain case and first written occurrence;
ungrouped identifiers omit reads enclosed by reducers. The AST uses number,
null, identifier, star, unary, binary, call and reduction nodes. Numbers retain
exact strings with int/float syntax tags; a reduction retains canonical name,
argument and the exact written call text. Grouping controls association without
adding wire nodes. Literal overflow is deferred to evaluation.

All six declared reducers and imported numeric functions parse. Only COUNT takes
a qualified record star; `COUNT(*)` is invalid. Reducers do not nest, including
beneath arithmetic or numeric calls. Imported numeric operators, arities,
reserved names and lexemes retain their existing contracts. This parser does
not qualify EXP/LN/POWER evaluation or relax numerical comparisons.

An `invalid` outcome contains a condition, owning requirement, context, and a
position with zero-based UTF-8 byte and Unicode scalar character offsets. The
aggregate requirements own imported numeric syntax failures. The lexer reads
the whole source before syntax validation; closure precedes function-name/arity
checks and nested-reduction checking. Nested failures name the first inner
reducer in written order. Unknown-function diagnostics retain the written name.

A `resource_limit` outcome contains resource, limit and position. Fixed policy
limits are 65,536 source bytes, 8,192 tokens excluding EOF, 4,096 arena nodes
including groups, and depth 64. The trusted Rust API permits lower budgets and
caps requested depth at 64. Transport requests cannot raise budgets. These are
resource policies, not language conditions or a byte-level allocator guarantee.

Malformed JSON, duplicate/unknown fields, wrong field types or protocol, and
requests over 1,048,576 bytes are transport errors. Adapters never return partial
syntax, invoke a resolver/callback or fall back to Python. R returns from native
code before its facade raises a transport error. Each call owns its response;
failures do not poison a subsequent call.

The core keeps an immutable postorder arena with bounded recursion and flat
ownership. Numeric and aggregate parsing share lexical rules and the numeric
function/arity table. Their arenas remain separate so the numeric compiler
cannot admit reductions accidentally.

`check_aggregate_grammar.py` checks the committed grammar cases, vocabulary and
independent shapes, followed by deterministic comparisons of complete ASTs,
metadata and diagnostics against unchanged Python. `aggregate_syntax.tsv` holds
independently authored portable observations replayed by Rust and both installed
hosts. CI runs Python direct-wheel and source-rebuild tests outside the checkout
and R installed tests from its standalone source archive. These gates qualify
syntax and transport; shared specification compilation and release qualification
remain open.

The Python source archive also carries `yamaa_native.pyi` beside its relocated
`pyproject.toml`. This preserves the public type stub and `py.typed` marker when
rebuilding a wheel from that archive, as required by maturin's
[pure Rust typing layout](https://www.maturin.rs/project_layout.html#adding-python-type-information).
Installed tests check this packaging contract in both distribution forms.
