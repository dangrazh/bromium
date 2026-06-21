use crate::save_ui_element::SaveUIElement;
use bromium_common::printfmt;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone)]
pub struct UIElementInTree {
    runtime_id: Vec<i32>,
    element_props: SaveUIElement,
    tree_index: usize,
}

impl UIElementInTree {
    pub fn new(element_props: SaveUIElement, tree_index: usize) -> Self {
        let rt_id = element_props.get_runtime_id().to_vec();
        UIElementInTree {
            runtime_id: rt_id,
            element_props,
            tree_index,
        }
    }

    pub fn get_element_props(&self) -> &SaveUIElement {
        &self.element_props
    }

    pub fn get_tree_index(&self) -> usize {
        self.tree_index
    }
}

impl PartialEq for UIElementInTree {
    fn eq(&self, other: &Self) -> bool {
        self.runtime_id == other.runtime_id
    }
}

impl Eq for UIElementInTree {}

impl Hash for UIElementInTree {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.runtime_id.hash(state);
    }
}

/// Reformats a raw XPath string (with multiple `[Attr="value"]` segments per line)
/// into the compact format used by the original C++ implementation.
pub fn match_original_format(xpath: &str) -> String {
    let lines: Vec<&str> = xpath.split('\n').filter(|line| !line.is_empty()).collect();
    let mut elements = Vec::new();
    let mut tag: &str;

    for line in lines {
        if line.is_empty() {
            continue;
        }

        // Extract the tag name (everything before the first '[')
        let tag_end = line.find('[').unwrap_or(line.len());
        if tag_end == 0 || !line.starts_with('/') {
            printfmt!("Skipping malformed line: {}", line);
            continue; // Skip lines that don't start with '/' or are malformed
        }

        // Skip the leading '/', i.e. the 1st character - handle UTF-8 characters (i.e. char boundaries) correctly
        let tag_extracted = line.chars().next().map(|c| &line[c.len_utf8()..tag_end]);
        match tag_extracted {
            None => {
                printfmt!("Skipping malformed line: {}", line);
                continue; // Skip lines that don't have a valid tag
            }
            Some(tag_content) => {
                tag = tag_content;
            }
        }

        // once we reached here, we have a valid tag and can start building the XPath element
        let mut element = format!("/{}", tag);

        // Helper function to extract attribute value and format it with escaped quotes
        let extract_attr = |attr_name: &str, line: &str| -> Option<String> {
            let attr_prefix = format!("[{}=\"", attr_name);
            if let Some(start_idx) = line.find(&attr_prefix) {
                let value_start = start_idx + attr_prefix.len();
                if let Some(end_idx) = line[value_start..].find("\"]") {
                    let value = &line[value_start..value_start + end_idx];
                    // Skip empty attributes
                    if value.is_empty() {
                        return None;
                    }
                    return Some(format!("[@{}=\\\"{}\\\"]", attr_name, value));
                }
            }
            None
        };

        // Helper function to get just the attribute value
        let get_attr_value = |attr_name: &str, line: &str| -> Option<String> {
            let attr_prefix = format!("[{}=\"", attr_name);
            if let Some(start_idx) = line.find(&attr_prefix) {
                let value_start = start_idx + attr_prefix.len();
                if let Some(end_idx) = line[value_start..].find("\"]") {
                    let value = &line[value_start..value_start + end_idx];
                    if !value.is_empty() {
                        return Some(value.to_string());
                    }
                }
            }
            None
        };

        // More complex logic to match original C++ behavior
        if tag == "Pane" || tag == "Window" {
            // Always include ClassName for Pane and Window
            if let Some(class_attr) = extract_attr("ClassName", line) {
                element.push_str(&class_attr);
            }
        } else if tag == "Group" {
            // For Group, only include ClassName if it's "LandmarkTarget"
            if let Some(class_value) = get_attr_value("ClassName", line)
                && class_value == "LandmarkTarget"
                && let Some(class_attr) = extract_attr("ClassName", line)
            {
                element.push_str(&class_attr);
            }
            // Skip ClassName for Group elements with other classes like "NamedContainerAutomationPeer"
        }
        // For other elements like Button and Custom, don't include ClassName

        // Add Name attribute (if non-empty)
        if let Some(name_attr) = extract_attr("Name", line) {
            element.push_str(&name_attr);
        }

        // Add AutomationId attribute (if non-empty)
        if let Some(id_attr) = extract_attr("AutomationId", line) {
            element.push_str(&id_attr);
        }

        elements.push(element);
    }

    // Join all elements into a single XPath string
    elements.join("")
}
