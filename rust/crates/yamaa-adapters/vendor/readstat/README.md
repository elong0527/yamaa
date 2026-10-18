# ReadStat source integration

This directory vendors the MIT-licensed SAS reader subset of ReadStat 1.1.9,
upstream commit `104ba03a8da116eb8c094abc18bc2530b733eda9` from
<https://github.com/WizardMac/ReadStat>, with the later upstream fixes listed
under [Upstream changes after 1.1.9](#upstream-changes-after-119), ported by
hand. Upstream `dev` was last reviewed through commit
`1e3c2f68ab36b0f171eb88de93a33d8ef7c666ad` (2026-10-18), the review baseline:
every upstream commit up to it that changes a vendored file is listed there.
The upstream license is in `LICENSE`.
SAS writer/header-construction helpers are omitted from the shared upstream
header reader unit. The adapter exports only a read-only memory bridge and
builds no writer or hash-table code.

`build.rs` compiles this fixed source inventory into a static library through
`cc` when the default `readstat` feature is enabled. Disabling that feature
omits this native backend and the adapter's direct `cc` build dependency.
Linux, macOS, MSVC Windows, and MinGW Windows use the same C/Rust
bridge; C owns ReadStat unions and bitfields. No dynamic ReadStat library,
Python/R reader package, generated bindings, configure program, platform iconv,
or network access is needed during package builds. The iconv shim fails closed;
Rust strictly decodes the explicitly declared source encoding.

## Local changes

Local changes are limited to removal of writer helpers, the integration bridge
and iconv shim, checked
allocation initialization, byte-preserving conversion rejecting embedded NUL,
source-authority encoding metadata treatment, early fixed table-shape
admission, SAS physical extent validation, shared ownership of column-text
metadata pages, unsupported compression signature
rejection, transport physical type/offset and partial-row validation, transport
version 8 observation count admission, overlapping RDC back-reference
propagation, accounting for records deleted in place, and exact IBM numeric
precision facts used by narrowing. The row text scratch buffer is sized from
the validated submitted columns rather than from every column attribute slot.
A transport member that declares no variables (or a negative count) reaches
the metadata handler, as a SAS file without columns does, instead of failing
as an empty allocation. SAS metadata still records the embedded encoding but
does not override the explicit declaration. The bridge contains snapshot memory
IO callbacks and never initializes filesystem IO. Rust contains every callback
panic before it reaches C. The parent parser lifetime contains all callbacks.

Records deleted in place are skipped, never returned as rows, as upstream
reads them: a bit in the deleted-row bitmap of a data or mix page that carries
the deleted-rows flag `0x0080`, or the subheader compression code `0x05` of a
compressed row. A moved record that no reference reaches (code `0x09` or
`0x0d`) also counts as deleted here. The row-size subheader counts the deleted
records (offset 56 in 64-bit files and 28 otherwise). Upstream trusts that
count; locally, every counted deletion must be found marked. The second pass therefore reads past
the last live record, through flagged pages and later pages, until it has
found them all, and a count left unmatched at the end stops the parse. A count
above the stored records, a mark beyond the count, a bitmap that does not fit
its page and a page row beyond its bitmap also stop it. Each of these stops
goes through the bridge's `yamaa_readstat_deleted_records` hook, so it reports
cause `deleted_records`. The bitmap extent is checked term by term against the
rest of the page, so no 64-bit sum can wrap. The bridge never sets a row limit
or offset, so the accounting covers every stored record.

Rust checks the fixed source-byte and table-shape boundaries from the shared
`Limits` policy. The C readers admit declared row/column/cell geometry before
table-sized metadata allocation. Allocation arithmetic and source page/header
extents are checked. Compressed SAS row buffers are allocated only after
physical scalar widths and offsets pass, when columns are submitted for a
compressed row or, if uncompressed rows came first, at the first compressed
row. Columns are submitted only once every declared column has attributes and
a name: a column without attributes would otherwise read as a zero-width
character column. Snapshot reads and seeks validate their ranges against the held input.
The SAS header/page sizes are bounded by the captured snapshot framing, rather
than a separate 16 MiB threshold. Their minimum framing sizes still apply.
Text-blob count follows the 65,536 slots addressable by source uint16 references.
Each metadata page containing text is retained once. Text-blob slots borrow
their validated ranges from that owner, so repeated and overlapping subheader
pointers preserve their physical indices without allocating another copy of
the referenced bytes. The parser owns these pages until all metadata and rows
have been consumed.
A small independently repacked haven fixture with 4,097 valid text
blobs preserves all twenty numeric cells and is accepted by the independent
pandas SAS reader; no lower 4,096-blob ceiling is applied.

## Upstream changes after 1.1.9

Each upstream commit after `104ba03a`, through the review baseline, that
changes a vendored file is ported by hand, inside the local changes above, or
not ported, with its reason. The release checklist in `docs/release.md`
reviews the commits after the baseline and moves it.

| Upstream commits | Change | Status |
| --- | --- | --- |
| `365c743e`, `78bb8a94` | A 64-bit subheader's eight-byte signature is compared in full, so a row stored uncompressed in a compressed dataset is not read as metadata when its first four bytes match a signature. Big-endian files keep their four-byte layout. | Ported |
| `d19f262e`, `634f40a8`, `5850fb57` (bitmap) | Records deleted in place are skipped: bitmap bits on flagged data and mix pages, and compression code `0x05`. The live count excludes them, including when a row offset applies, and the bitmap offset and the rows it covers are bounded. | Ported, with the deleted-record accounting above |
| `0a1771f7` | A compressed record SAS moved to another page is read at its reference (code `0x03`), from its moved slot (code `0x02` or `0x06`); a moved record no reference reaches (code `0x09` or `0x0d`) is skipped. | Ported; locally, a record no reference reaches counts as deleted, so the row-size subheader must count it |
| `39027f57`, `b05b6456` | A version 8 transport file reads exactly the observations its `OBSV8` header counts, so trailing all-blank records are data. A zero count stays unknown. | Ported; locally the count is parsed with `strtoll`, admitted before any record is read, must be met, and may be followed only by blank padding |
| `0dc2aeee` | Without a count, a trailing all-blank record at least 80 bytes wide is data, since padding is always shorter than one 80-byte record. This applies to version 5 files too. | Ported |
| `da9fcaa4` | A numeric value whose column width is outside 1 to 8 bytes fails instead of shifting by an undefined amount. | Ported. Every column has attributes when columns are submitted, and later attributes subheaders describe further slots, so a submitted width cannot change; the guard is defensive. |
| `5850fb57` (release field) | The header's release field is copied into a terminated buffer before `sscanf` reads it. | Ported |
| `3c68974f` | Allocation failures are checked. | Already covered by the local checked allocations |
| `73a35397` | Transport label buffers keep their memory when `realloc` fails. | Already covered by the local checked reallocations |
| `a0aa9f31` | RDC decompresses into the dataset's row buffer instead of allocating one per row. | Ported |
| `a2fcaa91` | A dataset with no records reads every page, so column metadata on a later page is not lost. | Ported |
| `4d3c7f77` | A zero-width character column is accepted; its values are empty text. | Ported. A column that has no attributes is not a zero-width column: submission waits for every column's attributes and name. |
| `b2d5407d`, `bb29605a`, `95aee731` | The release field accepts revision tags `J` and `TK` for SAS 8 and 9, any tag for other majors such as SAS 7, and the alternative magic number whose first 28 bytes are zero. | Ported |
| `d6ffcec1`, `73756b20` | Encoding code 136 names CP932 and code 38 names ISO-8859-10. | Ported. The name is recorded metadata only; the declared encoding decodes text. |
| `fdf4e1e0`, `5c446781`, `4010db5e`, `6f2bcc65`, `6443b805`, `1badba77`, `fa08c079` | Display format digits, widths and informats are read into each variable's format strings, in SAS datasets and version 8 transport files. | Not ported: the bridge passes on only names, widths and storage types, and display formats never select types, so these change no result. Reading informat text references would also refuse a file whose unused informat reference is out of range. |
| `38d4418e` | Header timestamps subtract their recorded time-zone offsets. | Not ported: the bridge does not pass header timestamps on. |
| `a1496e2c` | An unspecified encoding (code 0) is reported as none, and text converts from Windows-1252 by default. | Not ported: an unknown encoding is recorded as empty text, and the declared encoding always decodes text. |
| `725bb907` | `readstat_double_value` returns NaN for a tagged missing value, and a non-trimming conversion is added for Stata. | Not ported: the bridge reads missingness separately, and the SAS readers trim padding. |
| `6ed4ccda`, `98813868`, `ba4392e9`, `a4984d52`, `558aa896`, `913267ed`, `47579684` | Declarations, documentation, error codes and metadata in the shared files for the writers and the SPSS and Stata readers. | Not ported: the vendored subset builds neither, and the SAS readers do not use them. |
| `8884b6b3`, `2d942c31`, `1984af8e`, `95adba94` | SAS writer changes in the shared SAS unit. | Not ported: the writer helpers are omitted. |
| `211c342a`, `f6a54c24` | `(void)` prototypes and comment typos. | Not ported: no behavior change. |

## Tests

The independent adapter controls cover XPT v5/v8, version 8 observation
counts, long field names, multiple members, numeric missing tags, declared
Unicode decoding, exact narrowing and 1960 epochs, SAS uncompressed/RLE/RDC
storage, records deleted in place, moved records, malformed/truncated input,
compressed-data corruption, and source limits. Files for the ported upstream
changes are assembled at test time from the documented layouts by
`tests/support/sas7bdat.rs` and `tests/support/xport.rs`.
Source fixtures and licenses are cataloged separately; expected values are
never generated with this reader.
