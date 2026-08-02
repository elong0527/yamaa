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
