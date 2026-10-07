//! Raw callbacks run on the R thread; wrapper-held conditions survive native return.
use crate::specification_service::{boundary, capture_inputs, store_document};
use extendr_api::prelude::*;
use yamaa_adapters::{
    specification_diagnostics::inheritance_failure, specification_source::InheritancePort,
};
use yamaa_engine::inheritance::{Source, SourceError};

struct Port(Function);
impl Port {
    fn ask(
        &self,
        operation: &str,
        args: List,
        maximum: usize,
    ) -> std::result::Result<Robj, String> {
        let result = self
            .0
            .call(pairlist!(
                operation = operation,
                args = args,
                maximum = maximum as i32
            ))
            .map_err(|_| "inheritance dispatcher failed")?;
        let result = result
            .as_list()
            .filter(|v| v.len() == 2)
            .ok_or("invalid inheritance response")?;
        if result
            .elt(0)
            .map_err(|_| "missing inheritance status")?
            .as_integer()
            != Some(0)
        {
            return Err("inheritance callback failed".into());
        }
        result
            .elt(1)
            .map_err(|_| "missing inheritance response".into())
    }
}
fn text(value: &Robj, maximum: usize) -> std::result::Result<String, String> {
    let bytes = value.as_raw().ok_or("inheritance text must be raw UTF-8")?;
    if bytes.len() > maximum {
        return Err("inheritance text limit".into());
    }
    std::str::from_utf8(bytes.as_slice())
        .map(str::to_owned)
        .map_err(|_| "invalid inheritance UTF-8".into())
}
fn raw(value: &str) -> Raw {
    Raw::from_bytes(value.as_bytes())
}
fn available<T>(
    result: std::result::Result<Option<T>, String>,
) -> std::result::Result<T, SourceError<String>> {
    result
        .map_err(SourceError::Raised)?
        .ok_or(SourceError::Unavailable)
}
impl InheritancePort for Port {
    type Error = String;
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> std::result::Result<Source, SourceError<String>> {
        available((|| {
            let result = self.ask("canonicalize", list!(raw(declaring), raw(written)), 65_536)?;
            if result.is_null() {
                return Ok(None);
            }
            let result = result
                .as_list()
                .filter(|v| v.len() == 2)
                .ok_or("invalid inheritance identity")?;
            Ok(Some(Source {
                identity: text(&result.elt(0).map_err(|_| "missing identity")?, 65_536)?,
                display_path: text(&result.elt(1).map_err(|_| "missing display path")?, 65_536)?,
            }))
        })())
    }
    fn capture(
        &mut self,
        source: &Source,
        maximum: usize,
    ) -> std::result::Result<Vec<u8>, SourceError<String>> {
        available((|| {
            let result = self.ask(
                "capture",
                list!(raw(&source.identity), raw(&source.display_path)),
                maximum,
            )?;
            if result.is_null() {
                return Ok(None);
            }
            let bytes = result
                .as_raw()
                .ok_or("inheritance source must be raw YAML")?;
            if bytes.len() > maximum {
                return Err("inheritance capture limit".into());
            }
            Ok(Some(bytes.as_slice().to_vec()))
        })())
    }
    fn rebase(
        &mut self,
        layer: &Source,
        entry: &Source,
        written: &str,
        maximum: usize,
    ) -> std::result::Result<String, String> {
        text(
            &self.ask(
                "rebase",
                list!(raw(&layer.identity), raw(&entry.identity), raw(written)),
                maximum,
            )?,
            maximum,
        )
    }
}
#[extendr]
fn prepare_inherited_specification(
    names: List,
    modules: List,
    entry: i32,
    identity: Raw,
    source: Raw,
    dispatch: Function,
) -> List {
    boundary(|| {
        let (schema, source) = capture_inputs(&names, &modules, entry, &identity, &source)?;
        let display = source.identity.clone();
        let document = schema
            .prepare_inherited(source, display, &mut Port(dispatch))
            .map_err(|error| match inheritance_failure(error) {
                Ok(message) | Err(message) => message,
            })?;
        store_document(document)
    })
}
extendr_module! {mod specification_inheritance; fn prepare_inherited_specification;}
