# Regex identifier data

`18.0.0/DerivedCoreProperties.txt` is the unchanged Unicode Character Database
source from <https://www.unicode.org/Public/18.0.0/ucd/DerivedCoreProperties.txt>.
The versioned source and its Unicode-3.0 notice are preserved locally; neither
builds nor tests fetch data from the network. `LICENSE.txt` is the associated
notice from <https://www.unicode.org/license.txt>, retrieved 2026-10-06.
The ASCII source validator allows only these exact third-party data/notice
paths (including the two distribution copies), preserving their original UTF-8
copyright text. Other project source and nearby documentation remain checked.

ECMA RegExpIdentifierName uses **ID_Start** and **ID_Continue**, not the XID
properties used by some programming-language identifier libraries. The core
adds the ECMA dollar, underscore and join-control cases explicitly. Decoded
names retain exact scalar sequences without normalization. Pinning Unicode
18.0.0 makes admission independent of the operating system or host interpreter;
updating the data requires an explicit compatibility review.

From the repository root, `python rust/tools/generate_regex_identifiers.py`
checks the generated intervals and distributed license copies byte for byte.
Use `--write` to regenerate them intentionally. The generator merges only
adjacent intervals; it checks disjointness, scalar bounds and published property
cardinalities. A separate Rust test parses the authoritative text into bitmaps
and checks the actual safe binary-search lookup for every Unicode scalar.
No content digest or host identifier classifier supplies expected truth.

The generated tables have no runtime dependency, allocation or unsafe code.
The original data and notice accompany the Rust source; notice copies accompany
Python wheel/source distributions and installed R packages. The generator
maintains those required copies from the canonical notice.

The in-progress shared schema service also derives printable diagnostic-label
intervals from this source using `rust/tools/generate_schema_printable.py`.
They contain general categories L, M, N, P and S plus ASCII space, read from
the category annotations on Grapheme_Base and Grapheme_Extend. A separate
exhaustive Rust test checks the lookup against the authoritative annotations.
This affects representation of strings inside malformed compound name/id path
labels, not schema value comparison, normalization or regex admission.

Diagnostic display compatibility is not yet qualified across host Unicode
versions. A supplemental comparison with Python's Unicode 16.0.0 found 17,810
scalars printable in these Unicode 18.0.0 tables but unprintable in that host;
none differed in the opposite direction. This is an explicit open compatibility
item before the schema service is advertised. The comparison does not define
expected truth or alter the Python default backend.
