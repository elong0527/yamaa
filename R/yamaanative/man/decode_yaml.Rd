\name{decode_yaml}
\alias{decode_yaml}
\title{Decode an ASCII YAML snapshot through shared Rust}
\usage{decode_yaml(source)}
\arguments{\item{source}{An unclassed raw vector without attributes containing
one retained ASCII YAML snapshot.}}
\value{Owned yaml/1 outcome JSON. Source diagnostics and resource refusals are
data outcomes without a partial tree. Transport failures raise an R error
after the native call returns.}
\description{Decodes YAML 1.2 core values into the ordered arena accepted by
schema/1. Integer nodes contain exact decimal strings and float nodes contain
binary64 bits. No R numeric conversion or host YAML interpreter participates.}
\details{Source is limited to 8,388,608 bytes; output to 16,777,216 bytes.
Decoded documents are limited to 131,072 nodes, 262,144 edges, depth 64 and
8,388,608 text bytes. Numeric parsing permits at most 4,096 digits. Diagnostic
reparsing has a 33,554,432 source-byte work quota and diagnostic paths have an
8,388,608 cumulative byte quota. Refusal is reported explicitly.
Duplicate keys, anchors, aliases, explicit tags, plain merge keys, non-scalar
keys and multiple documents are rejected. Non-finite values normalize to null.
Integers retain their exact identity even outside the runtime signed-i64 range;
runtime range checks occur later. Escaped surrogates produce invalid-text
diagnostics, never substituted values. Filesystem access, includes, inheritance
and workflow planning remain separate responsibilities. This experimental
service does not grant execution support or change the default engine.}
