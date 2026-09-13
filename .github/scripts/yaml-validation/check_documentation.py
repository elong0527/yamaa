#!/usr/bin/env python3
"""Documentation lint for yamaa example and rule prose, run as its own CI gate.

Specification validity lives in validate_repository.py; this wrapper runs
only the prose checks, so a documentation gap fails documentation review
without masking (or being masked by) a spec verdict:

- validate_examples_readme_presence: every example has a README and every
  negative README carries its '## How to fix' section;
- validate_example_readmes: line width, schema-vocabulary-free data
  contracts, heading structure, described expected columns;
- validate_examples_index: the examples index table matches directories;
- validate_rule_metadata: rule front matter matches the rules index.
"""

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from validate_repository import (  # noqa: E402
    validate_example_readmes,
    validate_examples_index,
    validate_examples_readme_presence,
    validate_rule_metadata,
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[3],
        help="Repository root directory",
    )
    args = parser.parse_args()

    errors = []
    errors.extend(validate_examples_readme_presence(args.root))
    errors.extend(validate_example_readmes(args.root))
    errors.extend(validate_examples_index(args.root))
    errors.extend(validate_rule_metadata(args.root))

    for error in errors:
        print(error)
    if errors:
        return 1
    print("PASS: Documentation looks clean.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
