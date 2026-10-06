//! Strict predicate-syntax/1 service; parsing never advertises dataset execution.
use serde::Deserialize;
use serde_json::{json, Value};
use std::{fmt, panic::catch_unwind};
use yamaa_core::{
    predicate::Comparison,
    predicate_parser::{
        parse_predicate, GrammarFailure, ParseError, ParseLimits, ParseResource, ParsedKind,
        ParsedPredicate, TemporalKind,
    },
    regex::Resource,
    temporal::TemporalError,
};

/// Bound the entire untrusted JSON envelope before decoding it.
pub const MAX_REQUEST_BYTES: usize = 1_048_576;
const PROTOCOL: &str = "predicate-syntax/1";

/// Transport defects are separate from source language and resource outcomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportError {
    RequestLimit,
    InvalidRequest,
    UnsupportedProtocol,
    Internal,
}
impl fmt::Display for TransportError {
    /// Use fixed host messages without source text or panic payload disclosure.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::RequestLimit => "predicate syntax request exceeds byte limit",
            Self::InvalidRequest => "invalid predicate syntax request",
            Self::UnsupportedProtocol => "unsupported predicate syntax protocol",
            Self::Internal => "internal predicate syntax failure",
        })
    }
}
impl std::error::Error for TransportError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol: String,
    expression: String,
}

/// Return owned syntax, names and source diagnostics without resolving any value.
/// Recursive output follows the parser-owned tree, whose depth never exceeds 64.
pub fn analyze_predicate(request: &str) -> Result<String, TransportError> {
    if request.len() > MAX_REQUEST_BYTES {
        return Err(TransportError::RequestLimit);
    }
    catch_unwind(|| {
        let request: Request=serde_json::from_str(request).map_err(|_|TransportError::InvalidRequest)?;
        if request.protocol!=PROTOCOL { return Err(TransportError::UnsupportedProtocol); }
        let outcome=match parse_predicate(&request.expression,ParseLimits::default()) {
            Ok(parsed)=>json!({"status":"parsed","ast":ast(&parsed,parsed.root()),"identifiers":parsed.identifiers().collect::<Vec<_>>()}),
            Err(ParseError::Grammar{position,failure})=> {
                let context=match &failure {
                    GrammarFailure::InvalidExpression | GrammarFailure::InvalidEscape=>json!({}),
                    GrammarFailure::InvalidRegex{byte,reason}=>json!({"pattern_byte":byte,"reason":reason}),
                    GrammarFailure::InvalidTemporal{kind,error}=>json!({"literal_type":temporal_name(*kind),"temporal_error":match error {TemporalError::InvalidForm=>"invalid_form",TemporalError::InvalidDate=>"invalid_date",TemporalError::InvalidTime=>"invalid_time"}}),
                };
                json!({"status":"invalid","condition":failure.condition(),"requirement":failure.requirement(),"position":{"byte":position.byte,"character":position.character},"context":context})
            }
            Err(ParseError::Limit{position,resource,limit})=>json!({"status":"resource_limit","phase":"parse","resource":match resource {ParseResource::Bytes=>"bytes",ParseResource::Tokens=>"tokens",ParseResource::Nodes=>"nodes",ParseResource::Depth=>"depth"},"limit":limit,"position":{"byte":position.byte,"character":position.character}}),
            Err(ParseError::RegexLimit{position,resource,limit})=>json!({"status":"resource_limit","phase":"regex_compile","resource":regex_resource(resource),"limit":limit,"position":{"byte":position.byte,"character":position.character}}),
            Err(ParseError::UnsupportedRegex{position,byte,feature})=>json!({"status":"unsupported","feature":feature,"pattern_byte":byte,"position":{"byte":position.byte,"character":position.character}}),
        };
        serde_json::to_string(&json!({"protocol":PROTOCOL,"outcome":outcome})).map_err(|_|TransportError::Internal)
    }).map_err(|_|TransportError::Internal)?
}

