use std::collections::HashMap;

use roxmltree::{Document, Node};

#[derive(Debug, thiserror::Error)]
pub enum XpathGenError {
    #[error("XML parse error: {0}")]
    XmlParseError(String),
    #[error("UI Element with runtime ID '{0}' not found")]
    ElementNotFound(String),
}

struct AttributeIndex {
    id_counts: HashMap<String, usize>,
    name_counts: HashMap<String, usize>,
    ct_name_counts: HashMap<(String, String), usize>,
}

impl AttributeIndex {
    fn build(doc: &Document) -> Self {
        let mut id_counts: HashMap<String, usize> = HashMap::new();
        let mut name_counts: HashMap<String, usize> = HashMap::new();
        let mut ct_name_counts: HashMap<(String, String), usize> = HashMap::new();

        for node in doc.descendants() {
            if let Some(id) = node.attribute("id") {
                *id_counts.entry(id.to_string()).or_default() += 1;
            }
            if let Some(name) = node.attribute("name") {
                *name_counts.entry(name.to_string()).or_default() += 1;
            }
            if let Some(ct) = node.attribute("ControlType")
                && let Some(name) = node.attribute("Name")
            {
                *ct_name_counts
                    .entry((ct.to_string(), name.to_string()))
                    .or_default() += 1;
            }
        }

        Self {
            id_counts,
            name_counts,
            ct_name_counts,
        }
    }
}

fn is_attribute_unique(index: &AttributeIndex, node: Node, attr_name: &str) -> bool {
    let counts = match attr_name {
        "id" => &index.id_counts,
        "name" => &index.name_counts,
        _ => return false,
    };
    node.attribute(attr_name)
        .is_some_and(|val| counts.get(val) == Some(&1))
}

fn is_attribute_with_ct_unique(index: &AttributeIndex, node: Node) -> bool {
    if let Some(name) = node.attribute("Name")
        && let Some(ct) = node.attribute("ControlType")
    {
        let key = (ct.to_string(), name.to_string());
        return index.ct_name_counts.get(&key) == Some(&1);
    }
    false
}

/// Generate a robust, ROBULA+-like XPath for the given node.
fn get_xpath_robula(index: &AttributeIndex, node: Node, simple_xpath: bool) -> String {
    for attr in ["id", "name"] {
        if is_attribute_unique(index, node, attr) {
            return format!("//*[@{}='{}']", attr, node.attribute(attr).unwrap());
        }
    }

    let mut path_parts = Vec::new();
    let mut current = Some(node);

    while let Some(n) = current {
        if n.is_element() {
            let tag = n.tag_name().name();

            if !simple_xpath && is_attribute_with_ct_unique(index, n) {
                path_parts.push(format!("{}[@Name='{}']", tag, n.attribute("Name").unwrap()));
            } else {
                let parent = n.parent();
                let same_tag_count = parent.map_or(1, |p| {
                    p.children()
                        .filter(|c| c.is_element() && c.tag_name().name() == tag)
                        .count()
                });

                if same_tag_count > 1 {
                    let mut index = 1;
                    let mut prev = n.prev_sibling();
                    while let Some(sib) = prev {
                        if sib.is_element() && sib.tag_name().name() == tag {
                            index += 1;
                        }
                        prev = sib.prev_sibling();
                    }
                    path_parts.push(format!("{}[{}]", tag, index));
                } else {
                    path_parts.push(tag.to_string());
                }
            }
        }
        current = n.parent();
    }

    path_parts.reverse();
    format!("/{}", path_parts.join("/"))
}

