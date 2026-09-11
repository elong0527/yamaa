"""Command line entry point for ODM-to-Parquet normalization."""

from __future__ import annotations

import argparse
from pathlib import Path

from yamaa.odm.normalize import normalize_odm


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="python -m yamaa.odm",
        description="Stream an ODM XML document or archive into one Parquet file.",
    )
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument(
        "--profile",
        choices=("strict", "cart-t-openclinica", "kn189"),
        default="strict",
    )
    parser.add_argument("--batch-size", type=int, default=10_000)
    parser.add_argument(
        "--compression",
        choices=("zstd", "lz4", "snappy", "gzip", "brotli", "uncompressed"),
        default="zstd",
    )
    parser.add_argument("--overwrite", action="store_true")
    return parser


def main() -> None:
    args = _parser().parse_args()
    result = normalize_odm(
        args.source,
        args.output,
        profile=args.profile,
        batch_size=args.batch_size,
        compression=args.compression,
        overwrite=args.overwrite,
    )
    print(f"wrote {result.row_count} rows to {result.output_path}")


if __name__ == "__main__":
    main()
