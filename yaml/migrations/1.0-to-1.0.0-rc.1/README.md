# Migration from the unissued 1.0 design label

This canonical fixture changes only `schema_version`. The source records the
former, unissued design label; the target opts into the first prerelease ruled
by R022. Both files are already resolved (they have no `parents`), so they are
also the canonical resolved forms. No production compatibility is implied for
the source label.