/// Preserve the exact owning regex policy name rather than calling it invalid syntax.
fn regex_resource(resource: Resource) -> &'static str {
    match resource {
        Resource::PatternBytes => "pattern_bytes",
        Resource::Nodes => "nodes",
        Resource::Groups => "groups",
        Resource::Depth => "depth",
        Resource::Repetition => "repetition",
        Resource::Width => "width",
        Resource::WidthWork => "width_work",
        Resource::WidthCells => "width_cells",
        Resource::SubjectBytes => "subject_bytes",
        Resource::Work => "work",
        Resource::StateCells => "state_cells",
    }
}
/// Temporal spelling is syntax metadata, not a conversion or precision policy.
fn temporal_name(kind: TemporalKind) -> &'static str {
    match kind {
        TemporalKind::Date => "date",
        TemporalKind::DateTime => "datetime",
    }
}

/// Emit the existing portable AST vocabulary, retaining operand/call character sites.
/// Parser-built children are unique occurrences, not a caller-controlled DAG.
fn ast(parsed: &ParsedPredicate, id: usize) -> Value {
    let node = &parsed.nodes()[id];
    let text = &parsed.expression()[node.span.start..node.span.end];
    let position = parsed.expression()[..node.span.start].chars().count();
    match &node.kind {
        ParsedKind::Number { fractional } => {
            json!({"kind":"literal","type":if *fractional {"float"} else {"int"},"value":text,"position":position})
        }
        ParsedKind::String(text) => {
            json!({"kind":"literal","type":"str","value":text,"position":position})
        }
        ParsedKind::Temporal { kind, value } => {
            json!({"kind":"literal","type":temporal_name(*kind),"value":value,"position":position})
        }
        ParsedKind::Null => json!({"kind":"literal","type":null,"value":null,"position":position}),
        ParsedKind::Identifier => json!({"kind":"identifier","name":text,"position":position}),
        ParsedKind::Boolean(value) => json!({"kind":"boolean","value":value}),
        ParsedKind::Group(child) => ast(parsed, *child),
        ParsedKind::Not(child) => json!({"kind":"not","value":ast(parsed,*child)}),
        ParsedKind::And(left, right) | ParsedKind::Or(left, right) => {
            json!({"kind":if matches!(node.kind,ParsedKind::And(..)) {"and"} else {"or"},"left":ast(parsed,*left),"right":ast(parsed,*right)})
        }
        ParsedKind::Compare {
            operator,
            left,
            right,
        } => {
            json!({"kind":"comparison","operator":match operator {Comparison::Equal=>"=",Comparison::NotEqual=>"<>",Comparison::Less=>"<",Comparison::LessEqual=>"<=",Comparison::Greater=>">",Comparison::GreaterEqual=>">="},"left":ast(parsed,*left),"right":ast(parsed,*right)})
        }
        ParsedKind::IsNull { value, negated } => {
            json!({"kind":"null_test","value":ast(parsed,*value),"negated":negated})
        }
        ParsedKind::In {
            value,
            items,
            negated,
        } => {
            json!({"kind":"in","value":ast(parsed,*value),"values":items.iter().map(|&id|ast(parsed,id)).collect::<Vec<_>>(),"negated":negated})
        }
        ParsedKind::Between {
            value,
            lower,
            upper,
            negated,
        } => {
            json!({"kind":"between","value":ast(parsed,*value),"lower":ast(parsed,*lower),"upper":ast(parsed,*upper),"negated":negated})
        }
        ParsedKind::Like {
            value,
            pattern,
            escape,
            negated,
        } => {
            json!({"kind":"like","value":ast(parsed,*value),"pattern":ast(parsed,*pattern),"escape":escape.map(|c|c.to_string()),"negated":negated})
        }
        ParsedKind::Contains { source, pattern } => {
            let ParsedKind::String(pattern) = &parsed.nodes()[*pattern].kind else {
                unreachable!("admitted string literal")
            };
            json!({"kind":"call","function":"str_contains","source":ast(parsed,*source),"pattern":pattern,"position":position})
        }
    }
}
