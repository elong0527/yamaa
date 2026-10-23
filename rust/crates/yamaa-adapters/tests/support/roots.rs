//! Project functions and codelist sources written as plain test data, lowered
//! into the AST roots a project file's front end produces
//! (`yamaa_core::ast::project`). Every node takes the root path and no
//! position, as a hand-built document without positions lowers.
pub use yamaa_core::ast::CodelistType;
use yamaa_core::{
    ast::{self, Meta, Named, Paths, Span, Text},
    project_function::Language,
    value::{ColumnType, Value, ValueType},
};

pub fn meta() -> Meta {
    Meta {
        path: Paths::new().root(),
        span: Span::UNKNOWN,
    }
}
pub fn named(name: &str) -> Named {
    Named {
        name: name.into(),
        meta: meta(),
    }
}
pub fn text(text: &str) -> Text {
    Text {
        text: text.into(),
        meta: meta(),
    }
}
fn optional(value: Option<String>) -> Option<Text> {
    value.map(|value| text(&value))
}

/// One project function, as a function file writes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Definition {
    pub name: String,
    pub function: String,
    pub description: String,
    pub params: Vec<Parameter>,
    pub returns: ColumnType,
    pub may_return_missing: bool,
    pub comparison_decimals: i64,
    pub tests: Vec<Case>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub kind: ValueType,
    pub required: bool,
    pub default: Option<Value>,
    pub accepts_missing: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Case {
    pub id: String,
    pub covers: Vec<String>,
    pub args: Vec<(String, Value)>,
    pub result: Value,
}
impl Definition {
    /// The root of the function file defining this function, which names
    /// `function` as the callable `language` binds and no other (D8).
    pub fn root(self, language: Language) -> ast::Function {
        ast::Function {
            header: ast::Header {
                version: text("1.0"),
                meta: meta(),
            },
            name: named(&self.name),
            description: text(&self.description),
            params: self.params.into_iter().map(Parameter::root).collect(),
            returns: self.returns,
            may_return_missing: self.may_return_missing,
            comparison_decimals: self.comparison_decimals,
            implementations: vec![ast::Implementation {
                language,
                callable: text(&self.function),
            }],
            tests: self.tests.into_iter().map(Case::root).collect(),
            paths: Paths::new(),
            meta: meta(),
        }
    }
}

impl Definition {
    /// The root of the function file defining this function, which names
    /// the callable of each host language it implements; `function` is
    /// unused.
    pub fn file(self, implementations: &[(Language, &str)]) -> ast::Function {
        let mut root = self.root(Language::Python);
        root.implementations = implementations
            .iter()
            .map(|&(language, callable)| ast::Implementation {
                language,
                callable: text(callable),
            })
            .collect();
        root
    }
}

impl Definition {
    /// The plain data of a lowered definition, without its paths, for
    /// comparing with authored expectations. `function` is the first
    /// callable the definition writes.
    pub fn of(root: &ast::Function) -> Self {
        Self {
            name: root.name.name.clone(),
            function: root
                .implementations
                .first()
                .map(|implementation| implementation.callable.text.clone())
                .unwrap_or_default(),
            description: root.description.text.clone(),
            params: root
                .params
                .iter()
                .map(|parameter| Parameter {
                    name: parameter.name.name.clone(),
                    kind: parameter.kind,
                    required: parameter.required,
                    default: parameter.default.clone(),
                    accepts_missing: parameter.accepts_missing,
                })
                .collect(),
            returns: root.returns,
            may_return_missing: root.may_return_missing,
            comparison_decimals: root.comparison_decimals,
            tests: root
                .tests
                .iter()
                .map(|case| Case {
                    id: case.id.text.clone(),
                    covers: case.covers.iter().map(|tag| tag.text.clone()).collect(),
                    args: case
                        .args
                        .iter()
                        .map(|(name, value)| (name.name.clone(), value.clone()))
                        .collect(),
                    result: case.result.clone(),
                })
                .collect(),
        }
    }
}
impl Parameter {
    pub fn root(self) -> ast::Parameter {
        ast::Parameter {
            name: named(&self.name),
            kind: self.kind,
            required: self.required,
            default: self.default,
            accepts_missing: self.accepts_missing,
            meta: meta(),
        }
    }
}
impl Case {
    pub fn root(self) -> ast::FunctionTest {
        ast::FunctionTest {
            id: text(&self.id),
            covers: self.covers.iter().map(|tag| text(tag)).collect(),
            args: self
                .args
                .into_iter()
                .map(|(name, value)| (named(&name), value))
                .collect(),
            result: self.result,
            meta: meta(),
        }
    }
}

