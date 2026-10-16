# parquet 60.0.0

Source: the crates.io `parquet` 60.0.0 release, published by the Apache
Arrow project from [arrow-rs](https://github.com/apache/arrow-rs). The
crate is Apache-2.0 licensed; the released `LICENSE.txt` is retained here
under the name `LICENSE` (the local packaging checks require that name),
and the released `NOTICE.txt` is retained unchanged.

All files are the released bytes except:

- `src/file/metadata/writer.rs` carries the complete change recorded in
  `omit-column-orders.patch`: `ThriftMetadataWriter::finish` computes the
  footer `column_orders` as before but then discards it, writing the
  optional field as absent. Upstream offers no `WriterProperties` setting
  for this field, and it is computed unconditionally from the physical
  type, so FLOAT and DOUBLE columns always carried the IEEE 754
  total-order union member. Readers that predate that member (for
  example Polars before 1.43.2) refuse the whole footer, which is what
  forced yamaa's Python Polars floor to 1.43.2. yamaa writes no Parquet
  statistics (`EnabledStatistics::None` in the adapters writer), so no
  reader needs the declared order to interpret yamaa's files, and the
  field is optional in the Parquet format. With it omitted, Polars
  1.35.2 and later read yamaa's saved Parquet. Files saved by the
  pre-patch writer still carry the field and still need Polars 1.43.2
  or later (or another reader) to open.
- The crates.io packaging files `.cargo-ok`, `Cargo.lock` and
  `Cargo.toml.orig` are omitted: a workspace patch crate resolves
  through yamaa's canonical `rust/Cargo.lock`, and the released
  `Cargo.toml` is retained as the manifest.

No parquet API or dependency was changed.

Re-applying on a version bump: copy the new crates.io release into this
directory the same way, re-apply `omit-column-orders.patch` (it applies
to the `finish` method of `ThriftMetadataWriter`; if upstream has grown
a writer option for the field, prefer the option and delete this patch),
keep the `LICENSE` rename and the same three omissions, then regenerate
`rust/Cargo.lock` and re-run the saved-reader qualification at the
declared minimum reader versions. Remove this patch once a released
upstream version writes footers that the declared minimum Polars reads,
or offers the opt-out, and the saved-reader controls pass on it.
