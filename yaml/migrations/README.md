# Bundle migrations

A breaking released bundle must add a directory named `FROM-to-TO`. It contains
`source.resolved.yaml` and `target.resolved.yaml` plus a README that identifies
every intentional canonical difference. Both YAML files are complete, resolved
specifications with no `parents`.

There is no migration fixture yet because `1.0` is the single unreleased
development bundle, not a published source or target version. The first
breaking transition between published bundles must add the first fixture.
