"""Documentation lint for yamaa example prose, run as its own CI gate.

Specification validity lives in validate_repository.py and rule files are
checked by tools.rules.check; this wrapper runs only the example prose checks,
so a documentation gap fails documentation review without masking (or being
masked by) a spec verdict:

- validate_examples_readme_presence: every example has a README and every
  negative README carries its '## How to fix' section;
- validate_example_readmes: line width, schema-vocabulary-free data
  contracts, heading structure, described expected columns;
- validate_examples_index: the derived examples index matches directories.
"""

import argparse
import sys
from pathlib import Path

from tools.paths import ROOT
from tools.validation.editorial import (
    validate_example_readmes,
    validate_examples_index,
    validate_examples_readme_presence,
)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=ROOT,
        help="Repository root directory",
    )
    args = parser.parse_args(argv)

    errors = []
    errors.extend(validate_examples_readme_presence(args.root))
    errors.extend(validate_example_readmes(args.root))
    errors.extend(validate_examples_index(args.root))

    for error in errors:
        print(error)
    if errors:
        return 1
    print("PASS: Documentation looks clean.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
