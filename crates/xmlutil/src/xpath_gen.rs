use std::collections::HashMap;

use roxmltree::{Document, Node};

#[derive(Debug, thiserror::Error)]
pub enum XpathGenError {
    #[error("XML parse error: {0}")]
    XmlParseError(String),
    #[error("UI Element with runtime ID '{0}' not found")]
    ElementNotFound(String),
}

/// Encode a value as an XPath string literal, handling embedded quotes.
///
/// - If the value contains no single quotes, wrap in single quotes: `'value'`
/// - If the value contains no double quotes, wrap in double quotes: `"value"`
/// - If both, emit a `concat(...)` expression that splits around the quotes.
fn xpath_string_literal(value: &str) -> String {
    if !value.contains('\'') {
        format!("'{}'", value)
    } else if !value.contains('"') {
        format!("\"{}\"", value)
    } else {
        // Value contains both quote types — use concat()
        let mut parts = Vec::new();
        let mut remaining = value;
        while !remaining.is_empty() {
            if let Some(pos) = remaining.find('\'') {
                if pos > 0 {
                    parts.push(format!("'{}'", &remaining[..pos]));
                }
                parts.push("\"'\"".to_string());
                remaining = &remaining[pos + 1..];
            } else {
                parts.push(format!("'{}'", remaining));
                remaining = "";
            }
        }
        format!("concat({})", parts.join(","))
    }
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
            let lit = xpath_string_literal(node.attribute(attr).unwrap());
            return format!("//*[@{}={}]", attr, lit);
        }
    }

    let mut path_parts = Vec::new();
    let mut current = Some(node);

    while let Some(n) = current {
        if n.is_element() {
            let tag = n.tag_name().name();

            if !simple_xpath && is_attribute_with_ct_unique(index, n) {
                let lit = xpath_string_literal(n.attribute("Name").unwrap());
                path_parts.push(format!("{}[@Name={}]", tag, lit));
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

/// Prefer a named descendant unique within its outermost Window ancestor.
/// The caller must supply complete window coverage before relying on uniqueness.
/// Ambiguous/unnamed targets retain the full-path fallback.
pub fn get_xpath_window_scoped_from_runtime_id(
    runtime_id: &str,
    xml: &str,
) -> Result<String, XpathGenError> {
    let doc = Document::parse(xml).map_err(|e| XpathGenError::XmlParseError(e.to_string()))?;
    let index = AttributeIndex::build(&doc);
    let target = doc
        .descendants()
        .find(|n| n.attribute("RtID") == Some(runtime_id))
        .ok_or_else(|| XpathGenError::ElementNotFound(runtime_id.to_string()))?;
    if let Some(window) = target
        .ancestors()
        .filter(|n| n.has_tag_name("Window"))
        .last()
        && window != target
        && let Some(name) = target.attribute("Name").filter(|name| !name.is_empty())
        && window
            .descendants()
            .filter(|n| {
                *n != window
                    && n.has_tag_name(target.tag_name().name())
                    && n.attribute("Name") == Some(name)
            })
            .count()
            == 1
    {
        let anchor = get_xpath_robula(&index, window, false);
        return Ok(format!(
            "{anchor}//{}[@Name={}]",
            target.tag_name().name(),
            xpath_string_literal(name)
        ));
    }
    Ok(get_xpath_robula(&index, target, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_scoped_locator_survives_tooltip_and_ignores_other_windows() {
        let xml = r#"<Pane Name="Desktop" ControlType="Pane"><Window Name="Notepad" ControlType="Window"><Pane/><Pane><Button Name="Einstellungen" ControlType="Button" RtID="settings"/></Pane></Window><Window Name="Other" ControlType="Window"><Button Name="Einstellungen" ControlType="Button"/></Window></Pane>"#;
        let popup = xml.replacen("<Pane/>", "<Pane Name=\"PopupHost\"/><Pane/>", 1);
        let path = get_xpath_window_scoped_from_runtime_id("settings", xml).unwrap();
        assert_eq!(
            path,
            "/Pane[@Name='Desktop']/Window[@Name='Notepad']//Button[@Name='Einstellungen']"
        );
        assert_eq!(
            path,
            get_xpath_window_scoped_from_runtime_id("settings", &popup).unwrap()
        );
        for snapshot in [xml, popup.as_str()] {
            let result =
                crate::xpath_eval::eval_xpath_thread_cached(&format!("({path})/@RtID"), snapshot);
            assert!(result.is_success(), "{}", result.get_error_msg());
            assert_eq!(result.get_result_items().len(), 1);
            assert_eq!(result.get_result_items()[0].get_item_value(), "settings");
        }
        assert_ne!(
            get_xpath_full_from_runtime_id("settings", xml, true).unwrap(),
            get_xpath_full_from_runtime_id("settings", &popup, true).unwrap()
        );
    }

    #[test]
    fn duplicate_names_in_window_fall_back_to_full_path() {
        let xml = r#"<Pane><Window Name="App" ControlType="Window"><Pane><Button Name="OK" ControlType="Button" RtID="a"/><Button Name="OK" ControlType="Button" RtID="b"/></Pane></Window></Pane>"#;
        assert_eq!(
            get_xpath_window_scoped_from_runtime_id("b", xml).unwrap(),
            get_xpath_full_from_runtime_id("b", xml, false).unwrap()
        );
    }

    #[test]
    fn scoped_name_quotes_and_duplicate_window_titles_resolve_uniquely() {
        let xml = r#"<Pane><Window Name="App" ControlType="Window"/><Window Name="App" ControlType="Window"><Pane><Button Name="It&apos;s &quot;OK&quot;" ControlType="Button" RtID="b"/></Pane></Window></Pane>"#;
        let path = get_xpath_window_scoped_from_runtime_id("b", xml).unwrap();
        assert!(path.starts_with("/Pane/Window[2]//Button["));
        let result = crate::xpath_eval::eval_xpath_thread_cached(&format!("({path})/@RtID"), xml);
        assert!(result.is_success(), "{}", result.get_error_msg());
        assert_eq!(result.get_result_items().len(), 1);
        assert_eq!(result.get_result_items()[0].get_item_value(), "b");
    }

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

    // ─── xpath_string_literal tests ──────────────────────────────────────────

    #[test]
    fn test_xpath_literal_no_quotes() {
        assert_eq!(xpath_string_literal("hello"), "'hello'");
    }

    #[test]
    fn test_xpath_literal_with_apostrophe() {
        assert_eq!(xpath_string_literal("Bob's App"), "\"Bob's App\"");
    }

    #[test]
    fn test_xpath_literal_with_double_quote() {
        assert_eq!(xpath_string_literal(r#"Say "Hi""#), r#"'Say "Hi"'"#);
    }

    #[test]
    fn test_xpath_literal_with_both_quotes() {
        let result = xpath_string_literal(r#"It's a "test""#);
        assert_eq!(result, r#"concat('It',"'",'s a "test"')"#);
    }

    #[test]
    fn test_xpath_with_apostrophe_in_name() {
        // Name with apostrophe should produce double-quoted XPath literal
        let xml = r#"<Root ControlType="Window" Name="Bob&apos;s App" RtID="rt-root">
  <Button ControlType="Button" Name="Click" RtID="rt-btn"/>
</Root>"#;
        let result = get_xpath_full_from_runtime_id("rt-root", xml, false).unwrap();
        assert_eq!(result, r#"/Root[@Name="Bob's App"]"#);
    }
}
