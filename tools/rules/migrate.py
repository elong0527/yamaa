"""Move rule citations to cards and summarize a conversion for review.

During the 0.5.0 conversion a card absorbs legacy requirements and lists them
on its ``Replaces`` line, so existing citations keep resolving. Subcommands:

``links [--base REF]``
    Rewrite markdown links that target a replaced requirement's anchor to the
    card that replaces it, point every link whose text is a rule ID at that
    rule's anchor, and rewrite bare replaced IDs under ``rules/`` to their
    card. With ``--base``, a link to a rule file deleted or moved since
    ``REF`` follows its rules to the one file that now holds them. Rust,
    YAML and other non-markdown citations keep resolving through
    ``Replaces``.
``report --base REF``
    List each legacy requirement defined at ``REF`` that a card now replaces,
    that moved, or that is no longer defined, for a pull request body.
``statements --base REF``
    List the cards whose title or statement was added, changed or removed
    since ``REF``: the language a reviewer approves.

For example: ``python -m tools.rules.migrate statements --base
origin/main``.
"""

import argparse
import posixpath
import re
import sys
from pathlib import Path

from tools.paths import ROOT
from tools.rules.model import (
    RuleSourceError,
    anchors,
    authored_files,
    edit_lines,
    is_rule_file,
    load,
    load_ref,
    markdown_links,
    prose_lines,
)

LEGACY_ANCHOR = re.compile(r"req-(?P<number>\d{4,})")


def file_moves(base, current):
    """Map each rule file gone since ``base`` to the one file now holding its rules.

    Returns ``(moves, unmapped)``; ``unmapped`` names the files whose rules
    are spread over several files, or defined nowhere.
    """
    moves = {}
    unmapped = {}
    for path, rule_file in base.files.items():
        if path in current.files:
            continue
        identifiers = [item.id for item in (*rule_file.requirements, *rule_file.cards)]
        destinations = {
            location[0]
            for identifier in identifiers
            if (location := current.location(identifier)) is not None
        }
        if len(destinations) == 1:
            moves[path] = destinations.pop()
        else:
            unmapped[path] = sorted(destinations)
    return moves, unmapped


def _replace_ids(current, text):
    """Rewrite replaced legacy IDs in prose to the card that replaces them."""

    def card(match):
        identifier = match[0]
        if identifier in current.requirements or identifier not in current.replaced:
            return identifier
        return current.replaced[identifier].id

    return current.citation.sub(card, text)


class LinkMigration:
    """Rewrite one checkout's markdown citations from a base revision."""

    def __init__(self, root, current, base=None):
        self.root = Path(root)
        self.current = current
        self.moves, self.unmapped = file_moves(base, current) if base else ({}, {})
        self.titles = (
            {path: item.title for path, item in base.files.items()} if base else {}
        )
        self.anchors = {}
        self.log = []
        self.unresolved = []

    def anchors_of(self, path):
        if path not in self.anchors:
            self.anchors[path] = anchors((self.root / path).read_text(encoding="utf-8"))
        return self.anchors[path]

    def unmoved(self, old_file):
        """Why a link to a missing rule file cannot follow its rules."""
        if old_file in self.unmapped:
            places = self.unmapped[old_file]
            return (
                "its rules are now in " + ", ".join(places)
                if places
                else "no current rule file holds its rules"
            )
        if not self.titles:
            return "pass --base with a revision that has it"
        return "the base revision does not have it either"

    def link_edits(self, source, link, under_rules):
        """The edits that move one link to its rule, or an empty list."""
        current = self.current
        if link.path:
            joined = posixpath.normpath(
                posixpath.join(posixpath.dirname(source), link.path)
            )
        else:
            joined = source
        if not joined.startswith("rules/"):
            return []
        old_file = joined.removeprefix("rules/")
        fragment = link.fragment
        new_file, new_fragment = old_file, fragment
        label = link.text.strip()
        text = link.text
        legacy = LEGACY_ANCHOR.fullmatch(fragment)
        if current.identifier.fullmatch(label) and current.location(label):
            # A link whose text is a rule ID points at that rule; a replaced
            # ID becomes its card.
            new_file, new_fragment = current.location(label)
            text = text.replace(label, current.resolve(label).id)
        elif legacy and current.location(f"REQ-{legacy['number']}"):
            # A link to a legacy anchor follows its requirement or card.
            new_file, new_fragment = current.location(f"REQ-{legacy['number']}")
        elif not (self.root / joined).exists() and is_rule_file(old_file):
            if old_file not in self.moves:
                self.unresolved.append(
                    f"{source}:{link.line}: {link.target}: {self.unmoved(old_file)}"
                )
                return []
            # A link to a deleted or moved rule file follows its rules.
            new_file = self.moves[old_file]
            if fragment and fragment not in self.anchors_of(f"rules/{new_file}"):
                new_fragment = ""
            if label == self.titles.get(old_file):
                text = text.replace(label, current.files[new_file].title or label)
        if under_rules:
            text = _replace_ids(current, text)
        if (new_file, new_fragment) == (old_file, fragment):
            target = link.target
        elif f"rules/{new_file}" == source and new_fragment:
            target = f"#{new_fragment}"
        else:
            target = posixpath.relpath(f"rules/{new_file}", posixpath.dirname(source))
            target += f"#{new_fragment}" if new_fragment else ""
        edits = []
        if text != link.text:
            edits.append((link.line, link.text_start, link.text_end, text))
        if target != link.target:
            bracketed = link.target_end - link.target_start == len(link.target) + 2
            written = f"<{target}>" if bracketed else target
            edits.append((link.line, link.target_start, link.target_end, written))
        if edits:
            self.log.append(
                f"{source}:{link.line}: [{link.text}]({link.target}) -> [{text}]({target})"
            )
        return edits

    def id_edits(self, source, text, links):
        """Edits that rewrite bare replaced IDs in rules prose, outside links."""
        current = self.current
        relative = source.removeprefix("rules/")
        protected = {
            number
            for card in current.all_cards()
            if card.file == relative
            for entry in card.metadata
            if entry.label == "Replaces"
            for number in range(entry.line, entry.end + 1)
        }
        edits = []
        for number, line in enumerate(prose_lines(text), 1):
            if number in protected:
                continue
            for match in current.citation.finditer(line):
                identifier = match[0]
                card = current.replaced.get(identifier)
                if card is None or identifier in current.requirements:
                    continue
                if any(
                    link.line == number and link.start <= match.start() < link.end
                    for link in links
                ):
                    continue
                edits.append((number, match.start(), match.end(), card.id))
                self.log.append(f"{source}:{number}: {identifier} -> {card.id}")
        return edits

    def rewrite(self, path):
        """The migrated text of one markdown file, or None when unchanged."""
        source = path.relative_to(self.root).as_posix()
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeError):
            return None
        under_rules = source.startswith("rules/")
        links = markdown_links(text)
        edits = [
            edit
            for link in links
            if not link.external
            for edit in self.link_edits(source, link, under_rules)
        ]
        if under_rules:
            edits += self.id_edits(source, text, links)
        return edit_lines(text, edits) if edits else None


