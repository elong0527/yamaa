# Prior-package study pin preparation

The retained packages are in two unpublished drafts. These exact references are
prepared for root review; the public download URLs become usable after approved
publication. The [authenticated receipt](../qualification/rollback-assets.json)
records all eleven downloaded asset identities and byte comparisons from run
38450032850 at `99b4fc28110bceaa346df7a93cf28138f12ec44f`.

| Package and form | Exact source | Retained asset | Fresh reinstall proof |
| --- | --- | --- | --- |
| Prior Python 0.3.0 source distribution | `845a91ff9ba4ab6599efbd79fafe1c4e1dd14255`, prior tag `yamaa-py-0.3.0` | Release 411980272, asset 637944563, `yamaa-0.3.0.tar.gz` | Locked Rust 1.90.0/maturin 1.9.6 rebuild; Linux x86_64 CPython 3.12.14; 135 installed members, three unchanged committed outputs and six held Saves. |
| Prior Python 0.3.0 source-built wheel | Same prior source | Release 411980272, asset 637944633, `yamaa-0.3.0-cp312-abi3-linux_x86_64.whl` | Separate fresh Linux x86_64 CPython 3.12.14 wheel reinstall with the same controls. This is not a previously published binary. |
| Installed pre-cutover R 0.3.0 source package | `a1a9c2d216075f331ac108aa4d3638573f103cea` | Release 411979803, asset 637944247, `yamaa_0.3.0.tar.gz` | One identical archive passed fresh-library domain/check/Define and post-input-removal Saves on Linux, macOS and Windows in run 38435902670. No previous R release was published. |

## Python source pin

Merge this source selection into the compatible prior study project, preserving
its other dependency pins and runtime-artifact declarations:

```toml
[project]
dependencies = ["yamaa==0.3.0"]

[tool.uv.sources]
yamaa = { url = "https://github.com/elong0527/yamaa/releases/download/yamaa-py-reference-845a91ff/yamaa-0.3.0.tar.gz" }
```

The package backend pins maturin 1.9.6 and the source includes its canonical Cargo
lock/toolchain. The prior `uv.lock` is retained as asset 637944703, the exact
baseline dependency versions as asset 637944785, and both reinstall receipts and
output bytes as asset 637944830. Restore the prior calling code and complete
study dependency lock in a fresh environment; update the direct source identity
with `uv lock` and install with `uv sync --locked`. Preserve that regenerated
packaging lock and its source URL with the replay's provenance. Qualification
currently covers CPython 3.12.14 on Linux x86_64.

Verify the installed package before any study input is read:

```python
from importlib.metadata import version

assert version("yamaa") == "0.3.0"
```

## R source pin

The yamaa record prepared for review in a compatible prior `renv.lock` is:

```json
{
  "Package": "yamaa",
  "Version": "0.3.0",
  "Source": "URL",
  "RemoteType": "url",
  "RemoteUrl": "https://github.com/elong0527/yamaa/releases/download/yamaa-r-reference-a1a9c2d2/yamaa_0.3.0.tar.gz"
}
```

Preserve the study's other lock records and R 4.6.1 runtime, restore with renv in
a fresh project library, then verify its installed package path and version
before execution. Source installation requires Rust 1.90.0 and Cargo; Windows
also requires Rtools45 and the `x86_64-pc-windows-gnu` target. The retained
three-platform rehearsal used the exact archive with `R CMD INSTALL` into empty
libraries; complete renv restoration from the published URL remains a separate
check. Its source/run/install/Save/Define receipts are retained as asset
637944307.

```r
stopifnot(identical(as.character(packageVersion("yamaa")), "0.3.0"))
```

Before publication, authorized review can retrieve the exact assets through
`https://api.github.com/repos/elong0527/yamaa/releases/assets/637944563` (Python
source) or `https://api.github.com/repos/elong0527/yamaa/releases/assets/637944247`
(R source), using authenticated release-asset requests
with `Accept: application/octet-stream`. The hosted receipt already verifies
those responses against the approved source bytes. These draft references do
not approve evaluator removal, remaining rollback tuples, B17 budgets or the
combined transition. Prior saved outputs retain their original run provenance;
rollback starts a new whole run with the restored code, locks and artifacts.
