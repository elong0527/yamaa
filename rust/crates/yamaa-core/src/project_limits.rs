//! Cumulative, checked admission bounds for owned environment semantic inputs.
//! Exhaustion rejects the whole admission before allocating semantic findings.
use crate::{
    project_function::{CallArgument, Definition},
    project_terminology::Source,
    value::Value,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub nodes: usize,
    pub text_bytes: usize,
    pub work: usize,
    pub findings: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            nodes: 262_144,
            text_bytes: 16_777_216,
            work: 67_108_864,
            findings: 65_536,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Nodes,
    TextBytes,
    Work,
    Findings,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limit {
    pub resource: Resource,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdmissionError<F> {
    Limit(Limit),
    Findings(alloc::vec::Vec<F>),
}

pub(crate) struct Budget {
    limits: Limits,
    nodes: usize,
    text: usize,
    work: usize,
    findings: usize,
}
fn add(held: &mut usize, amount: usize, maximum: usize, resource: Resource) -> Result<(), Limit> {
    *held = held
        .checked_add(amount)
        .filter(|&n| n <= maximum)
        .ok_or(Limit { resource })?;
    Ok(())
}
fn product(a: usize, b: usize) -> Result<usize, Limit> {
    a.checked_mul(b).ok_or(Limit {
        resource: Resource::Work,
    })
}
impl Budget {
    pub(crate) fn new(limits: Limits) -> Self {
        Self {
            limits,
            nodes: 0,
            text: 0,
            work: 0,
            findings: 0,
        }
    }
    fn nodes(&mut self, count: usize) -> Result<(), Limit> {
        add(&mut self.nodes, count, self.limits.nodes, Resource::Nodes)
    }
    fn work(&mut self, count: usize) -> Result<(), Limit> {
        add(&mut self.work, count, self.limits.work, Resource::Work)
    }
    pub(crate) fn findings(&mut self, count: usize) -> Result<(), Limit> {
        add(
            &mut self.findings,
            count,
            self.limits.findings,
            Resource::Findings,
        )
    }
    pub(crate) fn text(&mut self, text: &str) -> Result<(), Limit> {
        self.nodes(1)?;
        add(
            &mut self.text,
            text.len(),
            self.limits.text_bytes,
            Resource::TextBytes,
        )?;
        self.work(text.len().max(1))
    }
    fn value(&mut self, value: &Value) -> Result<(), Limit> {
        if let Value::Str(text) = value {
            self.text(text)
        } else {
            self.nodes(1)?;
            self.work(1)
        }
    }
    fn ordered(&mut self, entries: usize, max_bytes: usize) -> Result<(), Limit> {
        let depth = usize::BITS as usize - entries.max(1).leading_zeros() as usize;
        self.work(product(product(entries, depth * 2 + 1)?, max_bytes.max(1))?)
    }
    pub(crate) fn function(&mut self, def: &Definition) -> Result<(), Limit> {
        let start_nodes = self.nodes;
        self.nodes(1)?;
        self.text(&def.name)?;
        self.text(&def.function)?;
        self.text(&def.description)?;
        self.findings(8)?;
        let max_name = def
            .params
            .iter()
            .map(|p| p.name.len())
            .max()
            .unwrap_or(1)
            .max(1);
        for p in &def.params {
            self.nodes(1)?;
            self.text(&p.name)?;
            if let Some(value) = &p.default {
                self.value(value)?;
            }
            self.findings(9)?;
        }
        self.ordered(def.params.len(), max_name)?;
        for case in &def.tests {
            self.nodes(1)?;
            self.text(&case.id)?;
            self.value(&case.result)?;
            self.findings(4)?;
            self.findings(def.params.len())?;
            for (name, value) in &case.args {
                self.text(name)?;
                self.value(value)?;
                self.findings(2)?;
            }
            for tag in &case.covers {
                self.text(tag)?;
                self.findings(2)?;
            }
            self.ordered(
                case.args.len(),
                case.args
                    .iter()
                    .map(|(name, _)| name.len())
                    .max()
                    .unwrap_or(1),
            )?;
            self.ordered(
                case.covers.len(),
                case.covers.iter().map(|tag| tag.len()).max().unwrap_or(1),
            )?;
            let searches = case
                .args
                .len()
                .checked_add(case.covers.len())
                .ok_or(Limit {
                    resource: Resource::Work,
                })?;
            self.work(product(product(searches, def.params.len())?, max_name)?)?;
            self.work(product(def.params.len(), max_name)?)?;
        }
        self.ordered(
            def.tests.len(),
            def.tests
                .iter()
                .map(|case| case.id.len())
                .max()
                .unwrap_or(1),
        )?;
        // Coverage belongs to this function; previous definitions are not revisited.
        // Generated tags add a bounded prefix to the longest parameter name.
        let tag_bytes = max_name.checked_add(32).ok_or(Limit {
            resource: Resource::Work,
        })?;
        self.ordered(self.nodes - start_nodes, tag_bytes)?;
        Ok(())
    }
    pub(crate) fn functions(&mut self, functions: &[Definition]) -> Result<(), Limit> {
        for function in functions {
            self.findings(1)?;
            self.function(function)?;
        }
        self.ordered(
            functions.len(),
            functions
                .iter()
                .map(|function| function.name.len())
                .max()
                .unwrap_or(1),
        )
    }
    pub(crate) fn sources(&mut self, sources: &[Source]) -> Result<(), Limit> {
        let mut lists = 0usize;
        let mut max_name = 1;
        for source in sources {
            self.nodes(1)?;
            if let Some(s) = &source.standard {
                self.text(&s.name)?;
                self.text(&s.publishing_set)?;
                self.text(&s.version)?;
            }
            for list in &source.codelists {
                self.nodes(1)?;
                self.text(&list.id)?;
                self.text(&list.name)?;
                self.findings(5)?;
                lists = lists.checked_add(1).ok_or(Limit {
                    resource: Resource::Nodes,
                })?;
                max_name = max_name.max(list.id.len()).max(list.name.len());
                for text in [&list.alias, &list.format_name].into_iter().flatten() {
                    self.text(text)?;
                }
                if let Some(external) = &list.external {
                    self.text(&external.dictionary)?;
                    self.text(&external.version)?;
                    if let Some(href) = &external.href {
                        self.text(href)?;
                    }
                }
                if let Some(items) = &list.items {
                    let mut max_value = 1;
                    for item in items {
                        self.nodes(1)?;
                        self.value(&item.value)?;
                        self.findings(3)?;
                        if let Value::Str(text) = &item.value {
                            max_value = max_value.max(text.len());
                        }
                        for text in [&item.decode, &item.alias].into_iter().flatten() {
                            self.text(text)?;
                        }
                    }
                    self.ordered(items.len(), max_value)?;
                }
            }
        }
        self.ordered(lists, max_name)
    }
    pub(crate) fn arguments(
        &mut self,
        def: &Definition,
        args: &[CallArgument],
    ) -> Result<(), Limit> {
        self.nodes(args.len())?;
        self.findings(def.params.len())?;
        let max_name = def
            .params
            .iter()
            .map(|p| p.name.len())
            .max()
            .unwrap_or(1)
            .max(1);
        for arg in args {
            self.text(&arg.name)?;
            self.findings(2)?;
        }
        self.work(product(product(args.len(), def.params.len())?, max_name)?)?;
        self.ordered(
            args.len(),
            args.iter().map(|a| a.name.len()).max().unwrap_or(1),
        )
    }
    pub(crate) fn binding(
        &mut self,
        items: &[crate::project_terminology::Item],
        allowed: &[Value],
    ) -> Result<(), Limit> {
        let mut max_bytes = 1;
        for value in items.iter().map(|item| &item.value).chain(allowed.iter()) {
            self.value(value)?;
            if let Value::Str(text) = value {
                max_bytes = max_bytes.max(text.len());
            }
        }
        self.findings(2)?;
        self.ordered(items.len(), max_bytes)?;
        self.ordered(allowed.len(), max_bytes)
    }
}
