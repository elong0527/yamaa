# Windows native file transport

This candidate ports the same approved-root resource policy and owned domain
lifecycle to Windows Python. Root/configuration/fallback selection remains the
current contract. The separate declaring-file policy approval is still pending.
Local drive roots are admitted; remote and device namespaces require additional
identity and filesystem qualification. R on Windows remains outside the current
installed matrix and is recorded as unqualified under #1742.

The private Windows adapter uses user-mode
[NtCreateFile](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile)
with a held `RootDirectory` and one counted UTF-16 component. Separators, traversal
components, embedded NUL and alternate-stream syntax are rejected before the
call. Each component opens without reparse traversal. Resource walks check the
file kind, compare opened identities and recheck the complete parent chain.
Canonical entry spelling comes from the held file; Windows ordinal comparison
handles root-case aliases without changing study identifier semantics.

[FILE_ID_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info)
provides the local volume serial and complete 128-bit physical file identifier.
These are OS identities, not content digests. Snapshot equality continues to
compare retained bytes directly and verify every accepted path/base witness.
The existing path, root, snapshot and byte limits remain in force.

Publication retains the selected root and parent handles. A nonblocking named
mutex keyed by the direct physical directory identity coordinates native writers
across processes and sessions; it grants no filesystem authority. Private staging
directories use a protected owner-rights DACL inherited by their children. The
candidate is created exclusively, written, flushed and synchronized through its
held handle. [FILE_RENAME_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info)
replaces one child of the held parent by renaming the held source. Name changes
cannot redirect the source or parent handle. Fallible cleanup preserves observed
foreign entries and retains both operation and cleanup errors. Successful
replacement keeps its successful-save result even if later cleanup fails.

Unsafe SDK calls are confined to the private `windows_file` module, with owned
handles, bounded buffers and a safety argument at each call. Other adapter code
denies unsafe code; core and engine continue to forbid it. The dependency guard
restricts windows-sys 0.61.2 and its exact features to the Windows adapter. Python
and R source packages retain the selected MIT notices for windows-sys/windows-link.

Cross-target checks exercise Windows Rust types and strict Clippy, using host
zstd metadata only for this non-linking check. They cannot establish Windows
execution. Installed Windows wheel and source tests must compare all seventeen
original public reports and successful saves, with all five former file-test
skips removed. The M1 job requires all twelve Windows Python reference/shared-run
tuples for the unchanged six-case cohort and retains six R rows as explicitly
unqualified. Both Unix hosts continue to require all eighteen tuples. The full
945-row reference-assisted inventory remains separate. Complete migration,
broader filesystem support and release acceptance stay open until their gates
are met.
