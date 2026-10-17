"""Check canonical contracts and current benchmark citations.

The repository gate derives the recursive current requirement index.
"""

import re
import sys

import yaml
from tools.paths import ROOT

REPO = ROOT
EXAMPLES = REPO / "benchmarks"
CITATION = re.compile(r"REQ-[0-9]{4,}")


def check_error(path, requirements, errors):
    try:
        contract = yaml.safe_load(path.read_text(encoding="utf-8"))
    except yaml.YAMLError as error:
        errors.append(f"ERROR: {path.relative_to(REPO)}: {error}")
        return
    if not isinstance(contract, dict):
        return
    label = path.relative_to(REPO)
    cited = contract.get("requirement")
    if cited is None:
        # Schema-phase failures (e.g. missing_required_field) carry no
        # requirement number; the rule documents them without one.
        return
    if not isinstance(cited, str) or CITATION.fullmatch(cited) is None:
        errors.append(f"ERROR: {label}: requirement must cite REQ-NNNN")
    elif cited not in requirements:
        errors.append(f"ERROR: {label}: {cited} names no numbered requirement")


def main():
    errors = []
    requirements = set()
    from tools.validation.check_rule_rewrite import check

    rule_errors, owners = check(REPO)
    errors.extend(rule_errors)
    requirements.update(owners)
    for path in sorted(EXAMPLES.glob("negative-*/expected/error.yaml")):
        check_error(path, requirements, errors)
    for error in errors:
        print(error, file=sys.stderr)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
