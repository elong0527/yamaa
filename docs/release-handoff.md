# Shared Rust release handoff

Current owner: [#2288](https://github.com/elong0527/yamaa/issues/2288), the
2026-10-14 remaining release checklist. It supersedes the migration roll-ups
referenced by historical receipts below. The remaining cleanup and assertion
migration belong to [#2289](https://github.com/elong0527/yamaa/issues/2289).

Prepared on 2026-10-13 (America/New_York) for
[#2147](https://github.com/elong0527/yamaa/issues/2147), using main
`1b795ca4c1cbc742233ba459c1b987ee4b88fd73` and the original evidence identified below. The current
[release guide](release.md) owns policy and the B01-B19 register. This handoff
owns final evidence reconciliation and the package selection for release.

Reconciled on 2026-10-14 (UTC) against main
`4bff6da202bdddbfd4955917873e5ee5e4650d9a`. The preparation source and historical
evidence below retain their original identities; this reconciliation is not a
new qualification result.

**Final release evidence is pending.** No final qualified source, completed
scoped Linux performance measurements or publishable 0.4.0 package selection is
asserted here. The maintainer sets no performance pass/fail budgets for 0.4.0.
Removal and coordinated 0.4.0 version changes already merged before final
item-10/item-11 acceptance. The delegated decision authority in
[#1973](https://github.com/elong0527/yamaa/issues/1973) permits evidence-backed
acceptance after the gates pass; it does not supply missing evidence or authorize
release tagging, package publication or asset replacement.

## Final acceptance reconciliation

| Historical gate / transition entry | Required final evidence | Current disposition |
| --- | --- | --- |
| Fixtures, language, workflow and APIs; B01-B12/B16 | All current directory-derived active cases in every installed tuple, two declared historical exclusions, retained host APIs/helpers and error/value/ownership/Save contracts. Independently committed truth remains unchanged. Unique retired assertions require qualified public or typed-native replacements. | Implementation merged; exact final-source installed renewal and retirement reconciliation pending under #2288/#2289. |
| Installed matrix, numerical behavior and reproducible packages; B12-B15 | 18 CPython 3.12/3.13/3.14 wheel/source tuples plus three R 4.6.1 source tuples, strict R `Status: OK`, three shared Rust checks with engine test-support, nine math assessments and their exact comparison. Six package builds must preserve canonical workspace/lock/toolchain bytes. | Final combined-main native audit and original-input independent replay pending under #2288. A successful workflow alone is insufficient. |
| Performance and capacity; B17-B18 | Approved longitudinal bounds and paired 100-column, 50,000-record Build/Save controls; 27 authored cases on Linux Python 3.12 wheel and R 4.6.1 source, independently audited for that scope. Each case has ten cold samples, three retained warmups and thirty warm samples. | Harness and auditor completed under #2141/#2143. Capacity implementation in PRs #2365/#2367 and paired local Python and hosted Linux R installed controls at `983e9c96` are recorded below. Final combined-main renewal and measured latency/RSS remain under #2288; no numeric budgets are set for 0.4.0. |
| Transition and rollback; B19 | Exact prior artifacts and complete fresh reinstall matrix; version checks before study effects; explicit item-11 decision linked to all passing entry gates. | Historical 18-tuple Python and three-host R raw receipt replay passed as described below. Final item-11 acceptance remains pending. |
| Removal; B03/B05/B13 | Retained API, assertion replacements, package/archive membership and dependency/validator locks reconcile with final installed contracts. | Removal is implemented. Its chronology does not establish entry-gate approval; final reconciliation remains pending. |
| Exact source/run/attempt/artifact handoff; B15/B19 | Final native source and parents, original Actions run/attempt/artifact identities, actual package names/bytes and independent replay; measured performance revision and policy bindings remain explicit. | Final source and selected release bytes are pending. Publication is separate. |

Earlier source `2ba10418607933f92597f7c00ac363a3113a49db`,
[run 38416920074](https://github.com/elong0527/yamaa/actions/runs/38416920074),
attempt 1, supports the retained public ratchet at that source. Main source
`89b1fa90b0edfa03bc367f73426b630b36b20e85`, [run 38497098490](https://github.com/elong0527/yamaa/actions/runs/38497098490),
attempt 1, retained complete public inventories but its independent native audit
was incomplete: 42 passing, 49 failing and six missing lanes. Neither observation
qualifies the eventual combined-main source. Preserve those original findings.

The historical
[Python retirement inventory](https://github.com/elong0527/yamaa/blob/43bb4437d9c0890ec7877ec7575a517c771a8147/rust/planning/python-retirement-inventory.json)
maps 77 retiring test files and 1,193 named tests to dispositions and 72 retained
proof paths. All 72 paths remain present at the preparation source. This preserves
source-level review provenance; path existence does not establish assertion
equivalence or installed qualification. Final admission still checks the mapped
public/typed-native behavior through the retained supplemental inventory, source
guards, package hygiene, complete independent conformance and native tests.
The historical
[validator disposition](https://github.com/elong0527/yamaa/blob/43bb4437d9c0890ec7877ec7575a517c771a8147/rust/planning/validator-assertion-disposition.json)
names 420 original controls. All 420 names remained among 426 named controls at
the preparation source after #2171. At the reconciliation source, 410 original
names remain among 426 named controls.
Nine execution-manifest controls and the former execution-manifest blocker-loader
control retired with that input. Six derived benchmark-inventory controls and the
validation-only blocker-loader control exercise its current successors; nine
further controls cover native static admission, aggregate bindings and example
index validation. These 16 additions do not establish equivalence for every
retired assertion. Final #2288/#2289 admission still reconciles the required
behavior and renewed native routing against installed evidence.

### Candidate implementation and scoped installed controls

PRs [#2365](https://github.com/elong0527/yamaa/pull/2365) and
[#2367](https://github.com/elong0527/yamaa/pull/2367) implement the longitudinal
default and actual native usage pools. Scoped installed-host checks at source
[`983e9c96`](https://github.com/elong0527/yamaa/commit/983e9c9687a1a49a7d283fe70e681196eef7bd8f)
passed all 18 Python supplemental suites locally and all 17 R supplemental
suites and 319 R conformance cases in the hosted Linux R 4.6.1 native job.
Both hosts passed the paired 100-column, 50,000-record Build/Save controls.
These suite counts are scoped checks at that
source, not the required 18-tuple Python and three-tuple R release matrix.

The strict R package check at that earlier source reported one NOTE. The packaging
repair at
[`7c04a026`](https://github.com/elong0527/yamaa/commit/7c04a026996e062ec3949bb6e71fcec14c85bbe8)
was queued for renewed checking at that stage; it does not change the source
identity of those installed results. The later `7c04a026` native run stalled in
the cohort job and is not a passing qualification run.

The 2026-10-15 candidate qualification uses source
[`46ee4dcb0f7ffe5cd8a95b477d242cfadcf0c689`](https://github.com/elong0527/yamaa/commit/46ee4dcb0f7ffe5cd8a95b477d242cfadcf0c689),
[native run 38657181937](https://github.com/elong0527/yamaa/actions/runs/38657181937),
attempt 1. All 31 required jobs passed; one optional job was skipped. This is
`reviewed-branch` qualification, with the following installed and independent
evidence:

| Candidate gate | Passing evidence at `46ee4dcb` |
| --- | --- |
| Python installed matrix | All 18 CPython 3.12/3.13/3.14 wheel/source tuples across Linux x86_64, macOS arm64 and Windows x64; each passed 319 active conformance cases and all 18 supplemental suites, retaining two explicit historical exclusions. |
| R installed matrix | All three R 4.6.1 source tuples on the same hosts; each passed 319 active conformance cases and all 17 supplemental suites, retaining the same two exclusions. All three strict package checks report `Status: OK`. The earlier `983e9c96` NOTE remains a prior observation. |
| Shared Rust and numerical behavior | All three shared Rust checks, all nine numerical assessments and their exact comparison passed. The numerical evidence establishes the assessed sample's portability, not universal correct rounding or historical bit parity. |
| Complete public and producer cohorts | All 18 tuple directories passed, retaining 468 case/host comparison records against committed truth and original held Save bytes. |
| Independent native audit | All 97 lanes passed with `complete: true`. The original hosted network-isolated replay reproduced the verdict byte for byte. |

The original audit is owned by
[job 116178061111](https://github.com/elong0527/yamaa/actions/runs/38657181937/job/116178061111).
Its small original record
[artifact 11938143736](https://github.com/elong0527/yamaa/actions/runs/38657181937/artifacts/11938143736)
retains both the verdict and its byte-identical hosted replay. Independent local
`--input`/`--check` replay of authenticated reconstituted original inputs also
passed all 97 lanes and reproduced those verdict bytes. That reconstruction
retained all 86 original advertised artifacts and 19 exact Git source files;
it did not download the full audit ZIP. The original small metadata and source,
run, attempt and producing-job identities remain unchanged.

Linux performance
[run 38657181920](https://github.com/elong0527/yamaa/actions/runs/38657181920)
uses attempt 1 at frozen source `46ee4dcb0f7ffe5cd8a95b477d242cfadcf0c689`.
The installed Python 3.12 wheel collector passed; its original collection is
[artifact 11943075352](https://github.com/elong0527/yamaa/actions/runs/38657181920/artifacts/11943075352),
produced by
[job 116178873002](https://github.com/elong0527/yamaa/actions/runs/38657181920/job/116178873002).
It uses the exact Linux candidate wheel from native artifact `11936644695`
listed below. The R collector and independent 54-cell coverage audit remain
pending. No complete measured result is asserted here. Record reviewed
latency/RSS in the [release guide](release.md#qualification-and-retained-records)
after that gate passes, preserving the measured revision and each original
producing job, artifact, package and attempt identity.

The later PK/QT candidate
[#2376](https://github.com/elong0527/yamaa/pull/2376), source
`3baef533a182b2bd707060a3b23854f39e17e3ae`, expands the authored inventory to
329 cases, 327 active and two excluded. Its final installed observations are
pending. Keep the 319-active-case candidate results above at their original
source; coverage of newly authored cases requires fresh evidence.

Final postmerge main/native qualification and its independent audit/replay,
remaining assertion migration under #2289, Linux measurements, exact release
package/tag selection and publication remain pending. Candidate qualification
does not establish final release qualification or item-11 acceptance and does
not close #2288.

### Source differences and evidence renewal

Record the campaign's measured source, collection run/attempt, original artifact
IDs, collector/workload interfaces and effective capacity policy. Record the
declared Linux scope and observed latency/RSS without numeric budgets. Later
documentation or decision-record commits
must keep the measured revision; do not replace it with the later main SHA.

For any source difference, inspect the complete Git diff and compare actual
source-package membership and bytes. Establish unchanged effective Rust/host
runtime, Python/R dependencies and locks, source staging/build configuration,
toolchain/targets, normative rules and independent workload truth, workload
generation, baseline behavior, measurement collectors/providers and runner
interfaces. A filename classification or unchanged version is insufficient.
Record the compared revisions, changed paths and equality evidence in the final
review. Renew all affected evidence when one of those effective inputs changes.
Regardless of performance equivalence, the final native audit must genuinely
qualify the final combined-main source and package bytes.

Prior Python lookup/aggregate/window lifecycle baselines are comparable only on
the same Python tuple and machine. The prior callback baseline is unavailable;
its producer baseline measures root Build, so full graph Build/Save/RSS ratios
are incomparable. Python comparator observations for R remain cross-host
observations. These reasons remain visible in the campaign and budget decision;
they cannot become passing regression ratios. The candidate still requires its
complete correctness and measurement coverage.

## Independently re-admitted rollback

The original receipts and exact asset identities are in
[rollback-assets.json](https://github.com/elong0527/yamaa/blob/main/rust/qualification/rollback-assets.json).
All release metadata below was rechecked through authenticated GitHub reads on
the preparation date. Published Python 0.3.0 release 409220247 has no attached
package assets. No shared 0.4.0 release is listed. The retained packages are
unpublished drafts; their public release-download URLs are future pins.

| Retained selection | Source and original rehearsal | Durable draft identity |
| --- | --- | --- |
| Python 0.3.0 source-built packages: Linux x86_64, macOS arm64 and Windows x64 wheels; independently rebuilt source on CPython 3.12-3.14 | Published prior source `845a91ff9ba4ab6599efbd79fafe1c4e1dd14255`; [run 38460509078](https://github.com/elong0527/yamaa/actions/runs/38460509078), attempt 1, preparation `7acd9bce2fd7943083ad9afe748d31a97fe97aef`. These wheels were not previously published binaries. | Release 411980272: source asset 637944563; Linux wheel 637944633; macOS wheel 638657539; Windows wheel 638657631; original 22-archive bundle 638657402; inventory 638657299; prior lock 637944703; dependency pins 637944785. |
| One identical R 0.3.0 pre-cutover source archive on all three supported hosts | Source `a1a9c2d216075f331ac108aa4d3638573f103cea`; [run 38435902670](https://github.com/elong0527/yamaa/actions/runs/38435902670), attempt 1, preparation `d909928c94f308550c7f584c12b6b42506c4c84c`. No prior R release was published. | Release 411979803: source asset 637944247, exactly 3,290,128 bytes; original reinstall receipts asset 637944307; retention manifest 637944356. |

Independent review downloaded every original Python ZIP listed in
[prior-python-rollback-run.json](https://github.com/elong0527/yamaa/blob/main/rust/qualification/prior-python-rollback-run.json)
and all three original R rehearsal ZIPs (11832953460, 11837325176, 11834482773).
The Python audit used the unchanged `audit.py`/`replay.py` at preparation
`7acd9bce`, with Polars 1.44.2 and truth read directly from Git source `845a91ff`.
All 18 tuple receipts, 54 domain outputs and 108 held Saves passed the original
raw-table/issue/ownership/version/build admission and exact committed CSV-byte
comparisons. Every retained prior `python/uv.lock` equals its original source.

The three R archives are byte-for-byte identical. Each retains a fresh R 4.6.1
library/version-before-study receipt, full-range integer and leading-zero CSV,
unchanged previously saved candidate bytes, and Define JSON/XML equal to the
archive's goldens and original source truth. The archive's Cargo workspace,
lock and toolchain bytes equal the original canonical source. The preparation
receipt's historical `rehearsal: pending` value is preserved; completed rehearsal
evidence is the separate retained result, logs and raw output bytes.

Latest authenticated retention
[run 38489655777](https://github.com/elong0527/yamaa/actions/runs/38489655777),
attempt 1, source `85b20ec0a229f2b376d0449c387c366947a45e19`, artifact
11860743673, re-admitted the complete original Python campaign and reused all
retained additions without overwrite. Current metadata preserves all twelve
Python and three R asset IDs. Original durable byte-equality proof remains
[run 38450032850](https://github.com/elong0527/yamaa/actions/runs/38450032850).
This review replayed existing raw receipts; it did not repeat valid package
rehearsals or publish drafts.

### Fresh whole-run restoration

Stop the application and discard outstanding handles. Restore compatible prior
calling code, complete study locks and runtime/input/resource snapshots in a
fresh environment, then select the exact prior artifacts above. Authenticated
draft retrieval uses `Accept: application/octet-stream` and the asset's API URL,
for example `https://api.github.com/repos/elong0527/yamaa/releases/assets/637944563`.
Retain actual package bytes and receipts; keep credentials and temporary signed
redirects outside locks.

For Python, preserve the study's prior dependencies and `yamaa==0.3.0`, select the
downloaded archive with a local `[tool.uv.sources]` path, run `uv lock`, preserve
both original/restored locks, then install with `uv sync --locked`. Source builds
require Rust 1.90.0 and maturin 1.9.6 with the archive's canonical Cargo lock.
For R, restore the compatible prior `renv.lock` into a fresh project library and
install the exact retained source archive there. R requires 4.6.1, Rust 1.90.0
and Cargo; Windows also requires Rtools45 and `x86_64-pc-windows-gnu`.

Before any study input, callback or build, assert Python distribution version
`importlib.metadata.version("yamaa") == "0.3.0"`, or R
`identical(as.character(packageVersion("yamaa")), "0.3.0")`, and verify its
installed path belongs to the fresh environment/library. Start a new whole run.
Saved outputs retain their original provenance and destinations; installation
does not undo earlier writes. Particular complete study locks and future public
URL restoration remain operator/publication checks. The representative rehearsal
does not establish a full prior producer-graph Save baseline.

## Exact 0.4.0 package selection

Final package selection awaits the passing combined-main run. Source manifests
currently declare Python, R and embedded Cargo workspace 0.4.0. Record actual
filenames, lengths and original artifact IDs from that run; preserve and compare
the tested bytes when preparing publication. Rebuilding creates new artifacts
and requires renewed admission.

The qualified candidate's six original Actions artifacts belong to source
`46ee4dcb0f7ffe5cd8a95b477d242cfadcf0c689`, native run `38657181937`, attempt 1.
The table records the actual package filenames and lengths inside those
artifacts. Each platform's source archive keeps its own tested bytes; these
candidate references are not a final release selection or published assets.

| Candidate package | Exact original package reference | Package bytes | Original Actions artifact |
| --- | --- | ---: | --- |
| Linux x86_64 Python wheel | `github-actions:38657181937/attempt-1/native-python-ubuntu-24.04-3.14/yamaa-0.4.0-cp312-abi3-manylinux_2_34_x86_64.whl` | 8,372,695 | [11936644695](https://github.com/elong0527/yamaa/actions/runs/38657181937/artifacts/11936644695) |
| Linux Python source | `github-actions:38657181937/attempt-1/native-python-ubuntu-24.04-3.14/yamaa-0.4.0.tar.gz` | 2,319,729 | [11936644695](https://github.com/elong0527/yamaa/actions/runs/38657181937/artifacts/11936644695) |
| macOS arm64 Python wheel | `github-actions:38657181937/attempt-1/native-python-macos-15-3.14/yamaa-0.4.0-cp312-abi3-macosx_11_0_arm64.whl` | 7,369,996 | [11937203956](https://github.com/elong0527/yamaa/actions/runs/38657181937/artifacts/11937203956) |
| macOS Python source | `github-actions:38657181937/attempt-1/native-python-macos-15-3.14/yamaa-0.4.0.tar.gz` | 2,320,515 | [11937203956](https://github.com/elong0527/yamaa/actions/runs/38657181937/artifacts/11937203956) |
| Windows x64 Python wheel | `github-actions:38657181937/attempt-1/native-python-windows-latest-3.14/yamaa-0.4.0-cp312-abi3-win_amd64.whl` | 7,534,978 | [11937900083](https://github.com/elong0527/yamaa/actions/runs/38657181937/artifacts/11937900083) |
| Windows Python source | `github-actions:38657181937/attempt-1/native-python-windows-latest-3.14/yamaa-0.4.0.tar.gz` | 2,320,174 | [11937900083](https://github.com/elong0527/yamaa/actions/runs/38657181937/artifacts/11937900083) |
| Linux x86_64 R source | `github-actions:38657181937/attempt-1/native-r-ubuntu-24.04/yamaa_0.4.0.tar.gz` | 3,591,231 | [11937604220](https://github.com/elong0527/yamaa/actions/runs/38657181937/artifacts/11937604220) |
| macOS arm64 R source | `github-actions:38657181937/attempt-1/native-r-macos-15/yamaa_0.4.0.tar.gz` | 3,590,120 | [11937463740](https://github.com/elong0527/yamaa/actions/runs/38657181937/artifacts/11937463740) |
| Windows x64 R source | `github-actions:38657181937/attempt-1/native-r-windows-latest/yamaa_0.4.0.tar.gz` | 3,590,728 | [11937549706](https://github.com/elong0527/yamaa/actions/runs/38657181937/artifacts/11937549706) |

| Selection | Required final record |
| --- | --- |
| Linux x86_64 Python wheel and source distribution | `native-python-ubuntu-24.04-3.14` original artifact; actual wheel/source names and bytes; qualified CPython 3.12/3.13/3.14 wheel/source reinstall evidence. |
| macOS arm64 Python wheel and source distribution | `native-python-macos-15-3.14` original artifact and the same exact platform/form records. |
| Windows x64 Python wheel and source distribution | `native-python-windows-latest-3.14` original artifact and the same exact platform/form records. |
| Staged standalone R source package | Select the exact tested archive and its original `native-r-*` artifact. Record each host's build/installation/check identity and compare archive contents/bytes before asserting that one archive represents every host. |
| Shared provenance | Final source and parents, native run/attempt, package/runtime versions, runner images/architectures, Rust 1.90.0/Cargo, build backend, canonical Cargo workspace/lock and Python dependency-lock identities, membership/byte comparisons and independent audit/replay. |

Use the [Python installation/API guide](https://github.com/elong0527/yamaa/blob/main/python/README.md)
and [R source installation/API guide](https://github.com/elong0527/yamaa/blob/main/R/yamaa/README.md).
Migration uses Python `domain`/`check`/`define` and R
`yamaa_domain`/`yamaa_check`/`yamaa_define`; retain supported ODM helpers and style
CLI. Retired evaluator/probe/authoring routes have no runtime fallback. Review
the release guide's PortableLibmV1 compatibility note before opting in: EXP/LN/
POWER bits and dependent predicates, conversions and saved bytes may differ from
historical Python platform behavior. Restore an entire prior release for rollback.

Remaining release evidence belongs to #2288: complete postmerge native audit
and independent original-input replay, final-source numerical qualification and
renewed installed capacity controls, scoped Linux performance observations,
release notes and actual tested package selection. The remaining original
assertion migration and restoration qualification belong to #2289. Earlier branch passes and
historical rollback re-admission do not supply the final combined-main results.
The 17 raw measurement files were deleted by #2345 under the maintainer's finite
disposition; Git retains their bytes and the old capacity-input obligation is
abandoned.

Record the final source commit, native run/attempt, audit/replay artifacts and
selected package names/bytes here only after their gates pass. Close the current
checklists when their criteria are met; superseded trackers stay closed.
Package/tag publication remains a separate maintainer action.

