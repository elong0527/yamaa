//! Decode a normalized codelist source while retaining authored source/list order.
use crate::{
    project_function_document::{Finding, Kind, Reader},
    project_terminology::{Codelist, DataType, External, Item, Source, Standard},
    schema::Document,
};
use alloc::{format, vec::Vec};

pub fn decode(document: &Document, root: usize) -> Result<Source, Vec<Finding>> {
    let mut reader = Reader::new(document);
    let standard = document.field(root, "standard").map(|node| Standard {
        name: reader.text_field(node, "name", "standard.name"),
        publishing_set: reader.text_field(node, "publishing_set", "standard.publishing_set"),
        version: reader.text_field(node, "version", "standard.version"),
    });
    let mut codelists = Vec::new();
    for (index, node) in reader
        .sequence(root, "codelists", "codelists")
        .into_iter()
        .enumerate()
    {
        let path = format!("codelists[{index}]");
        let id = reader.text_field(node, "id", &format!("{path}.id"));
        let name = reader.text_field(node, "name", &format!("{path}.name"));
        let data_type = match reader
            .text_field(node, "data_type", &format!("{path}.data_type"))
            .as_str()
        {
            "text" => DataType::Text,
            "integer" => DataType::Integer,
            "float" => DataType::Float,
            _ => {
                reader.fault(&format!("{path}.data_type"), node, Kind::NormalizedShape);
                DataType::Text
            }
        };
        let extensible = reader.boolean(node, "extensible", &format!("{path}.extensible"));
        let alias = reader.optional_text(node, "alias", &format!("{path}.alias"));
        let format_name = reader.optional_text(node, "format_name", &format!("{path}.format_name"));
        let items = document.field(node, "items").map(|_| {
            reader
                .sequence(node, "items", &format!("{path}.items"))
                .into_iter()
                .enumerate()
                .map(|(index, node)| {
                    let path = format!("{path}.items[{index}]");
                    let value = reader
                        .field(node, "value", &format!("{path}.value"))
                        .map(|node| reader.scalar(node, &format!("{path}.value")))
                        .unwrap_or(crate::value::Value::Missing);
                    let decode = reader.optional_text(node, "decode", &format!("{path}.decode"));
                    let rank = document
                        .field(node, "rank")
                        .and_then(|node| reader.integer(node, &format!("{path}.rank")));
                    let alias = reader.optional_text(node, "alias", &format!("{path}.alias"));
                    let extended = reader.boolean(node, "extended", &format!("{path}.extended"));
                    Item {
                        value,
                        decode,
                        rank,
                        alias,
                        extended,
                    }
                })
                .collect()
        });
        let external = document.field(node, "external").map(|node| External {
            dictionary: reader.text_field(
                node,
                "dictionary",
                &format!("{path}.external.dictionary"),
            ),
            version: reader.text_field(node, "version", &format!("{path}.external.version")),
            href: reader.optional_text(node, "href", &format!("{path}.external.href")),
        });
        codelists.push(Codelist {
            id,
            name,
            data_type,
            extensible,
            alias,
            format_name,
            items,
            external,
        });
    }
    let findings = reader.finish();
    if findings.is_empty() {
        Ok(Source {
            standard,
            codelists,
        })
    } else {
        Err(findings)
    }
}
