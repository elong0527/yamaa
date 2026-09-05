---
id: R022
title: Schema release and compatibility
status: normative
applies_to: [schema_bundle, implementations, migrations]
depends_on: [R006]
---

# Schema release and compatibility

## Intent

Make a bundle version a reproducible compatibility contract rather than a
name inferred from repository history.

## Boundaries

R006 owns bundle loading and exact version matching. This rule owns assignment,
publication, compatibility, implementation capability reporting, and migration
between those exact versions. Individual rules continue to own runtime behavior
and errors.

## Version authority and format

The maintainers assign versions through a reviewed change to `schema.yaml` and
its release manifest. Published bundle versions use Semantic Versioning 2.0.0,
including optional prerelease identifiers. All included schema documents have
the exact same version under R006.

The current `1.0` value is an **unreleased development identifier** retained to
avoid rewriting every specification while the language is being designed. It
is not a Semantic Versioning release, carries no compatibility or immutability
promise, and must never be tagged or advertised as supported in production.
Its `development_revision` in `yaml/releases/development.yaml` increments for
each release-sensitive change under the unchanged identifier. The first
published bundle receives a three-component Semantic Versioning version only
after all indexed normative rules and executable conformance fixtures are
accepted.

## Compatibility classification

A change is breaking when an already-valid document can be rejected, resolves
to a different canonical form, executes differently, emits a different
artifact, or produces a different machine-readable error condition. This
includes removals or restrictions in schema declarations; changes to normative
rules, defaults, grammar, types, ordering, missing-value behavior, errors, or
rendering; and making a previously optional implementation behavior required.
It requires a major-version increment after `1.0.0`.

A backward-compatible addition accepts a construct that older bundles reject
without changing existing documents. It requires a minor-version increment.
A clarification, non-normative documentation correction, or implementation fix
that leaves every accepted document and observable result unchanged requires a
patch-version increment when it changes a published bundle artifact.

Prereleases make no compatibility promise, but every release-sensitive change
still increments the prerelease identifier or selects a new version. The
unreleased development bundle instead increments `development_revision`. No
change may silently reuse a published bundle version. Compatible additions
therefore never share an already published version.

## Publication and immutability

A bundle is published only by an annotated Git tag named
`schema-vVERSION` whose commit contains `yaml/releases/VERSION.yaml` with
`status: released`. The tag and manifest identify the entry points, transitive
schema modules, indexed normative rules, validation surfaces, canonical
fixtures, and their SHA-256 digest. Release automation must archive those
paths as an immutable artifact and verify the digest before publication.

A published tag, manifest, or archive is never replaced. Corrections receive a
new version. Prerelease manifests are reproducible candidates but are not the
production support promise. The development manifest is not a release artifact
and cannot be tagged.

## Implementation capability reporting

Every implementation exposes a machine-readable capability document with its
implementation name and version, `supported_bundle_versions`, and
`default_bundle_version`. The default must be in the supported list.
`yaml/implementations/r.yaml` and `yaml/implementations/python.yaml` are the
repository implementations' canonical reports.

Before structural validation or execution, an implementation compares the
document's `schema_version` with that list by exact string equality. A missing
version is `schema_version_missing`; a version outside the list is
`schema_version_unsupported`. Neither case may fall back to the default or the
nearest version. R and Python must return the same condition and include the
requested and supported versions in diagnostic data.

## Deprecation and migration

Deprecation starts only in a minor release that still accepts the construct.
The release notes name its replacement and earliest removal major; it remains
supported for that major line. Removal occurs only in the announced or a later
major version.

Each breaking release supplies a migration directory under `yaml/migrations/`
with the source document, migrated document, canonical resolved forms, and a
README describing intentional differences. Migration never mutates an input
in place. No migration is manufactured for the current development identifier;
the first required fixture accompanies the first breaking transition between
published bundles.

## Change control

`check_schema_release.py` compares release-sensitive paths with the pull
request base. A schema, normative rule, default, grammar, error contract,
rendering contract, or canonical fixture change fails under an unchanged
published bundle version, or under an unchanged development revision. It also
checks the version format, uniform schema-module versions, capability reports,
migration coverage, and release manifest.

Reviewers classify the version increment using this rule. CI detects a missing
version action; it does not infer whether major, minor, or patch is semantically
correct. A production manifest additionally requires its immutable tag.

## Errors

Implementations must reject unsupported versions as specified above. Release
validation fails for an invalid or inconsistent version, an unreported current
version, a missing migration fixture, a release-sensitive change under an
unchanged version, a manifest mismatch, or a released manifest without its
exact annotated tag.
