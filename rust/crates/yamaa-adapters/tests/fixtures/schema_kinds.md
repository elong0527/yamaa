# Decoded schema kind witnesses

The new `portable-kinds-existing-wire` row in `schema_layer_admission.tsv`
contains fourteen independent queries. Each decoded kind (`null`, `boolean`,
`integer`, `float`, `text`, `sequence`, `mapping`) is rejected once as an
inheritance layer and once by an incompatible descriptor type. This distinguishes
the existing layer labels (`NoneType`, `list`, `dict`) from the existing
descriptor labels (`null`, `sequence`, `mapping`). Both installed hosts replay
the complete stateless response and the captured schema response.

The new expected response was authored from Python's unchanged
`_validate_layer` and `_validate_type` reference functions on 2026-10-07. The
schema and compilation metadata are retained from the pre-existing independent
`layer-column-fragment` fixture. No native output was used as expected truth,
and all seven pre-existing rows are unchanged.

Core findings carry `DocumentKind` in `SchemaContext::ValueKind` or
`SchemaContext::LayerKind`. A named schema class or an authored type expression
remains text; identifiers such as `column_class` and `list[str]` describe the
language rather than a host class. Only the adapter projects the retained wire
vocabulary. The eight-byte maximum portable kind name is charged to the shared
diagnostic text budget even though the enum owns no string. The existing bounded
wire projection charges the actual rendered text separately.
