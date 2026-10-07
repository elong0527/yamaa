# Output declaration truth

`output_declarations.tsv` contains five authored original JSON documents (valid
YAML) and their complete failure reports. Each document reads `source.csv` with
the held bytes `ID\n1\n`, derives `ID` and `VALUE`, then reaches output declaration
validation. The combined case preserves the ordered profile, undeclared column,
duplicate column and internal key findings. It does not reject during preflight.

The independent Python reference `execute_example` produced these reports on
2026-10-07 from the reference source at
`c813b08d98f002e827c2e9fbe1832e965297d24a`. The candidate Rust engine did not
generate them. Only backend and runtime/engine version identity were normalized
to the existing prototype fixture labels. Source tables, capture accounting,
diagnostics, node outcomes, verification and artifact observations are retained.
No artifact is published. Existing committed benchmark truth is unchanged.

The installed Python and R tests compare the complete reports, retain each
failed result after the compiled handle is released, and require repeated save
attempts to fail without publication or recapture. This covers the private
original-document use case; it does not qualify the replacement public API.
