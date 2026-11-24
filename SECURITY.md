# Security policy

## Reporting a vulnerability

Report a vulnerability privately, through GitHub's private vulnerability
reporting: open the repository's **Security** tab and choose **Report a
vulnerability**, or go to
<https://github.com/elong0527/yamaa/security/advisories/new>. Do not open a
public issue, discussion or pull request for it: everything there is public.

A vulnerability here is a way for a file or an argument to make yamaa do
what it should not, such as a crash, a memory-safety fault, a hang or
unbounded memory use, a read or write outside the paths a run declares, or
code run that no specification calls. yamaa reads untrusted `sas7bdat`, XPT,
Parquet, ODM XML, TAR and YAML, part of it in vendored C code, so a fault
found by reading such a file is in scope. A wrong derived value is a bug:
report it as a public issue.

Include the yamaa version (`yamaa --version`), the package and platform, the
smallest input that shows the fault, and what happens. Use invented data
only; never attach study data.

## Supported versions

| Version | Supported |
| --- | --- |
| The latest release on GitHub and PyPI | Yes |
| Earlier releases | No |

A fix is made on `main` and ships in the next release.

## What to expect

The maintainer answers on the private advisory, where the report stays while
it is confirmed and fixed. Once a release carries the fix, the advisory is
published with the affected and fixed versions, and credits the reporter
unless they ask otherwise.