def migrate_links(root, base=None, dry_run=False):
    """Rewrite citations in every markdown file; return the migration record."""
    migration = LinkMigration(root, load(root), base)
    for path in authored_files(root, {".md"}):
        text = migration.rewrite(path)
        if text is not None and not dry_run:
            path.write_text(text, encoding="utf-8", newline="")
    return migration


def legacy_report(base, current, ref):
    """Markdown accounting for every legacy requirement defined at ``ref``."""
    rows = []
    counts = {"replaced": 0, "moved": 0, "removed": 0, "unchanged": 0}
    for identifier in sorted(base.requirements):
        was = base.requirements[identifier].file
        if identifier in current.requirements:
            now = current.requirements[identifier].file
            if now == was:
                counts["unchanged"] += 1
                continue
            counts["moved"] += 1
            status = f"moved to `{now}`"
        elif identifier in current.replaced:
            counts["replaced"] += 1
            card = current.replaced[identifier]
            status = f"{card.id} (`{card.file}`)"
        else:
            counts["removed"] += 1
            status = "no longer defined"
        rows.append(f"| {identifier} | `{was}` | {status} |")
    lines = [
        f"Legacy requirements defined at {ref}: {len(base.requirements)}; "
        f"{counts['replaced']} replaced by cards, {counts['moved']} moved, "
        f"{counts['removed']} no longer defined, {counts['unchanged']} unchanged.",
    ]
    if rows:
        lines += ["", "| Requirement | Was in | Now |", "| --- | --- | --- |", *rows]
    unknown = sorted(set(current.replaced) - set(base.requirements))
    if unknown:
        lines += [
            "",
            f"Replaced IDs that {ref} does not define: "
            + ", ".join(f"{item} ({current.replaced[item].id})" for item in unknown)
            + ".",
        ]
    return "\n".join(lines) + "\n"


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
    links = commands.add_parser("links", help="move markdown citations to cards")
    links.add_argument("--base", help="revision that still has deleted rule files")
    links.add_argument("--dry-run", action="store_true", help="report, do not write")
    for name, summary in (
        ("report", "account for the legacy requirements of a revision"),
        ("statements", "list card language changed since a revision"),
    ):
        command = commands.add_parser(name, help=summary)
        command.add_argument("--base", required=True, help="Git revision to compare")
    arguments = parser.parse_args(argv)
    try:
        base = load_ref(arguments.root, arguments.base) if arguments.base else None
    except RuleSourceError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    if arguments.command == "links":
        migration = migrate_links(arguments.root, base, arguments.dry_run)
        for entry in migration.log:
            print(entry)
        for entry in migration.unresolved:
            print(f"UNRESOLVED: {entry}", file=sys.stderr)
        verb = "would rewrite" if arguments.dry_run else "rewrote"
        print(f"{verb} {len(migration.log)} citation(s)")
        return int(bool(migration.unresolved))
    current = load(arguments.root)
    report = legacy_report if arguments.command == "report" else statement_report
    sys.stdout.write(report(base, current, arguments.base))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
