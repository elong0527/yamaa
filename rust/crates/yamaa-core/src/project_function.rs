//! Static admission of versionless project function definitions.
//!
//! This trusted typed service admits lowered definition roots
//! ([`ast::Function`]). It owns no source reader, lock verifier, import
//! mechanism or callable port.
use crate::ast::{self, FunctionTest};
use crate::value::{ColumnType, Value, ValueType};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Python,
    R,
}
impl Language {
    /// The project `environments` key that names this host language.
    pub fn key(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::R => "r",
        }
    }
}

/// Authored indices survive admission and identify the exact rejected fields.
/// `InvalidCallable` refuses a 0.5 definition's `function`; a 1.0 function
/// file is refused when it names no callable at all (`MissingImplementation`,
/// FUNC-4) or one that is not a callable of its language
/// (`InvalidImplementation`, FUNC-8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Finding {
    InvalidName,
    InvalidCallable,
    MissingImplementation,
    InvalidImplementation(Language),
    EmptyDescription,
    NegativeComparisonDecimals,
    InvalidParameterName { parameter: usize },
    DuplicateParameter { parameter: usize },
    RequiredDefault { parameter: usize },
    MissingDefault { parameter: usize },
    InvalidDefault { parameter: usize },
    InvalidCaseId { case: usize },
    DuplicateCaseId { case: usize },
    EmptyCoverage { case: usize },
    DuplicateCoverage { case: usize, tag: usize },
    InvalidCoverage { case: usize, tag: usize },
    UnsupportedEvidence { case: usize, tag: usize },
    DuplicateArgument { case: usize, argument: usize },
    UnknownArgument { case: usize, argument: usize },
    MissingRequiredArgument { case: usize, parameter: usize },
    ArgumentType { case: usize, argument: usize },
    InvalidResult { case: usize },
    MissingObligation(String),
}

/// A compiler-resolved argument type; None denotes an explicit missing literal.
/// Column references retain their declared kind even when runtime values are missing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallArgument {
    pub name: String,
    pub kind: Option<ValueType>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallFinding {
    DuplicateArgument {
        argument: usize,
    },
    UnknownArgument {
        argument: usize,
    },
    ArgumentType {
        argument: usize,
        expected: ValueType,
        actual: ValueType,
    },
    MissingRequiredArgument {
        parameter: usize,
    },
}

