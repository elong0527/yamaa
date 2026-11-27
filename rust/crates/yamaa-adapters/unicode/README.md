# Unicode data

`15.0.0/UnicodeData-uppercase.txt` holds fields 0 and 12 (the code point and
its simple uppercase mapping) of the rows of
<https://www.unicode.org/Public/15.0.0/ucd/UnicodeData.txt> that state a
simple uppercase, cut by `tools/packaging/generate_case_table.py --cut`.
`LICENSE.txt` is the Unicode-3.0 notice, kept byte for byte with
`rust/crates/yamaa-core/unicode/LICENSE.txt`. Neither builds nor checks fetch
data from the network.

From the repository root, `python -m tools.packaging.generate_case_table`
rebuilds `../casing/uppercase.txt`, the case table behind the ordinal
case-insensitive comparison of locations (OUTPUT-9), from this source and
compares it and this notice byte for byte; `--write` regenerates them. It
also derives `../casing/windows-unmapped.txt`, the rows Windows' ordinal
comparison does not apply, from the Windows oracle's record in
`../tests/fixtures/windows_ordinal_separations.txt` (#4991). The generator's
docstring says how.
