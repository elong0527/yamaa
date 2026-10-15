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
source-authority encoding metadata treatment, early row/column/page/header-work
admission, SAS physical extent validation, unsupported compression signature
rejection, transport physical type/offset and partial-row validation, overlapping RDC
back-reference propagation, and exact IBM numeric precision
facts used by narrowing. SAS metadata still records the embedded encoding but
does not override the explicit declaration. The bridge contains bounded memory
IO callbacks and never initializes filesystem IO. Rust contains every callback
panic before it reaches C. The parent parser lifetime contains all callbacks.

The shared decoder receipt route also admits cumulative snapshot bytes read
before copying and returns the actual admitted work charge. Independent C
buffer/header stages reuse the allowance, so their successful highwater and the
cumulative read charge combine by maximum. Rust adds its admitted retention
charge to the receipt using the same maximum rule. These are the enforced
logical quota charges, not measurements of allocator usage, RSS, or CPU time.
The existing public parser keeps its separate snapshot IO policy.

Native allocation admission uses the supplied work allowance, with checked
arithmetic and source page/header extents validated first. Compressed SAS row
buffers are allocated only after row/cell bounds and physical scalar widths
and offsets pass. IO operation admission derives from the bounded snapshot
read budget, row/cell allowance, column allowance, and fixed framing overhead;
there is no lower fixed million-operation or 16 MiB allocation ceiling.
The SAS header/page sizes are bounded by the captured snapshot framing, rather
than a separate 16 MiB threshold. Their minimum framing sizes still apply.
Text-blob count follows the 65,536 slots addressable by source uint16 references;
cumulative blob bytes and both metadata arrays still pass work admission before
allocation. A small independently repacked haven fixture with 4,097 valid text
blobs preserves all twenty numeric cells and is accepted by the independent
pandas SAS reader; no lower 4,096-blob ceiling is applied.

The independent adapter controls cover XPT v5/v8, long field names, multiple
members, numeric missing tags, declared Unicode decoding, exact narrowing and
1960 epochs, SAS uncompressed/RLE/RDC storage, malformed/truncated input,
compressed-data corruption, and source limits. Source fixtures and licenses
are cataloged separately; expected values are never generated with this reader.