/// Immutable definition admitted with every inferable coverage obligation.
#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    /// The host language whose callable the build binds, or none on an engine
    /// that claims no host language (#3030 section 2.10).
    language: Option<Language>,
    definition: ast::Function,
}
impl Function {
    /// The environment has already admitted the complete definition collection
    /// once. Transfer its definitions without repeating admission or copying
    /// their test cases.
    pub(crate) fn from_admitted(language: Option<Language>, definition: ast::Function) -> Self {
        Self {
            language,
            definition,
        }
    }
    /// The host language whose callable the build binds; none on an engine
    /// that claims no host language, which binds no callable.
    pub fn language(&self) -> Option<Language> {
        self.language
    }
    /// The lowered definition.
    pub fn definition(&self) -> &ast::Function {
        &self.definition
    }
    /// The logical name specifications call the function by.
    pub fn name(&self) -> &str {
        &self.definition.name.name
    }
    /// The callable this host binds. Admission refuses a 0.5 definition
    /// without a valid one for its language; a 1.0 function file may name
    /// none for this host, and a call to it then fails before any data is
    /// read (FUNC-1). An engine that claims no host language binds none, so
    /// the callable is empty.
    pub fn call(&self) -> &str {
        self.language
            .map_or("", |language| callable_of(language, &self.definition))
    }
    /// The host language this definition names no callable for, so that a
    /// call to it fails before any data is read (FUNC-1). None when it names
    /// one, and on an engine that claims no host language, which misses no
    /// implementation: it runs no project function, and a run that calls one
    /// is refused instead (D110).
    pub fn missing_implementation(&self) -> Option<Language> {
        self.language
            .filter(|&language| self.definition.implementation(language).is_none())
    }
    /// Reuse the shared exact-type/default/missing invocation signature without
    /// inventing versions or a logical-to-host parameter mapping.
    pub fn invocation_plan(
        &self,
    ) -> Result<
        crate::function_signature::ProjectInvocationPlan,
        crate::function_signature::PlanError,
    > {
        use crate::function_signature::{
            Parameter as BoundParameter, Presence, ProjectFunctionIdentity, ProjectInvocationPlan,
        };
        let parameters = self
            .definition
            .params
            .iter()
            .map(|p| BoundParameter {
                name: p.name.name.clone(),
                host_name: p.name.name.clone(),
                kind: p.kind,
                accepts_missing: p.accepts_missing,
                presence: if p.required {
                    Presence::Required
                } else {
                    Presence::Optional(p.default.clone().expect("admitted optional default"))
                },
            })
            .collect();
        ProjectInvocationPlan::new(
            ProjectFunctionIdentity {
                name: self.name().into(),
                call: self.call().into(),
            },
            parameters,
            self.definition.returns,
            self.definition.may_return_missing,
        )
    }
    /// Bind every argument statically before any code activation or study capture.
    /// Missing permissions govern invocation; an explicit missing literal is legal
    /// for a non-accepting parameter and causes a runtime short circuit.
    pub fn bind_call(&self, arguments: &[CallArgument]) -> Vec<CallFinding> {
        let mut findings = Vec::new();
        let mut names = BTreeSet::new();
        for (argument, item) in arguments.iter().enumerate() {
            if !names.insert(&item.name) {
                findings.push(CallFinding::DuplicateArgument { argument });
            }
            match self
                .definition
                .params
                .iter()
                .find(|p| p.name.name == item.name)
            {
                None => findings.push(CallFinding::UnknownArgument { argument }),
                Some(parameter) => {
                    if let Some(actual) = item.kind {
                        if actual != parameter.kind {
                            findings.push(CallFinding::ArgumentType {
                                argument,
                                expected: parameter.kind,
                                actual,
                            });
                        }
                    }
                }
            }
        }
        for (parameter, item) in self.definition.params.iter().enumerate() {
            if item.required && !names.contains(&item.name.name) {
                findings.push(CallFinding::MissingRequiredArgument { parameter });
            }
        }
        findings
    }
    pub fn admit(language: Language, definition: ast::Function) -> Result<Self, Vec<Finding>> {
        let findings = validate(Some(language), &definition);
        if findings.is_empty() {
            Ok(Self {
                language: Some(language),
                definition,
            })
        } else {
            Err(findings)
        }
    }
}

/// The callable `language` binds, or nothing to admit when the definition
/// writes none for it.
fn callable_of(language: Language, def: &ast::Function) -> &str {
    def.implementation(language)
        .map_or("", |implementation| implementation.callable.text.as_str())
}
fn identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
fn python_name(name: &str) -> bool {
    identifier(name)
        && !matches!(
            name,
            "False"
                | "None"
                | "True"
                | "and"
                | "as"
                | "assert"
                | "async"
                | "await"
                | "break"
                | "class"
                | "continue"
                | "def"
                | "del"
                | "elif"
                | "else"
                | "except"
                | "finally"
                | "for"
                | "from"
                | "global"
                | "if"
                | "import"
                | "in"
                | "is"
                | "lambda"
                | "nonlocal"
                | "not"
                | "or"
                | "pass"
                | "raise"
                | "return"
                | "try"
                | "while"
                | "with"
                | "yield"
        )
}
fn r_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    let first = bytes.next();
    let valid_start = match first {
        Some(b'.') => !name.as_bytes().get(1).is_some_and(u8::is_ascii_digit),
        Some(byte) => byte.is_ascii_alphabetic(),
        None => false,
    };
    valid_start
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
        && !matches!(
            name,
            "break"
                | "else"
                | "FALSE"
                | "for"
                | "function"
                | "if"
                | "Inf"
                | "in"
                | "NA"
                | "NA_character_"
                | "NA_complex_"
                | "NA_integer_"
                | "NA_real_"
                | "NaN"
                | "next"
                | "NULL"
                | "repeat"
                | "TRUE"
                | "while"
                | "..."
        )
        && !name
            .strip_prefix("..")
            .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
}
fn host_name(language: Language, name: &str) -> bool {
    match language {
        Language::Python => python_name(name),
        Language::R => r_name(name),
    }
}
fn callable(language: Language, name: &str) -> bool {
    match language {
        Language::Python => {
            let parts: Vec<_> = name.split('.').collect();
            parts.len() >= 2 && parts.iter().all(|part| python_name(part))
        }
        Language::R => name.split_once("::").is_some_and(|(package, function)| {
            package
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
                && package
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
                && r_name(function)
        }),
    }
}
fn case_id(id: &str) -> bool {
    let mut bytes = id.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_lowercase())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn column_kind(kind: ColumnType) -> ValueType {
    match kind {
        ColumnType::Str => ValueType::Str,
        ColumnType::Int => ValueType::Int,
        ColumnType::Float => ValueType::Float,
        ColumnType::Date => ValueType::Date,
        ColumnType::DateTime => ValueType::DateTime,
    }
}

