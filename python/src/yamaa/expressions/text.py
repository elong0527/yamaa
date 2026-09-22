"""The ASCII scalar substitutions R019 admits.

REQ-0708 fixes these exactly: they preserve scalar count, have no context
rule, language tailoring, or Unicode-version dependency, and a host casing
routine conforms only when it returns the same result for every input.
REQ-0710's case-insensitive mapping fold is the same upward substitution.
"""

from __future__ import annotations


def ascii_upper(value: str) -> str:
    """Replace U+0061..U+007A with U+0041..U+005A and keep every other scalar."""
    return "".join(
        chr(ord(character) - 32) if "a" <= character <= "z" else character
        for character in value
    )


def ascii_lower(value: str) -> str:
    """Replace U+0041..U+005A with U+0061..U+007A and keep every other scalar."""
    return "".join(
        chr(ord(character) + 32) if "A" <= character <= "Z" else character
        for character in value
    )


def ascii_sentence(value: str) -> str:
    """Uppercase the first scalar and lowercase every later scalar (ASCII only).

    Non-ASCII scalars pass through unchanged, so the scalar count is always
    preserved. A host ``capitalize`` routine must not be used: host Unicode
    behavior can expand or alter non-ASCII scalars (e.g. U+00DF or U+0130).
    """
    return ascii_upper(value[:1]) + ascii_lower(value[1:])


def ascii_title(value: str) -> str:
    """Title-case each maximal run of ASCII letters; keep other scalars.

    The first ASCII letter of every ``[A-Za-z]+`` run is uppercased and the
    remaining letters of the run are lowercased. Every other scalar --
    including non-ASCII letters -- passes through unchanged, so the scalar
    count is always preserved. Word detection is ASCII-only: there is no
    locale, no Unicode word-break rule, and no Unicode-version dependency.
    """
    parts: list[str] = []
    in_word = False
    for character in value:
        if "a" <= character <= "z" or "A" <= character <= "Z":
            if in_word:
                parts.append(ascii_lower(character))
            else:
                parts.append(ascii_upper(character))
                in_word = True
        else:
            parts.append(character)
            in_word = False
    return "".join(parts)
