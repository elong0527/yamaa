# Stability contract (draft)

Part of the [1.0 interface RFC](../interface-1.0.md): #3030 section 2.11,
D20 and P-STAB-1. The contract goes into `docs/release.md`, or a page it
links, before 1.0.0, and replaces the unadopted policy proposal of #2599. The
text below is written as it would be published; this pull request does not
publish it.

## Contract text

> ## Stability contract
>
> From yamaa 1.0.0, every 1.x release keeps the 1.0 interface. A project, a
> program or a script that works with 1.0.0 works the same way with every
> later 1.x release, with the same results and the same saved bytes. Only a
> 2.0 release may break the interface, and its release notes say what breaks.
>
> **What is frozen.**
>
> - Files: the four file types (`project`, `spec`, `codelists`, `function`),
>   with every field's name, kind, type, default and meaning; the closed
>   grammars; the verbs and verifications with their parameters; and the
>   closed vocabularies, such as column types, profiles and the CDISC lists.
> - Diagnostics: condition names and the phases each may report; the
>   condition and the rule an existing failure reports; the twelve columns of
>   `issues` and the fourteen of the run report, in order.
> - Host functions: the Python, R and command-line names, their arguments
>   (names, order and defaults), result members, the values of `status`,
>   return types, and the type mapping of each host.
> - The command line: commands, flags, exit statuses, and the keys of
>   `--format json` output.
> - The public items of the `yamaa` Rust crate, as its public API inventory
>   lists them; the internal crates `yamaa-core` and `yamaa-adapters` are not
>   covered.
> - Saved artifacts: the bytes each profile writes (CSV, Parquet, SAS
>   Transport version 5, Dataset-JSON 1.1, Define-XML 2.1 and the run report)
>   for an admitted project and its inputs, on every supported platform.
> - The layout of a printed summary line.
>
> **What a 1.x minor release may add.** Optional fields whose absence keeps
> the 1.0 meaning; file types, verbs, verifications and grammar functions;
> enumeration values; conditions, for new features only; host functions,
> members, and optional arguments after the existing ones; commands, flags and
> JSON keys; and table columns after the frozen ones. A file that uses a
> feature added in 1.N declares `yamaa: "1.N"`, and an older engine refuses it
> with one condition naming the version it needs. A minor release may also
> raise a platform floor, such as the Python or R version, announced in its
> release notes.
>
> **What a patch release may change.** Defect fixes, documentation,
> packaging and platform support, and message text. A change a repository
> such as CRAN requests after a release moves every host to the same patch
> version.
>
> **What no 1.x release may do.** Rename, remove or retype a field,
> argument, member, command, flag, column, condition, phase or enumeration
> value; add a required field or argument; change a default; report a
> different condition or rule for an existing failure; change the results or
> bytes of an admitted file; or refuse a file an earlier 1.x release admitted.
>
> **Messages are not frozen.** The `message` column, error text and the
> message in a printed summary may change in any release. Programs match
> `condition` and `rule`, never `message`.
>
> **Defect fixes.** Behavior that contradicts a rule is a defect, and fixing
> it is not a break, even when bytes change. Each fix names the rule it
> restores, adds a benchmark that pins the corrected bytes, and is listed in
> the release notes. When a rule itself is wrong, changing its statement
> changes the language: it waits for 2.0, or arrives in 1.x as a new optional
> field.
>
> **Enforcement.** `make check-repository` compares the interface manifests
> with the latest 1.x release tag and refuses a breaking change unless the
> version starts a new major release. Every benchmark present at that tag
> keeps its specification, input and expected bytes. Release candidates are
> not part of the contract: the freeze starts at 1.0.0.

## The manifests

