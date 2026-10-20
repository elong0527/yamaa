"""Summarize the card language a pull request changes, for review.

A change to a card's title or statement changes the language and needs the
maintainer's approval (``rules/README.md``). One subcommand:

``statements --base REF``
    List the cards whose title or statement was added, changed or removed
    since ``REF``: the language a reviewer approves.

For example: ``python -m tools.rules.migrate statements --base
origin/main``.
"""

import argparse
import sys
from pathlib import Path

from tools.paths import ROOT
from tools.rules.model import RuleSourceError, load, load_ref


def _language(card):
    return " ".join(card.title.split()), " ".join((card.statement or "").split())


def _card_block(card):
    lines = [f"### {card.id}. {card.title}", "", f"`rules/{card.file}`"]
    if card.statement:
        lines += ["", card.statement]
    return lines


def statement_report(base, current, ref):
    """Markdown listing every card title and statement changed since ``ref``."""
    before = {}
    for card in base.all_cards():
        before.setdefault(card.id, card)
    after = {}
    for rule_file in current.ordered_files():
        for card in rule_file.cards:
            after.setdefault(card.id, card)
    added = [card for identifier, card in after.items() if identifier not in before]
    removed = [card for identifier, card in before.items() if identifier not in after]
    changed = [
        (before[identifier], card)
        for identifier, card in after.items()
        if identifier in before and _language(before[identifier]) != _language(card)
    ]
    lines = [
        f"Card language relative to {ref}: {len(added)} added, {len(changed)} "
        f"changed, {len(removed)} removed."
    ]
    if added:
        lines += ["", "## Added"]
        for card in added:
            lines += ["", *_card_block(card)]
    if changed:
        lines += ["", "## Changed"]
        for old, new in changed:
            (old_title, old_statement), (new_title, new_statement) = (
                _language(old),
                _language(new),
            )
            diff = []
            if old_title != new_title:
                diff += [f"- {old_title}", f"+ {new_title}"]
            if old_statement != new_statement:
                diff += [""] if diff else []
                diff += [f"- {old_statement}", f"+ {new_statement}"]
            lines += [
                "",
                f"### {new.id}. {new.title}",
                "",
                f"`rules/{new.file}`",
                "",
                "```diff",
                *diff,
                "```",
            ]
    if removed:
        lines += ["", "## Removed"]
        for card in removed:
            lines += ["", *_card_block(card)]
    return "\n".join(lines) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--root", type=Path, default=ROOT)
    commands = parser.add_subparsers(dest="command", required=True)
    statements = commands.add_parser(
        "statements", help="list card language changed since a revision"
    )
    statements.add_argument("--base", required=True, help="Git revision to compare")
    arguments = parser.parse_args(argv)
    try:
        base = load_ref(arguments.root, arguments.base)
    except RuleSourceError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    sys.stdout.write(statement_report(base, load(arguments.root), arguments.base))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
