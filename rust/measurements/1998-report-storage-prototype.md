# 1k report storage prototype

The installed optimized Python prototype at `0e441087` accepts all nine approved
1k variants, including complete independent typed output, empty issues/logs,
absence of files after Build, complete graph/source ledgers, and two successful
held Saves with exact artifact bytes. The callback observer independently
retains the 859 actual delegated calls. Ten children are retained under
`/workspace/scratch/resource-map-report-frontier-0e441087-locked`.

The encoded report ceiling remains 16 MiB. Pre-clone projection accounting has
a separate 64 MiB ceiling; source, execution, output and work limits remain
unchanged. Fixed type/value cell-map structure is charged before the actual
payload width. These are accounting bounds, not allocator/process RSS promises.
The optional capacity trace is temporary prototype instrumentation and must be
removed before release integration.

Ordinary Save prepares one complete report and reserves its at-most-three
artifact slots before any publisher. Every possible prefix is smaller than the
admitted complete encoded report. After a successful publisher, Save moves an
already-owned artifact into an already-reserved slot; it does not allocate a
new JSON subtree or perform further report admission. Repeated Saves create a
fresh prefix. Observation transport serializes immutable Build fields with an
empty artifact array, or the held actual publication report, without copying
the complete source/output cell tree.

| Case | Child peak bytes | Build JSON bytes | Save JSON bytes | Maximum projection charge |
| --- | ---: | ---: | ---: | ---: |
| lookup 1k, multiplicity 4 | 87,314,432 | 434,116 | 475,655 | 22,046,552 |
| window 1k, width 100 | 90,087,424 | 359,148 | 402,612 | 16,593,228 |
| producer 1k, depth 3 | 139,718,656 | 1,300,701 | 1,300,929 | 51,945,328 |

The producer's six individual projection charges are 4.35–8.21 MB under
11,184,808-byte retained fractions. Its aggregate ownership/copy charge is the
largest observed boundary. Ordinary Save's modeled charge is approximately
half the previous multiple-prefix implementation: lookup multiplicity 4 was
44.89 MB and window width 100 was 34.05 MB in `efa49d39`.

All 108 adapter unit controls pass. Complete first/middle/last failure reports,
original IO payload identity, unwind payload identity, entered/completed
prefixes, retries, failed-build ancillary logs, linked-parent prepublication
refusal and report-limit refusal before every publisher are covered. Dependency,
diagnostic and declared API guards pass. R compilation/installed qualification
requires CI because this machine has no R installation.

These are functional capacity observations. Shared-machine clocks are retained
as diagnostic data and do not approve timing distributions or numerical budgets.
The separate initial `resource-map-report-frontier-0e441087` attempt is marked
invalid because a wheel reinstall resolved newer transitive dependencies; locked
requirements were restored before the directory used above was collected.

The producer report has 28,000 typed cell objects at 1k. Scaling the same JSON
cell-map representation to 100k would retain millions of map objects and repeat
them across node/aggregate/Save owners. A larger report quota alone is therefore
not proposed. The next storage amendment should stream immutable table rows
into bounded encoded blocks and share them while retaining the existing exact
serialized report shape, original read/error order and complete prepublication
prefix admission. Larger source/output capacities remain proposals until that
storage policy and ordinary indexed-choice accounting are reviewed and measured
progressively at 1k, 10k and 100k.