/// One codelist source.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub standard: Option<Standard>,
    pub codelists: Vec<Codelist>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Standard {
    pub name: String,
    pub publishing_set: String,
    pub version: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Codelist {
    pub id: String,
    pub name: String,
    pub data_type: ast::CodelistType,
    pub extensible: bool,
    pub alias: Option<String>,
    pub format_name: Option<String>,
    pub items: Option<Vec<Item>>,
    pub external: Option<External>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub value: Value,
    pub decode: Option<String>,
    pub rank: Option<i64>,
    pub alias: Option<String>,
    pub extended: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct External {
    pub dictionary: String,
    pub version: String,
    pub href: Option<String>,
}
impl Standard {
    pub fn root(self) -> ast::TerminologyStandard {
        ast::TerminologyStandard {
            name: text(&self.name),
            publishing_set: text(&self.publishing_set),
            version: text(&self.version),
            meta: meta(),
        }
    }
}
fn plain(text: &Option<Text>) -> Option<String> {
    text.as_ref().map(|text| text.text.clone())
}
impl Source {
    /// The plain data of a lowered codelist source, without its paths.
    pub fn of(root: &ast::Codelists) -> Self {
        Self {
            standard: root.standard.as_ref().map(|standard| Standard {
                name: standard.name.text.clone(),
                publishing_set: standard.publishing_set.text.clone(),
                version: standard.version.text.clone(),
            }),
            codelists: root
                .codelists
                .iter()
                .map(|list| Codelist {
                    id: list.id.name.clone(),
                    name: list.name.text.clone(),
                    data_type: list.data_type,
                    extensible: list.extensible,
                    alias: plain(&list.alias),
                    format_name: plain(&list.format_name),
                    items: list.items.as_ref().map(|items| {
                        items
                            .iter()
                            .map(|item| Item {
                                value: item.value.clone(),
                                decode: plain(&item.decode),
                                rank: item.rank,
                                alias: plain(&item.alias),
                                extended: item.extended,
                            })
                            .collect()
                    }),
                    external: list.external.as_ref().map(|external| External {
                        dictionary: external.dictionary.text.clone(),
                        version: external.version.text.clone(),
                        href: plain(&external.href),
                    }),
                })
                .collect(),
        }
    }
    /// The codelist source root.
    pub fn root(self) -> ast::Codelists {
        ast::Codelists {
            header: None,
            standard: self.standard.map(Standard::root),
            codelists: self
                .codelists
                .into_iter()
                .map(|list| ast::Codelist {
                    id: named(&list.id),
                    name: text(&list.name),
                    data_type: list.data_type,
                    extensible: list.extensible,
                    alias: optional(list.alias),
                    format_name: optional(list.format_name),
                    items: list.items.map(|items| {
                        items
                            .into_iter()
                            .map(|item| ast::CodelistItem {
                                value: item.value,
                                decode: optional(item.decode),
                                rank: item.rank,
                                alias: optional(item.alias),
                                extended: item.extended,
                                meta: meta(),
                            })
                            .collect()
                    }),
                    external: list.external.map(|external| ast::ExternalDictionary {
                        dictionary: text(&external.dictionary),
                        version: text(&external.version),
                        href: optional(external.href),
                        meta: meta(),
                    }),
                    meta: meta(),
                })
                .collect(),
            paths: Paths::new(),
            meta: meta(),
        }
    }
}
