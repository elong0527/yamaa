# ReadStat source integration

This directory vendors the MIT-licensed SAS reader subset of ReadStat 1.1.9,
upstream commit `104ba03a8da116eb8c094abc18bc2530b733eda9` from
<https://github.com/WizardMac/ReadStat>. The upstream license is in `LICENSE`.
SAS writer/header-construction helpers are omitted from the shared upstream
header reader unit. The adapter exports only a read-only memory bridge and
builds no writer or hash-table code.

`build.rs` compiles this fixed source inventory into a static library through
pinned `cc`. Linux, macOS, MSVC Windows, and MinGW Windows use the same C/Rust
bridge; C owns ReadStat unions and bitfields. No dynamic ReadStat library,
Python/R reader package, generated bindings, configure program, platform iconv,
or network access is needed during package builds. The iconv shim fails closed;
Rust strictly decodes the explicitly declared source encoding.

Local changes are limited to removal of writer helpers, the integration bridge
and iconv shim, checked
allocation initialization, byte-preserving conversion rejecting embedded NUL,
source-authority encoding metadata treatment, early fixed table-shape
admission, SAS physical extent validation, source-extent bounds on retained
text-blob copies, unsupported compression signature
rejection, transport physical type/offset and partial-row validation, overlapping RDC
back-reference propagation, and exact IBM numeric precision
facts used by narrowing. SAS metadata still records the embedded encoding but
does not override the explicit declaration. The bridge contains snapshot memory
IO callbacks and never initializes filesystem IO. Rust contains every callback
panic before it reaches C. The parent parser lifetime contains all callbacks.

Rust checks the fixed source-byte and table-shape boundaries from the shared
`Limits` policy. The C readers admit declared row/column/cell geometry before
table-sized metadata allocation. Allocation arithmetic and source page/header
extents are checked. Compressed SAS row buffers are allocated only after
physical scalar widths and offsets pass. Snapshot reads and seeks validate
their ranges against the held input.
The SAS header/page sizes are bounded by the captured snapshot framing, rather
than a separate 16 MiB threshold. Their minimum framing sizes still apply.
Text-blob count follows the 65,536 slots addressable by source uint16 references.
Their cumulative retained bytes cannot exceed the captured file length, including
copies reached through repeated or overlapping subheader pointers.
A small independently repacked haven fixture with 4,097 valid text
blobs preserves all twenty numeric cells and is accepted by the independent
pandas SAS reader; no lower 4,096-blob ceiling is applied.

The independent adapter controls cover XPT v5/v8, long field names, multiple
members, numeric missing tags, declared Unicode decoding, exact narrowing and
1960 epochs, SAS uncompressed/RLE/RDC storage, malformed/truncated input,
compressed-data corruption, and source limits. Source fixtures and licenses
are cataloged separately; expected values are never generated with this reader.
