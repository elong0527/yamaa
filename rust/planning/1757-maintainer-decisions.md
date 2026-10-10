# Decisions for the public environment migration

The maintainer approved the following recommendations on 2026-10-10 in the
Rust migration assessment conversation, replying "agree with your recommendation".
This records the explicit numerical choice required by #1740 and the implementation
defaults for #1757. It does not claim that the remaining acceptance tests pass.

- Qualify pinned `libm` 0.2.16 with default features disabled, under the internal
  `PortableLibmV1` label, for shared POWER, EXP and LN. Accept the documented
  differences from historical Python platform math, including differences that
  affect conversions, predicates, handlers or rendered bytes. Do not claim
  universal correct rounding. The locked yamaa release supplies one behavior;
  public APIs and YAML do not select a numerical policy. Implementation and
  installed qualification precede release exposure.
- The original #1757 approval kept AGENTS.md unchanged, reused `python/uv.lock`
  for Python benchmark packages and used hash-free R locks. The later
  [Cargo decision](maintainer-decisions.md#cargo-dependency-locking) on 2026-10-10
  supersedes that restriction only for the canonical `rust/Cargo.lock` and its
  unchanged staged copies. Runtime identity digests remain prohibited; R study
  locks remain hash-free.
- Replace `negative-function-contract`'s obsolete requested-contract-version
  mismatch with a missing-required-argument failure. Independently specify its
  replacement diagnostic and explain this intentional expected-fixture change in
  the PR. Other benchmark expected artifacts remain unchanged.
- Keep the existing Define-XML schema, generator and submission fixtures until
  #1758's replacement qualifies. Environment admission may carry submission
  sections without retiring the old submission path prematurely.

The #1757 closing change implements these decisions. Its scope and actual
qualification evidence are recorded in [1757-qualification.md](1757-qualification.md).
The issue closes on merge after review and installed CI qualification. Producer
qualification remains #1741; submission replacement remains #1758; full release
scope, reproducibility, performance and controlled transition remain #1742/#1585.

The closing change includes authoritative formats, public static diagnostics and
fresh per-build activation, environment terminology, all five equivalent locked
Python/R benchmarks, and removal of the superseded function-code paths. The
retained submission compatibility path follows the approved disposition above.
Resource-root compatibility remains in the implementation until the later
approved declaring-file replacement qualifies under #1905. No evidence from the
other migration checkout is counted.
