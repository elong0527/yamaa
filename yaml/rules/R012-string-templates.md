---
id: R012
title: String Templates
status: normative
applies_to: [expression.str_template, string_template]

---

# String templates

## Intent

Build readable strings from named variables and literal text without admitting
host-language code or a general string-expression language.

## Boundaries

This rule owns the `string_template` grammar, escaping, interpolation, and
template-specific failures. R002 owns how placeholder names bind, R001 owns
dependency ordering, R007 owns input and result types, and R008 owns the
optional missing-value replacement. R019 owns literal and interpolated text.

## Written forms

**R012-1.** `str_template` accepts a bare template as R006 shorthand:

```yaml
str_template: "{METSTATR}|{ECOG0R}|{REGIONUSR}"
```

**R012-2.** The canonical form exposes the optional `missing` handler:

```yaml
str_template:
  template: "{SITEID}:{SUBJID}"
  missing: UNKNOWN
```

**R012-3.** The shorthand expands to `{template: <written value>}`. It
adds no missing handler.

## Grammar

**R012-4.** Scan a template from left to right under this closed
grammar:

```text
template    := part*
part        := text | placeholder | "{{" | "}}"
placeholder := "{" variable "}"
text        := one or more R019 scalar values other than "{" and "}"
```

**R012-5.** `grammar/string-template.yaml` is this grammar's single
source. The block above is its rendering, and its cases record the
literal text and placeholders every implementation must produce for a
template, or the template it must reject.

**R012-6.** Repository validation and the R implementation both read
that file, so no transcription of this grammar can drift from it
without failing.

**R012-7.** The `variable` contents must satisfy the schema type of that
name exactly. Whitespace is therefore not ignored inside braces.

**R012-8.** `{{` emits one literal `{` and `}}` emits one literal `}`.
The pairs take precedence while scanning, so `{{{SITEID}}}` produces
`{UCSD}` when `SITEID` is `UCSD`.

**R012-9.** Every single brace must begin or end a valid placeholder.
Empty placeholders, unmatched braces, format directives, operators,
function calls, and nested placeholders are invalid. In particular,
`{A + B}` is invalid rather than an expression to evaluate.

## Binding and evaluation

**R012-10.** Each placeholder is one variable reference under R002.
Qualified and unqualified names have the same meaning they have in a
field typed as `variable`.

**R012-11.** R001 collects all placeholders as dependencies before
evaluation; repeated placeholders contribute one dependency but are
replaced at every position where they occur.

**R012-12.** After its dependencies are complete, replace each
placeholder with its string value and unescape brace pairs. Placeholder
values are not implicitly converted.

**R012-13.** If any value is not a string, evaluation fails under R007.

**R012-14.** If any value is missing, return the declared `missing`
literal; without that handler, the missing input is fatal under R008.

**R012-15.** Otherwise the result is R019's exact concatenation of
literal text and replacement values, including an empty string when the
template itself is empty.

## Rationale

The template language admits only variable references and literal text,
which keeps templates readable without admitting host-language code or
a general string-expression language. Brace-pair escaping takes
precedence while scanning so that literal braces stay expressible. The
bare shorthand carries no missing handler so that specifications opt
into a replacement explicitly through the canonical form.

## Errors

**R012-16.** A template that does not parse under the grammar: fail
validation and report its specification path and the invalid placeholder
or unmatched brace.

**R012-17.** A placeholder that does not bind: fail under R001 and R002.

**R012-18.** A non-string placeholder value: fail under R007.

**R012-19.** A missing placeholder value without `missing`: fail under
R008.
