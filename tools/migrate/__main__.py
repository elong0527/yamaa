"""Convert 0.5 YAML files and Markdown blocks into a target directory.

    python -m tools.migrate benchmarks/adam-adsl-bmi --output /tmp/converted
    python -m tools.migrate benchmarks --output /tmp/converted --counts
    python -m tools.migrate --embedded rust/crates --output /tmp/converted

Each SOURCE is a benchmark or project directory, a directory searched for
them, a project file, or a YAML or Markdown file. The converted files are
written under the target at their path relative to `--base` (by default
the common parent of the sources); files that need no change are copied
unchanged from a unit directory. The manual follow-up list goes to standard
output. The target must not overlap a source: nothing is converted in place.

With `--embedded`, each SOURCE is a Rust, Python, R, JSON or TSV file or a
directory searched for them, and the YAML embedded in them is converted
instead (`tools.migrate.embedded`); only the files that change are written.
"""

import argparse
import sys
from collections import Counter

from tools.migrate import SourceError, convert, write
from tools.migrate.embedded import convert_embedded
from tools.migrate.table import TableError


def render_counts(result):
    lines = []
    rows = sorted(
        {row for _, row in result.counts} | {row for _, row in result.documents}
    )
    if rows:
        # The embedded converter counts statistics only.
        lines.append("Converter footprint (occurrences / documents):")
        for area in ("files", "markdown"):
            lines.append(f"  {area}:")
            for row in rows:
                occurrences = result.counts.get((area, row), 0)
                documents = len(result.documents.get((area, row), ()))
                if occurrences or documents:
                    lines.append(f"    {row}: {occurrences} / {documents}")
    lines.append("Statistics:")
    for name, value in sorted(result.stats.items()):
        lines.append(f"  {name}: {value}")
    return "\n".join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(
        prog="python -m tools.migrate",
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "sources", nargs="+", help="directories, project files, YAML or Markdown files"
    )
    parser.add_argument(
        "--embedded",
        action="store_true",
        help="convert the YAML embedded in Rust, Python, R, JSON and TSV files",
    )
    parser.add_argument(
        "--output", required=True, help="target directory, outside every source"
    )
    parser.add_argument("--base", help="directory the written paths are relative to")
    parser.add_argument(
        "--counts", action="store_true", help="also print the converter footprint"
    )
    args = parser.parse_args(argv)
    try:
        if args.embedded:
            result = convert_embedded(args.sources, base=args.base)
        else:
            result = convert(args.sources, base=args.base)
        target = write(result, args.output, args.sources)
    except (SourceError, TableError, FileNotFoundError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    kinds = Counter(note.kind for note in result.notes)
    print(
        f"Wrote {len(result.files)} files to {target}; dropped {len(result.removed)}."
    )
    for removed in result.removed:
        print(f"  dropped {removed}")
    if result.notes:
        summary = ", ".join(f"{kind} {count}" for kind, count in sorted(kinds.items()))
        print(f"Manual follow-up ({len(result.notes)}: {summary}):")
        for note in sorted(
            result.notes, key=lambda item: (item.file, item.line, item.kind)
        ):
            print(f"  {note.render()}")
    else:
        print("Manual follow-up: none.")
    if args.counts:
        print(render_counts(result))
    return 0


if __name__ == "__main__":
    sys.exit(main())
