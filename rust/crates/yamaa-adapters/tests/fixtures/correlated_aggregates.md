# Aggregate relation controls

`correlated_aggregates.tsv` contains independently authored original YAML,
source bytes, complete portable observations and exact CSV bytes. Its values
and diagnostics are declared directly, without copying native execution output.
The Rust frontend and both installed host suites consume the same truth.

The sixteen controls cover:

- Enclosing grouped rows, qualified donor matching and completed output-row
  reduction in one run. The grouped expression produces 13 and 12; their
  treatment-group mean is exactly 12.5. The missing group remains missing.
- Exact integer MAX above binary64's consecutive-integer range and COUNT of
  two missing values, which is zero for a nonempty relation.
- Mixed relation and ungrouped identifier admission, required output groups,
  qualified group scope, missing key fields and match variables, and key type
  validation with original operation and expression paths.
- Scalar and list key shorthands, including repeated equal list pairs, and
  explicit empty map/list keys with their complete retained declarations.
- Eager ONLY behind COALESCE, retaining row name, complete raw group context
  and record cardinality before any accepted output or publication.
- A later column ONLY failure preserving the enclosing grouped-row context
  and the current output key identity.

The fixed engine/runtime versions are report comparison controls. Float table
observations use exact binary64 bits; missing cells retain the established
`{"type":"missing","value":null}` wire spelling. Each failed build refuses
save and each successful build is saved twice with identical bytes.

Derived aggregate bindings, between narrowing, correlated current-row filters
and aggregate reads through named intermediates remain explicit follow-on
scope. Existing benchmark expected artifacts are unchanged.