fn obligations(def: &ast::Function) -> BTreeSet<String> {
    let mut needed = BTreeSet::from([String::from("normal"), String::from("boundary")]);
    for p in &def.params {
        let name = &p.name.name;
        if !p.required {
            needed.insert(std::format!("default:{name}"));
        }
        needed.insert(std::format!(
            "{}:{name}",
            if p.accepts_missing {
                "accepted-missing"
            } else {
                "short-circuit-missing"
            },
        ));
        if p.kind == ValueType::Bool {
            needed.insert(std::format!("boolean-true:{name}"));
            needed.insert(std::format!("boolean-false:{name}"));
        }
    }
    if def.may_return_missing {
        needed.insert(String::from("nullable-output"));
    }
    if def.returns == ColumnType::Float {
        needed.insert(String::from("numeric-comparison"));
    }
    needed
}

fn evidence(
    tag: &str,
    def: &ast::Function,
    case: &FunctionTest,
    args: &BTreeMap<&str, &Value>,
    short_circuit: bool,
) -> Option<bool> {
    match tag {
        "normal" | "boundary" => return Some(true),
        "nullable-output" => {
            return Some(def.may_return_missing && !short_circuit && case.result == Value::Missing)
        }
        "numeric-comparison" => {
            return Some(def.returns == ColumnType::Float && matches!(case.result, Value::Float(_)))
        }
        _ => (),
    }
    let (kind, name) = tag.split_once(':')?;
    if !identifier(name) {
        return None;
    }
    if !matches!(
        kind,
        "default" | "accepted-missing" | "short-circuit-missing" | "boolean-true" | "boolean-false"
    ) {
        return None;
    }
    let Some(p) = def.params.iter().find(|p| p.name.name == name) else {
        return Some(false);
    };
    let explicit = args.get(name).copied();
    let effective = explicit.or(p.default.as_ref());
    Some(match kind {
        "default" => !p.required && explicit.is_none(),
        "accepted-missing" => {
            p.accepts_missing && explicit == Some(&Value::Missing) && !short_circuit
        }
        "short-circuit-missing" => {
            !p.accepts_missing && explicit == Some(&Value::Missing) && case.result == Value::Missing
        }
        "boolean-true" => p.kind == ValueType::Bool && effective == Some(&Value::Bool(true)),
        "boolean-false" => p.kind == ValueType::Bool && effective == Some(&Value::Bool(false)),
        _ => unreachable!(),
    })
}

/// Whether a definition is a 1.0 function file, which declares its own header
/// and names a callable for each host language it implements (D8).
fn function_file(def: &ast::Function) -> bool {
    matches!(
        def.header,
        Some(ast::Header {
            surface: ast::Surface::V1 { .. },
            ..
        })
    )
}

