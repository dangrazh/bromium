
use roxmltree::{Document, Node};

/// Check if an attribute uniquely identifies the node among all nodes in the document.
fn is_attribute_unique(doc: &Document, node: Node, attr_name: &str) -> bool {
    if let Some(attr_value) = node.attribute(attr_name) {
        let count = doc
            .descendants()
            .filter(|n| n.attribute(attr_name) == Some(attr_value))
            .count();
        eprintln!("Attribute '{}' with value '{}' found {} times", attr_name, attr_value, count);
        if count == 1 {
            return true;
        } else {
            return false;
        }

    }
    // Attribute not present - return false
    false
}

fn is_attribute_with_ct_unique(doc: &Document, node: Node, attr_name: &str) -> bool {
    if let Some(attr_value) = node.attribute(attr_name) {
        if let Some(ct_value) = node.attribute("ControlType") {
            let count = doc
                .descendants()
                .filter(|n| n.attribute(attr_name) == Some(attr_value) && n.attribute("ControlType") == Some(ct_value))
                .count();
            eprintln!("Attribute '{}' with value '{}' found {} times", attr_name, attr_value, count);
        
            if count == 1 {
                return true;
            } else {
                return false;
            }
        }
    }
    // Attribute not present - return false
    false
}

/// Generate a robust, ROBULA+-like XPath for the given node.
fn get_xpath_robula(doc: &Document, node: Node, simple_xpath: bool) -> String {
    // Rule 1: Prefer globally unique attribute
    for attr in ["id", "name"] {
        if is_attribute_unique(doc, node, attr) {
            return format!("//*[@{}='{}']", attr, node.attribute(attr).unwrap());
        }
    }

    // Build full path up to the root with optimization rules
    let mut path_parts = Vec::new();
    let mut current = Some(node);

    while let Some(n) = current {
        if n.is_element() {
            let tag = n.tag_name().name();

            // Try using unique attribute in parent scope
            eprintln!("Checking Name attribute for node: {}", tag);
            if !simple_xpath && is_attribute_with_ct_unique(doc, n, "Name") {
                eprintln!("Using Name attribute for node: {}", tag.to_string());
                path_parts.push(format!("{}[@Name='{}']", tag, n.attribute("Name").unwrap()));
            } else {
                eprintln!("Cannot use Name attribute for node: {}", tag);
                // Determine if this node needs an index
                let parent = n.parent();
                let same_tag_count = parent.map_or(1, |p| {
                    p.children()
                        .filter(|c| c.is_element() && c.tag_name().name() == tag)
                        .count()
                });

                if same_tag_count > 1 {
                    // Count this node's position among siblings
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



// pub fn get_xpath_from_runtime_id(runtime_id: String, xml: &str) -> String {

//     let root_node = RNode::new_document();
//     parse(root_node.clone(), xml, None).unwrap();

//     let target = root_node
//     .descend_iter()
//     .find(|n| n.attribute("RtID") == Some(runtime_id.clone()))
//     .unwrap();

//     get_xpath_from_rnode(&root_node, &target)

// }

pub fn get_xpath_full_from_runtime_id(runtime_id: &str, xml: &str, simple_path: bool) -> String {

    let doc = Document::parse(xml).unwrap();

    if let Some(node_id) = doc
        .descendants()
        .find(|n| n.attribute("RtID") == Some(runtime_id)) {
            get_xpath_robula(&doc, node_id, simple_path)
        } else {
            "UI Element not found - no xpath available".to_string()
        }


    

}