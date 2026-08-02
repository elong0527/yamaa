# YAML Configuration Layer

This directory is used to store YAML configuration files for the project.

**Agent Guidelines:**
- Ensure all YAML files are strictly valid and well-formatted.
- Keep configurations modular and clearly name files based on their purpose.


## schema.yaml

The `schema.yaml` file defines specifications for ODM to SDTM and SDTM to ADaM
derivations.

### Derivation model

Derivation occurs in two steps:

1. **Row construction** starts from the dataset declared by `base` and
   establishes the output rows. Each row is identified by the dataset `keys`.
   Row construction may change the number of rows.
2. **Column derivation** adds one value per key combination by left-joining on
   the dataset `keys`. Column derivation must not change the number of rows.

When multiple source records match one output key, the derivation must filter,
aggregate, or select one record before the join. Otherwise, it must fail.

For a BDS dataset, each `rows` entry constructs exactly one parameter. Directly
mapped and newly derived parameters use separate row definitions.

### Dataset inputs

`datasets` is a mapping from dataset identifiers to source data paths. The key
is the dataset identifier used by `base` and qualified variable references.
Paths are resolved relative to the specification file.

### Variable references

Qualified references use `DATASET.VARIABLE`. For example, `ODM.ItemOID` reads
the `ItemOID` field from the ODM source and `LB.LBSTRESN` reads `LBSTRESN` from
LB. Unqualified references such as `AVAL` refer to columns in the dataset
currently being derived.

ODM item identifiers may contain periods. `ODM.IT.LB.LBDTC` means the `Value`
whose `ItemOID` is `IT.LB.LBDTC`, resolved within the current ODM context.

### Filter expressions

The schema treats filters as the primitive `sql` type and assumes each value is
a valid SQL predicate. Supported core syntax includes `=`, `<>`,
`<`, `<=`, `>`, `>=`, `IN`, `BETWEEN`, `LIKE`, `IS NULL`, `IS NOT NULL`,
`AND`, `OR`, `NOT`, and parentheses. String literals use single quotes.

SQL three-valued logic applies: a row is retained only when the filter evaluates
to `TRUE`; `FALSE` and `UNKNOWN` both remove the row. A list of filters is
equivalent to joining the predicates with `AND`.

### Function calls

`function` is a quoted `function_call` expression using this language-agnostic
grammar:

```text
function_call := name "(" [named_argument ("," named_argument)*] ")"
named_argument := name "=" value
value          := variable | literal | list | function_call
```

Arguments must be named and unique; their order has no meaning. Unquoted values
inside the expression are variable references, while string literals use
single quotes. Each function defines its required and optional argument names.
Implementations must reject positional, missing, duplicate, and unknown
arguments. They must parse calls into a syntax tree and dispatch registered
functions rather than evaluate the expression as host-language code.
