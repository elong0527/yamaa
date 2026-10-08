"""Data-frame conversion for Rust-owned public-result records."""

from collections.abc import Sequence

import polars as pl

IssueRow = tuple[str, str, str | None, list[str], str]
ISSUE_SCHEMA = {
    "phase": pl.String,
    "condition": pl.String,
    "requirement": pl.String,
    "spec_paths": pl.List(pl.String),
    "context": pl.String,
}


def issues_frame(rows: Sequence[IssueRow]) -> pl.DataFrame:
    """Copy complete native records, including empty and all-missing columns."""
    return pl.DataFrame(rows, schema=ISSUE_SCHEMA, orient="row")
