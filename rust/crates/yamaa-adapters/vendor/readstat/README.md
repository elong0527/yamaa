# ReadStat source integration

This directory vendors the MIT-licensed SAS reader subset of ReadStat 1.1.9,
upstream commit `104ba03a8da116eb8c094abc18bc2530b733eda9` from
<https://github.com/WizardMac/ReadStat>, with the later upstream fixes listed
under [Upstream changes after 1.1.9](#upstream-changes-after-119), ported by
hand. The upstream license is in `LICENSE`.
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
rejection, transport physical type/offset and partial-row validation, overlapping RDC
back-reference propagation, refusal of records deleted in place, and exact IBM numeric precision
facts used by narrowing. SAS metadata still records the embedded encoding but
does not override the explicit declaration. The bridge contains snapshot memory
IO callbacks and never initializes filesystem IO. Rust contains every callback
panic before it reaches C. The parent parser lifetime contains all callbacks.

Records deleted in place are refused, never skipped or returned as rows. A data
or mix page whose type carries the deleted-rows flag `0x0080`, checked before
any of the page's subheaders or rows is parsed, or a non-zero
deleted-row count in the row-size subheader (offset 56 in 64-bit files and 28
otherwise, as upstream commit `d19f262e` reads it), stops the parse through the
bridge's `yamaa_readstat_deleted_records` hook. Upstream's deleted-row bitmap
support from that commit and its follow-ups `634f40a8` and `5850fb57` is not
ported. A deleted compressed row keeps its subheader compression code `0x05`,
which remains unsupported compression.

Rust checks the fixed source-byte and table-shape boundaries from the shared
`Limits` policy. The C readers admit declared row/column/cell geometry before
table-sized metadata allocation. Allocation arithmetic and source page/header
extents are checked. Compressed SAS row buffers are allocated only after
physical scalar widths and offsets pass, when columns are submitted for a
compressed row or, if uncompressed rows came first, at the first compressed
row. Snapshot reads and seeks validate their ranges against the held input.
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

Each upstream commit after `104ba03a` that changes a vendored reader is ported
by hand, inside the local changes above, or declined here with its reason.

| Upstream commits | Change | Status |
| --- | --- | --- |
| `365c743e`, `78bb8a94` | A 64-bit subheader's eight-byte signature is compared in full, so a row stored uncompressed in a compressed dataset is not read as metadata when its first four bytes match a signature. Big-endian files keep their four-byte layout. | Ported |
| `da9fcaa4` | A numeric value whose column width is outside 1 to 8 bytes fails instead of shifting by an undefined amount. | Ported |
| `5850fb57` (release field) | The header's release field is copied into a terminated buffer before `sscanf` reads it. The deleted-row part of this commit is listed below. | Ported |
| `a0aa9f31` | RDC decompresses into the dataset's row buffer instead of allocating one per row. | Ported |
| `a2fcaa91` | A dataset with no records reads every page, so column metadata on a later page is not lost. | Ported |
| `3c68974f` | Allocation failures are checked. | Already covered by the local checked allocations |
| `4d3c7f77` | A zero-width character column is accepted; its values are empty text. | Ported |
| `b2d5407d`, `bb29605a`, `95aee731` | The release field accepts revision tags `J` and `TK` for SAS 8 and 9, any tag for other majors such as SAS 7, and the alternative magic number whose first 28 bytes are zero. | Ported |
| `d6ffcec1`, `73756b20` | Encoding code 136 names CP932 and code 38 names ISO-8859-10. The name is recorded metadata only; the declared encoding decodes text. | Ported |
| `fdf4e1e0`, `5c446781`, `4010db5e`, `6f2bcc65`, `6443b805`, `1badba77` | Display format digits, widths and informats are read into each variable's format strings. | Declined: the bridge passes on only names, widths and storage types, and display formats never select types, so these change no result. Reading informat text references would also refuse a file whose unused informat reference is out of range. |

## Tests

The independent adapter controls cover XPT v5/v8, long field names, multiple
members, numeric missing tags, declared Unicode decoding, exact narrowing and
1960 epochs, SAS uncompressed/RLE/RDC storage, records deleted in place,
malformed/truncated input, compressed-data corruption, and source limits.
SAS7BDAT files for the ported upstream changes are assembled at test time from
the documented layout by `tests/support/sas7bdat.rs`.
Source fixtures and licenses are cataloged separately; expected values are
never generated with this reader.
