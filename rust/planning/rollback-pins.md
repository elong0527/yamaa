# Prior-package study pin preparation

The retained packages are in two unpublished drafts. These exact references are
prepared for root review; the public download URLs become usable after approved
publication. The [authenticated receipt](../qualification/rollback-assets.json)
records the original eleven downloaded asset identities and byte comparisons
from run 38450032850 at `99b4fc28110bceaa346df7a93cf28138f12ec44f`, plus the
complete prior-Python matrix retention in
[run 38488955022](https://github.com/elong0527/yamaa/actions/runs/38488955022)
at `6b88f6dba0023cb09f630487e00e6adb548f33da`.

| Package and form | Exact source | Retained asset | Fresh reinstall proof |
| --- | --- | --- | --- |
| Prior Python 0.3.0 source distribution | `845a91ff9ba4ab6599efbd79fafe1c4e1dd14255`, prior tag `yamaa-py-0.3.0` | Release 411980272, asset 637944563, `yamaa-0.3.0.tar.gz` | Locked Rust 1.90.0/maturin 1.9.6 rebuild; Linux x86_64 CPython 3.12.14; 135 installed members, three unchanged committed outputs and six held Saves. |
| Prior Python 0.3.0 source-built wheel | Same prior source | Release 411980272, asset 637944633, `yamaa-0.3.0-cp312-abi3-linux_x86_64.whl` | Separate fresh Linux x86_64 CPython 3.12.14 wheel reinstall with the same controls. This is not a previously published binary. |
| Prior Python 0.3.0 macOS arm64 wheel | Same prior source archive | Release 411980272, asset 638657539, `yamaa-0.3.0-cp312-abi3-macosx_11_0_arm64.whl` | This exact wheel passed fresh CPython 3.12/3.13/3.14 reinstalls in run 38460509078. |
| Prior Python 0.3.0 Windows x64 wheel | Same prior source archive | Release 411980272, asset 638657631, `yamaa-0.3.0-cp312-abi3-win_amd64.whl` | This exact wheel passed fresh CPython 3.12/3.13/3.14 reinstalls in run 38460509078. |
| Complete prior Python rollback receipts and platform-wheel bytes | Run 38460509078, attempt 1, `7acd9bce2fd7943083ad9afe748d31a97fe97aef` | Release 411980272, bundle asset 638657402 and inventory asset 638657299 | All eighteen raw reinstall receipts, three exact platform-wheel build artifacts and the complete summary were independently re-admitted and preserved as unchanged original ZIP bytes. |
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
packaging lock and its source URL with the replay's provenance. The original
retained Linux wheel was built on CPython 3.12.14. The complete
[prior-Python rehearsal](https://github.com/elong0527/yamaa/actions/runs/38460509078)
subsequently passed all eighteen fresh reinstalls: CPython 3.12/3.13/3.14 on Linux
x86_64, macOS arm64 and Windows x64, each using either its retained platform
wheel or an independent rebuild of the same exact source archive. The
[reviewed original artifact inventory](../qualification/prior-python-rollback-run.json)
pins the source run, attempt, preparation revision and all 22 artifact identities.
Each lane retained raw tables, issues, source/build/version records and six held
Saves equal to independently committed lookup, aggregate and window outputs.

The rebuilt macOS and Windows wheels are
`yamaa-0.3.0-cp312-abi3-macosx_11_0_arm64.whl` and
`yamaa-0.3.0-cp312-abi3-win_amd64.whl`. They implement the actual prior Python
source revision; they are not previously published binary releases. The
retained bundle `prior-python-rollback-38460509078.zip` preserves
every original Actions ZIP byte, including all eighteen tuple receipts, three
platform-wheel build artifacts and the complete summary. Its companion
`prior-python-rollback-38460509078.json` records their immutable identities and
re-admitted lane inventory. Hosted retention added these four assets to the same
unpublished Python draft, preserved all eight existing asset identities, and
verified authenticated downloaded-byte equality for each addition. The
rollback-assets.json receipt records these actual proofs. No draft was published.

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
not approve evaluator removal, study-specific lock restoration, B17 budgets or the
combined transition. Prior saved outputs retain their original run provenance;
rollback starts a new whole run with the restored code, locks and artifacts.

## Authenticated preparation while drafts remain unpublished

Public release-download URLs above are future publication pins. Authorized
reviewers can retrieve the exact durable source packages now:

```sh
mkdir retained-prior-packages
gh api repos/elong0527/yamaa/releases/assets/637944563 \
  --header 'Accept: application/octet-stream' \
  > retained-prior-packages/yamaa-0.3.0.tar.gz
gh api repos/elong0527/yamaa/releases/assets/637944247 \
  --header 'Accept: application/octet-stream' \
  > retained-prior-packages/yamaa_0.3.0.tar.gz
```

Use an authenticated GitHub identity authorized to read unpublished drafts.
Preserve these exact API references and receipt records with the local package
bytes. Keep credentials and temporary redirected download URLs outside study
locks. Python source installation requires Rust 1.90.0 and maturin 1.9.6; use the
retained source package's unchanged Cargo lock. On a new compatible prior study
checkout, select a local direct source until publication:

```toml
[tool.uv.sources]
yamaa = { path = "retained-prior-packages/yamaa-0.3.0.tar.gz" }
```

Keep `yamaa==0.3.0`, the study's prior calling code and all other dependency pins.
Regenerate the study lock for that selected archive with `uv lock`, then restore
it with `uv sync --locked`. Keep both the original and restored study locks.
The hosted rehearsal used fresh virtual environments with all seventeen exact
baseline dependency versions before installing either the platform wheel or the
independently source-built wheel. Its evidence establishes that package replay;
restoration of a particular complete study lock remains study-specific.
For R review, install the fetched archive into an empty library with
`R CMD INSTALL --library=<fresh-library> retained-prior-packages/yamaa_0.3.0.tar.gz`.
The checked-in R rollback workflow retrieves exact durable draft asset 637944247
directly, verifies its release/source/package identities and rehearses all three
hosts. It no longer requires an unexpired original Actions artifact.