| # | Manifest | Path and size today | Compared as | Breaking change |
| --- | --- | --- | --- | --- |
| 1 | The `yaml/` schemas (decided) | `yaml/schema*.yaml`, 16 files in 0.5.0; the four file-type schemas of Phase 1 from then on; plus the two grammars of `yaml/grammar/` | a normalized field inventory, best taken from committed generated JSON Schemas, one per file type (what `yamaa schema TYPE` prints), and the grammars' vocabulary | a removed or renamed field or class; a narrowed type; a new required field; a changed default; a removed enumeration value; a changed field kind; tightened extra-property rules; a removed function, reducer, keyword, operator or production |
| 2 | `yaml/conditions.yaml` (decided) | 202 conditions, 12 phases | condition names and their phases | a removed condition; a phase removed from a condition; messages are not compared |
| 3 | `rust/interface/bindings.yaml`, extended to the command line and tables (decided) | format 2: `values`, `tables`, `records`, `public`, `cli` | API elements, columns, commands | a removed or renamed function, class, member or argument; a reordered positional argument; a changed default; an optional argument made required; a changed result type; a removed value; a removed, reordered or retyped column; a removed command or flag; a changed exit-status meaning; a removed JSON key. The private native sections are not compared |
| 4 | The Rust public API inventory (decided) | `rust/crates/yamaa/public-api.txt` | lines | a removed line; a changed signature shows as a removed line plus an added one |
| 5 | The registry's diagnostic identities (recommended) | the core registry: 343 (phase, condition, rule) triples after the D28 rewrite | triples | a removed triple: an existing failure would report another condition or rule |
| 6 | Rule card IDs (recommended) | `rules/**/*.md`: 402 cards | IDs | a removed card; issues cite card IDs, and numbers are never reused |
| 7 | Benchmarks present at the baseline (recommended) | `benchmarks/`: 424 directories | bytes | any change to a baseline benchmark's specification, input or expected files; README prose may change |

Manifests 5 to 7 make checkable what the contract already says: "a
different condition for an existing failure" (5 and 6) and "every 1.0
benchmark keeps its expected bytes" (7).

The facade crate is `publish = false`, like the whole workspace. AGENTS.md
says unpublished crates may change freely until 1.0.0 and that 2.11 applies
from 1.0.0. At 1.0.0 it should say that `yamaa-core` and `yamaa-adapters`
stay internal while the facade's inventory is frozen.

## How `make check-repository` enforces it

1. A new check, `tools/validation/check_interface_stability.py`, joins the
   repository checks in `tools/commands/repository.py`, so
   `make check-repository`, `make quick` and CI's repository job run it.
2. **Baseline.** The newest tag matching `yamaa-v<M>.<m>.<p>` exactly, with
   no pre-release suffix, whose major `M` equals the workspace's major version
   (`[workspace.package] version` in `rust/Cargo.toml`; the shared-version
   check keeps Python and R in step). Candidate tags are never baselines (D1).
   Today's tags are `yamaa-py-0.1.0` to `yamaa-py-0.3.0` and `yamaa-v0.4.0`,
   so the check stays inactive until `yamaa-v1.0.0` exists.
3. **Activation.** At a workspace version `M.0.0` with no tag of major `M`, the
   check is inactive and says why. At any other version with no tag of its
   major, it fails and asks for the tags to be fetched, so a shallow clone
   cannot pass by accident. When the workspace's major is above the
   baseline's, it lists the breaks for the release notes and passes.
4. **CI.** The repository job checks out with `fetch-depth: 0`; it uses the
   default depth of 1 today. `release.yml` and `release-rehearsal.yml` already
   fetch everything.
5. **Reading the baseline.** `git show <tag>:<path>` for each file, and
   `git ls-tree` for globs, as `load_ref` in `tools/rules/model.py` already
   reads a revision's rules.
6. **Comparison.** Each manifest is normalized into an inventory and diffed.
   Every removed or changed element is classified as breaking or not;
   additions are reported for the release notes.
7. **Output.** One line per break, `<manifest>: <element>: <what changed>`,
   and exit status 1 when any break remains and the majors are equal.
8. **Tests.** `tools/tests/test_interface_stability.py` runs each rule against
   synthetic baselines.
9. **Release checklist.** `docs/release.md` adds a step: the release commit
   passes the check against the previous release.
10. **No allowlist.** An intentional break is a major release. A defect fix
    changes behavior, which its new benchmark pins, not a manifest or an
    existing benchmark. If one ever must change a manifest element or a
    baseline benchmark, that is an owner decision recorded in the release
    notes, reviewed like a golden change.

## Platform floors

Section 2.11 is silent on floors. **Recommendation:** floors such as Python
3.12 and R 4.4 move only in minor releases, announced in the release notes,
never in a patch; the contract text above says so.
