# Representative public performance evidence

The committed [raw frontier record](1998-frontier-2026-10-13.json) retains actual
clock boundaries, child peak RSS, refusals and origins for installed Linux
x86_64 / CPython 3.12.14 wheels. It is functional evidence, not an approved
10-cold/30-warm distribution or release qualification. Candidate source is
`a1a9c2d216075f331ac108aa4d3638573f103cea`, from the successful
[native run 38394553441](https://github.com/elong0527/yamaa/actions/runs/38394553441).
The previous release is tag `yamaa-py-0.3.0`, source
`845a91ff9ba4ab6599efbd79fafe1c4e1dd14255`.

## Observations

The candidate completes exact independent output and both held Saves for the
1,000-row aggregate workloads (group widths 10 and 100) and callback workload.
All 10,000/100,000-row cases refuse. The corrected producer workload now has a
key-correlated, ordered first-match intermediate, preserving independently
computed outputs; its remaining refusals are engine findings, not invalid
rename-only declarations. Refused Builds and false/throwing Saves are retained.
Their durations never become successful samples.

| Actual accepted frontier | Child lifetime peak RSS |
| --- | ---: |
| Candidate aggregate, 1,000 rows / width 10 | 66,617,344 bytes |
| Candidate aggregate, 1,000 rows / width 100 | 65,351,680 bytes |
| Candidate callback, 1,000 rows | 78,036,992 bytes |
| Previous Python reference lookup, 100,000 rows / multiplicity 4 | 528,199,680 bytes |
| Previous Python reference aggregate, 100,000 rows / width 10 | 157,077,504 bytes |
| Previous Python reference window, 100,000 rows / partition width 100 | 1,345,900,544 bytes |

The previous release passes all 18 lookup/aggregate/window variants at the three
approved sizes against independent typed cells, float bits and both original CSV
Saves. Its public reference API does not accept `environment`; nine callback and
producer cases retain the resulting `TypeError`. No baseline ratio is available
for those families. The reference wheel also needs its supported explicit
`schema_root` argument; the collector retains the release's original YAML bytes
outside the checkout rather than substituting candidate schema content.

The 1,000-row callback observer records 859 actual delegated invocations: the
two present activation-test arguments and 857 present study arguments, in exact
independent argument/result order. No missing argument reaches the callable.
Ordinary and observed output, both Saves and complete reports agree after only
fresh-directory identities are normalized. The wrapper has the installed
callable's admitted signature and delegates once. Activation/derivation stage
counts and lock/binding counts remain unavailable. Each successful repetition
observes two output property accesses and one access to each issue/log property.
These accessor counts do not estimate internal allocations or copies.

Memory uses the candidate child's `wait4` high-water result on Linux/macOS and
`GetProcessMemoryInfo` / `PeakWorkingSetSize` on Windows. A small, newly executed
broker launches the measured process: a large workload controller's RSS would
otherwise survive fork/exec in the child high-water observation. An actual
192 MiB parent-allocation control verifies isolation, and an actual 64 MiB child
allocation verifies measurement and nonzero exit preservation. Raw observations
retain original units. Warm-child RSS is cumulative across repetitions and
includes collector transport. Supported-platform provider failures stay null
and reject admission. R and Windows implementations still require CI execution.

## Concrete resource-policy proposal for review

Size approval does not approve quota increases. The following is a bounded
proposal, not an implemented or approved policy:

| Resource | Approved workload requirement | Proposed ceiling |
| --- | --- | --- |
| Source rows | Lookup multiplicity 4 has 342,856 donor rows at 100k driving rows. | 524,288 per source |
| Captured source cells | That donor has 1,028,568 cells, plus 100,000 base cells. | 2,097,152 per execution |
| Output rows/cells | Windows have 100,000 rows and 700,000 output cells. | 131,072 rows / 1,048,576 cells |
| Key/identity cells | 100k driving identities; donor key preparation exceeds 262,144. | 1,048,576 cells, with 8 MiB retained identity text |
| Retained output text | Lookup ID/text and window partition identities exceed 1 MiB. | 8 MiB |
| Execution work | Existing ordinary keyed selectors scan all donors for each current row. | Keep existing work refusal until indexed selection is qualified; then review a bounded 67,108,864-work-unit ceiling. |
| Report projection | The 16 MiB conservative projection charge blocks complete public results before actual encoded report bytes reach 16 MiB. | Review actual retained/encoded allocation charges and aggregate graph accounting before selecting a ceiling. Do not mechanically raise it to GiB. |

The current `dataset_intermediates.rs` ordinary selection loop performs about
34.3 billion donor visits for the largest lookup and about 342.8 million at 10k.
Raising the work allowance to accommodate this scan would permit quadratic
execution. Index ordinary eligible keyed donors while preserving source filters,
donor order, missingness, correlated checks and stable first/last selection;
retain the existing path for scopes whose eligibility depends on the current
row. Qualify that change before increasing admitted source/cell capacity.

The actual reference window peak of 1.346 GB demonstrates that the largest
approved truth can run on this machine; it does not establish candidate memory
at increased quotas. After reviewing the policy amendment, collect progressively
larger candidate cases and retain refusal/peak evidence before a full
characterization. A policy ceiling is an accounting bound, not an allocator or
operating-system memory guarantee.

## Proposed numerical budgets and remaining gates

For item 10 review, retain the existing recommended numerical proposal:

- At cutover, each 10k/100k workload's median warm Build time is at most 1.00
  times the comparable previous Python reference on the same runner; maximum
  child-lifetime peak RSS is at most 1.00 times that reference under the same
  collection protocol.
- After release, each workload's median warm Build time is at most 1.10 times
  the previous release, using the approved 30 measured warm repetitions after
  three retained warm-ups, plus ten fresh cold children.

These numbers remain proposals. No ratio or budget is approved from a refused
sample or a single frontier run. The previous release's unavailable callback
and producer API requires an explicit comparable baseline or a separately
reviewed absolute budget after successful candidate measurements. Preparation,
activation, ingestion, derivation and verification stage attribution remains
unavailable; copied views and held Saves are actual separate outer boundaries.

The dispatched [collector workflow](../../.github/workflows/performance.yml)
downloads a named successful native run's distributions, builds/installs source
forms and a retained previous-release baseline before collection, and covers all
18 Python tuples plus three R source tuples. Every case has a fresh independent
truth child before timing. Characterization requires complete provenance and the
approved counts. It retains every child, warm-up, raw report, saved bytes, RSS,
failure and unlaunched slot, and preserves failures before rejecting the job.
Characterization compresses retained views/reports/copied CSV bytes losslessly
to keep the full raw scope within runner storage; comparisons use the original
decompressed bytes. An actual installed compressed aggregate control passes.
Callback observation runs separately and does not count toward timing samples.

The 90-day Actions retention is a collection window. Preserve the accepted
baseline/archive and raw evidence durably before that window expires and before
retiring reference code. The previous GitHub release currently has no package
assets, and the configured package registry cannot resolve `yamaa==0.3.0`.
Rollback needs a qualified, retrievable package artifact and an actual pinning
rehearsal. A source tag alone does not establish package rollback. No release
asset, budget or transition was approved by this collection; #1998 and gates
B17/B18 remain open.

Raw evidence remains at `/workspace/scratch/performance-final-frontier`,
`/workspace/scratch/performance-reference-schema-frontier` and
`/workspace/scratch/performance-callback-observer-corrected`. The built baseline
and original archive remain in `/workspace/scratch/performance-baseline`.
