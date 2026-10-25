//! Authored document trees with stable child-before-parent arena order.
use super::document_schema::{Document, DocumentNode as N};

#[derive(Clone, Debug)]
pub enum Tree {
    Scalar(N),
    Map(Vec<(String, Tree)>),
    List(Vec<Tree>),
    Pairs(Vec<(Tree, Tree)>),
}
impl Tree {
    pub fn text(value: impl AsRef<str>) -> Self {
        Self::Scalar(N::Text(value.as_ref().into()))
    }
    pub fn integer(value: impl ToString) -> Self {
        Self::Scalar(N::Integer(value.to_string()))
    }
    pub fn float(value: f64) -> Self {
        Self::Scalar(N::Float(value))
    }
    pub fn boolean(value: bool) -> Self {
        Self::Scalar(N::Boolean(value))
    }
    pub fn map(fields: Vec<(&str, Self)>) -> Self {
        Self::Map(
            fields
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }
    pub fn list(items: Vec<Self>) -> Self {
        Self::List(items)
    }
    pub fn at(&mut self, path: &[&str]) -> &mut Self {
        let Some((field, tail)) = path.split_first() else {
            return self;
        };
        match self {
            Self::Map(fields) => fields
                .iter_mut()
                .find(|(key, _)| key == field)
                .expect(field)
                .1
                .at(tail),
            Self::List(items) => items[field.parse::<usize>().unwrap()].at(tail),
            _ => panic!("scalar path {path:?}"),
        }
    }
    pub fn set(&mut self, path: &[&str], value: Self) {
        let (key, head) = path.split_last().unwrap();
        match self.at(head) {
            Self::Map(fields) => {
                if let Some((_, old)) = fields.iter_mut().find(|(name, _)| name == key) {
                    *old = value;
                } else {
                    fields.push(((*key).into(), value));
                }
            }
            _ => panic!("set needs map"),
        }
    }
    pub fn remove(&mut self, path: &[&str]) {
        let (key, head) = path.split_last().unwrap();
        match self.at(head) {
            Self::Map(fields) => fields.retain(|(name, _)| name != key),
            _ => panic!("remove needs map"),
        }
    }
    pub fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            Self::Scalar(node) => node,
            Self::Pairs(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(key, value)| (key.append(nodes), value.append(nodes)))
                    .collect(),
            ),
            Self::List(items) => {
                N::Sequence(items.into_iter().map(|item| item.append(nodes)).collect())
            }
            Self::Map(fields) => N::Mapping(
                fields
                    .into_iter()
                    .map(|(key, value)| (Self::text(key).append(nodes), value.append(nodes)))
                    .collect(),
            ),
        };
        let index = nodes.len();
        nodes.push(node);
        index
    }
    pub fn document(self) -> Document {
        let mut nodes = Vec::new();
        let root = self.append(&mut nodes);
        Document::new(nodes, root).unwrap()
    }
}
