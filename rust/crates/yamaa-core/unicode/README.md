# Unicode data

`18.0.0/DerivedCoreProperties.txt` is the unchanged Unicode Character Database
source from <https://www.unicode.org/Public/18.0.0/ucd/DerivedCoreProperties.txt>.
The versioned source and its Unicode-3.0 notice are preserved locally; neither
builds nor tests fetch data from the network. `LICENSE.txt` is the associated
notice from <https://www.unicode.org/license.txt>, retrieved 2026-10-06.
The ASCII source validator allows only these exact third-party data/notice
paths (including the two distribution copies), preserving their original UTF-8
copyright text. Other project source and nearby documentation remain checked.

The shared schema service derives printable diagnostic-label intervals from
this source using `tools/packaging/generate_schema_printable.py`. They contain
general categories L, M, N, P and S plus ASCII space, read from the category
annotations on Grapheme_Base and Grapheme_Extend. A separate exhaustive Rust
test checks the lookup against the authoritative annotations. This affects
representation of strings inside malformed compound name/id path labels, not
schema value comparison, normalization or regex admission. Regular-expression
group names are ASCII identifiers and need no Unicode property data.

From the repository root, `python -m tools.packaging.generate_schema_printable`
checks the generated intervals and distributed license copies byte for byte.
Use `--write` to regenerate them intentionally. The generator merges only
adjacent intervals and checks disjointness and scalar bounds. No content digest
or host classifier supplies expected truth. Pinning Unicode 18.0.0 makes the
tables independent of the operating system or host interpreter; updating the
data requires an explicit compatibility review.

The generated tables have no runtime dependency, allocation or unsafe code.
The original data and notice accompany the Rust source; notice copies accompany
Python wheel/source distributions and installed R packages. The generator
maintains those required copies from the canonical notice.

Diagnostic display compatibility is not yet qualified across host Unicode
versions. A supplemental comparison with Python's Unicode 16.0.0 found 17,810
scalars printable in these Unicode 18.0.0 tables but unprintable in that host;
none differed in the opposite direction. This is an explicit open compatibility
item before the schema service is advertised. The comparison does not define
expected truth or alter the Python default backend.