/// Report all independent static faults, followed by unsatisfied obligations in
/// lexical tag order. Cases with invalid arguments/results cannot supply coverage.
///
/// A 0.5 definition binds one callable, for `language`, the language its
/// environment file's key states; without a language it has no valid
/// callable, although an engine that claims no host language reads no
/// environment file and so admits no 0.5 definition. A 1.0 function file is
/// language-neutral: every callable it names must be one of its language,
/// and each parameter name a name in every language it implements, whichever
/// host runs the build (FUNC-4, FUNC-8).
pub fn validate(language: Option<Language>, def: &ast::Function) -> Vec<Finding> {
    let mut findings = Vec::new();
    if !identifier(&def.name.name) {
        findings.push(Finding::InvalidName);
    }
    let languages: Vec<Language> = if function_file(def) {
        if def.implementations.is_empty() {
            findings.push(Finding::MissingImplementation);
        }
        for implementation in &def.implementations {
            if !callable(implementation.language, &implementation.callable.text) {
                findings.push(Finding::InvalidImplementation(implementation.language));
            }
        }
        def.implementations
            .iter()
            .map(|implementation| implementation.language)
            .collect()
    } else {
        if !language.is_some_and(|language| callable(language, callable_of(language, def))) {
            findings.push(Finding::InvalidCallable);
        }
        language.into_iter().collect()
    };
    if def.description.text.is_empty() {
        findings.push(Finding::EmptyDescription);
    }
    if def.comparison_decimals < 0 {
        findings.push(Finding::NegativeComparisonDecimals);
    }
    let mut names = BTreeSet::new();
    for (parameter, p) in def.params.iter().enumerate() {
        if !identifier(&p.name.name)
            || languages
                .iter()
                .any(|&language| !host_name(language, &p.name.name))
        {
            findings.push(Finding::InvalidParameterName { parameter });
        }
        if !names.insert(p.name.name.as_str()) {
            findings.push(Finding::DuplicateParameter { parameter });
        }
        match (p.required, &p.default) {
            (true, Some(_)) => findings.push(Finding::RequiredDefault { parameter }),
            (false, None) => findings.push(Finding::MissingDefault { parameter }),
            (false, Some(value))
                if value
                    .value_type()
                    .map_or(!p.accepts_missing, |kind| kind != p.kind) =>
            {
                findings.push(Finding::InvalidDefault { parameter })
            }
            _ => (),
        }
    }
    let mut ids = BTreeSet::new();
    let mut covered = BTreeSet::new();
    for (case_index, case) in def.tests.iter().enumerate() {
        let start = findings.len();
        if !case_id(&case.id.text) {
            findings.push(Finding::InvalidCaseId { case: case_index });
        }
        if !ids.insert(&case.id.text) {
            findings.push(Finding::DuplicateCaseId { case: case_index });
        }
        let mut args = BTreeMap::new();
        for (argument, (name, value)) in case.args.iter().enumerate() {
            if args.insert(name.name.as_str(), value).is_some() {
                findings.push(Finding::DuplicateArgument {
                    case: case_index,
                    argument,
                });
            }
            match def.params.iter().find(|p| p.name.name == name.name) {
                None => findings.push(Finding::UnknownArgument {
                    case: case_index,
                    argument,
                }),
                Some(p) if value.value_type().is_some_and(|kind| kind != p.kind) => {
                    findings.push(Finding::ArgumentType {
                        case: case_index,
                        argument,
                    })
                }
                _ => (),
            }
        }
        let mut short_circuit = false;
        for (parameter, p) in def.params.iter().enumerate() {
            match args
                .get(p.name.name.as_str())
                .copied()
                .or(p.default.as_ref())
            {
                None if p.required => findings.push(Finding::MissingRequiredArgument {
                    case: case_index,
                    parameter,
                }),
                Some(Value::Missing) if !p.accepts_missing => short_circuit = true,
                _ => (),
            }
        }
        let valid_result = if short_circuit {
            case.result == Value::Missing
        } else {
            case.result
                .value_type()
                .map_or(def.may_return_missing, |kind| {
                    kind == column_kind(def.returns)
                })
        };
        if !valid_result {
            findings.push(Finding::InvalidResult { case: case_index });
        }
        let valid_case = findings.len() == start;
        if case.covers.is_empty() {
            findings.push(Finding::EmptyCoverage { case: case_index });
        }
        let mut tags = BTreeSet::new();
        for (tag_index, tag) in case.covers.iter().enumerate() {
            let tag = &tag.text;
            if !tags.insert(tag) {
                findings.push(Finding::DuplicateCoverage {
                    case: case_index,
                    tag: tag_index,
                });
            }
            match evidence(tag, def, case, &args, short_circuit) {
                None => findings.push(Finding::InvalidCoverage {
                    case: case_index,
                    tag: tag_index,
                }),
                Some(false) => findings.push(Finding::UnsupportedEvidence {
                    case: case_index,
                    tag: tag_index,
                }),
                Some(true) if valid_case => {
                    covered.insert(tag.clone());
                }
                _ => (),
            }
        }
    }
    for missing in obligations(def).difference(&covered) {
        findings.push(Finding::MissingObligation(missing.clone()));
    }
    findings
}
