---
id: R012
title: String Templates
status: normative
applies_to: [expression.str_template, string_template]
depends_on: [R001, R002, R006, R007, R008]
---

# String templates

## Intent

Build readable strings from named variables and literal text without admitting
host-language code or a general string-expression language.

## Boundaries

This rule owns the `string_template` grammar, escaping, interpolation, and
template-specific failures. R002 owns how placeholder names bind, R001 owns
dependency ordering, R007 owns input and result types, and R008 owns the
optional missing-value replacement.

## Written forms

`str_template` accepts a bare template as R006 shorthand:

```yaml
str_template: "{METSTATR}|{ECOG0R}|{REGIONUSR}"
```

The canonical form exposes the optional `missing` handler:

```yaml
str_template:
  template: "{SITEID}:{SUBJID}"
  missing: UNKNOWN
```

The shorthand expands to `{template: <written value>}`. It adds no missing
handler.

## Grammar

Scan a template from left to right under this closed grammar:

```text
template    := part*
part        := text | placeholder | "{{" | "}}"
placeholder := "{" variable "}"
text        := one or more Unicode code points other than "{" and "}"
```

The `variable` contents must satisfy the schema type of that name exactly.
Whitespace is therefore not ignored inside braces. `{{` emits one literal `{`
and `}}` emits one literal `}`. The pairs take precedence while scanning, so
`{{{SITEID}}}` produces `{UCSD}` when `SITEID` is `UCSD`.

Every single brace must begin or end a valid placeholder. Empty placeholders,
unmatched braces, format directives, operators, function calls, and nested
placeholders are invalid. In particular, `{A + B}` is invalid rather than an
expression to evaluate.

## Binding and evaluation

Each placeholder is one variable reference under R002. Qualified and
unqualified names have the same meaning they have in a field typed as
`variable`. R001 collects all placeholders as dependencies before evaluation;
repeated placeholders contribute one dependency but are replaced at every
position where they occur.

After its dependencies are complete, replace each placeholder with its string
value and unescape brace pairs. Placeholder values are not implicitly
converted. If any value is not a string, evaluation fails under R007. If any
value is missing, return the declared `missing` literal; without that handler,
the missing input is fatal under R008. Otherwise the result is the exact
concatenation of literal text and replacement values, including an empty
string when the template itself is empty.

## Errors

- A template that does not parse under the grammar: fail validation and report
  its specification path and the invalid placeholder or unmatched brace.
- A placeholder that does not bind: fail under R001 and R002.
- A non-string placeholder value: fail under R007.
- A missing placeholder value without `missing`: fail under R008.