pub fn get_xpath_full_from_runtime_id(
    runtime_id: &str,
    xml: &str,
    simple_path: bool,
) -> Result<String, XpathGenError> {
    let doc = Document::parse(xml).map_err(|e| XpathGenError::XmlParseError(e.to_string()))?;
    let index = AttributeIndex::build(&doc);

    if let Some(node_id) = doc
        .descendants()
        .find(|n| n.attribute("RtID") == Some(runtime_id))
    {
        Ok(get_xpath_robula(&index, node_id, simple_path))
    } else {
        Err(XpathGenError::ElementNotFound(runtime_id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_XML: &str = r#"<Root ControlType="Window" Name="MainWindow" RtID="rt-root">
  <Panel ControlType="Panel" Name="Header" RtID="rt-header">
    <Button ControlType="Button" Name="OK" RtID="rt-ok"/>
    <Button ControlType="Button" Name="Cancel" RtID="rt-cancel"/>
  </Panel>
  <Panel ControlType="Panel" Name="Content" RtID="rt-content">
    <TextBox ControlType="Edit" Name="Username" id="txt-user" RtID="rt-user"/>
    <TextBox ControlType="Edit" Name="OK" RtID="rt-ok-text"/>
  </Panel>
  <List ControlType="List" Name="Items" RtID="rt-list">
    <Item RtID="rt-item1"/>
    <Item RtID="rt-item2"/>
    <Item RtID="rt-item3"/>
  </List>
</Root>"#;

    fn find_node_by_rtid<'a>(doc: &'a Document<'a>, rtid: &str) -> Node<'a, 'a> {
        doc.descendants()
            .find(|n| n.attribute("RtID") == Some(rtid))
            .unwrap()
    }

    #[test]
    fn test_unique_id_gives_shortcut_xpath() {
        let doc = Document::parse(TEST_XML).unwrap();
        let index = AttributeIndex::build(&doc);
        let node = find_node_by_rtid(&doc, "rt-user");
        assert!(is_attribute_unique(&index, node, "id"));
        let xpath = get_xpath_robula(&index, node, false);
        assert_eq!(xpath, "//*[@id='txt-user']");
    }

    #[test]
    fn test_non_unique_name_is_not_shortcut() {
        let doc = Document::parse(TEST_XML).unwrap();
        let index = AttributeIndex::build(&doc);
        let button_ok = find_node_by_rtid(&doc, "rt-ok");
        assert!(!is_attribute_unique(&index, button_ok, "Name"));
    }

    #[test]
    fn test_ct_name_unique() {
        let doc = Document::parse(TEST_XML).unwrap();
        let index = AttributeIndex::build(&doc);
        let button_ok = find_node_by_rtid(&doc, "rt-ok");
        assert!(is_attribute_with_ct_unique(&index, button_ok));
    }

    #[test]
    fn test_ct_name_unique_different_control_types() {
        let doc = Document::parse(TEST_XML).unwrap();
        let index = AttributeIndex::build(&doc);
        let text_ok = find_node_by_rtid(&doc, "rt-ok-text");
        assert!(is_attribute_with_ct_unique(&index, text_ok));
    }

    #[test]
    fn test_ct_name_not_unique_without_attribute() {
        let doc = Document::parse(TEST_XML).unwrap();
        let index = AttributeIndex::build(&doc);
        let item = find_node_by_rtid(&doc, "rt-item1");
        assert!(!is_attribute_with_ct_unique(&index, item));
    }

    #[test]
    fn test_full_path_with_ct_name() {
        let result = get_xpath_full_from_runtime_id("rt-ok", TEST_XML, false).unwrap();
        assert_eq!(
            result,
            "/Root[@Name='MainWindow']/Panel[@Name='Header']/Button[@Name='OK']"
        );
    }

    #[test]
    fn test_full_path_unique_name_cancel() {
        let result = get_xpath_full_from_runtime_id("rt-cancel", TEST_XML, false).unwrap();
        assert_eq!(
            result,
            "/Root[@Name='MainWindow']/Panel[@Name='Header']/Button[@Name='Cancel']"
        );
    }

    #[test]
    fn test_indexed_siblings_simple_path() {
        let result = get_xpath_full_from_runtime_id("rt-item2", TEST_XML, true).unwrap();
        assert_eq!(result, "/Root/List/Item[2]");
    }

    #[test]
    fn test_simple_vs_full_path_differ() {
        let simple = get_xpath_full_from_runtime_id("rt-item2", TEST_XML, true).unwrap();
        let full = get_xpath_full_from_runtime_id("rt-item2", TEST_XML, false).unwrap();
        assert_ne!(simple, full);
        assert_eq!(simple, "/Root/List/Item[2]");
        assert_eq!(
            full,
            "/Root[@Name='MainWindow']/List[@Name='Items']/Item[2]"
        );
    }

    #[test]
    fn test_unique_id_shortcut_via_public_api() {
        let result = get_xpath_full_from_runtime_id("rt-user", TEST_XML, false).unwrap();
        assert_eq!(result, "//*[@id='txt-user']");
    }

    #[test]
    fn test_runtime_id_not_found() {
        let result = get_xpath_full_from_runtime_id("nonexistent", TEST_XML, false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, XpathGenError::ElementNotFound(_)));
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn test_malformed_xml() {
        let result = get_xpath_full_from_runtime_id("x", "<bad", false);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            XpathGenError::XmlParseError(_)
        ));
    }

    #[test]
    fn test_root_element_xpath() {
        let result = get_xpath_full_from_runtime_id("rt-root", TEST_XML, false).unwrap();
        assert_eq!(result, "/Root[@Name='MainWindow']");
    }

    #[test]
    fn test_first_sibling_index_is_one() {
        let result = get_xpath_full_from_runtime_id("rt-item1", TEST_XML, true).unwrap();
        assert_eq!(result, "/Root/List/Item[1]");
    }

    #[test]
    fn test_third_sibling_index() {
        let result = get_xpath_full_from_runtime_id("rt-item3", TEST_XML, true).unwrap();
        assert_eq!(result, "/Root/List/Item[3]");
    }
}
