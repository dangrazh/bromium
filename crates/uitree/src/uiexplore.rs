use crate::{printfmt, UITreeMap};

use std::sync::mpsc::Sender;
use uiautomation::core::UIAutomation;
use uiautomation::{UIElement, UITreeWalker};




#[derive(Debug, Clone)]
pub struct UIElementInTree {
    element_props: SaveUIElement,
    tree_index: usize,
}

impl UIElementInTree {
    pub fn new(element_props: SaveUIElement, tree_index: usize) -> Self {
        UIElementInTree {element_props, tree_index}
    }

    pub fn get_element_props(&self) -> &SaveUIElement {
        &self.element_props
    }

    pub fn get_tree_index(&self) -> usize {
        self.tree_index
    }
}

#[derive(Debug, Clone)]
pub struct UITree {
    tree: UITreeMap<SaveUIElement>,
    ui_elements: Vec<UIElementInTree>,
}

impl UITree {
    pub fn new(tree: UITreeMap<SaveUIElement>, ui_elements: Vec<UIElementInTree>) -> Self {
        UITree {tree, ui_elements} 
    }

    pub fn get_tree(&self) -> &UITreeMap<SaveUIElement> {
        &self.tree
    }

    pub fn get_elements(&self) -> &Vec<UIElementInTree> {
        &self.ui_elements
    }

    pub fn for_each<F>(&self, f: F)
    where
        F: FnMut(usize, &SaveUIElement),
    {
        self.tree.for_each(f);
    }

    pub fn root(&self) -> usize {
        self.tree.root()
    }

    pub fn children(&self, index: usize) -> &[usize] {
        self.tree.children(index)
    }

    pub fn node(&self, index: usize) -> (&str, &SaveUIElement) {
        let node = &self.tree.node(index);
        (&node.name, &node.data)

    }
    
    pub fn get_xpath_for_element(&self, index: usize) -> String {
        let path = self.get_xpath_raw_for_element(index);
        // path
        // println!("Raw XPath: {}", path);
        
        let xpath = match_original_format(&path);
        // println!("Formatted XPath: {}", xpath);
        xpath
    }

    fn get_xpath_raw_for_element(&self, index: usize) -> String {
    
        let mut path = Vec::new();
    
        let path_to_element = self.tree.get_path_to_element(index);
        if path_to_element.is_empty() {
            return "/".to_string(); // Return root if no path
        }

        for &node_index in path_to_element.iter() {
            let node = &self.tree.node(node_index);
            let ui_elem_props = &node.data;
            
            let mut control_type: String = "".to_string();
            if let Ok(ctrl_type) =  ui_elem_props.element.get_control_type() {
                control_type = ctrl_type.to_string();    
            }
            // let ctrl_type = ui_elem_props.element.get_control_type().unwrap();
            let control_type_localized = ui_elem_props.element.get_localized_control_type().unwrap_or("".to_string());
            
            let name = ui_elem_props.element.get_name().unwrap_or("".to_string());
            let class_name = ui_elem_props.element.get_classname().unwrap_or("".to_string());
            let automation_id = ui_elem_props.element.get_automation_id().unwrap_or("".to_string());

            let bounding_rect: uiautomation::types::Rect = ui_elem_props.element.get_bounding_rectangle().unwrap_or(uiautomation::types::Rect::new(0, 0, 0, 0));
            let left = bounding_rect.get_left();
            let top = bounding_rect.get_top();
            let right = bounding_rect.get_right();
            let bottom = bounding_rect.get_bottom();
            let width = right - left;
            let height = bottom - top;
            
            let runtime_id = ui_elem_props.element.get_runtime_id().unwrap().iter().map(|x| x.to_string()).collect::<Vec<String>>().join(".");
            path.push(format!("/{}[LocalizedControlType=\"{}\"][ClassName=\"{}\"][Name=\"{}\"][AutomationId=\"{}\"][x={}][y={}][width={}][height={}][lx={}][ly={}][position()={}][RuntimeId=\"{}\"]\n", control_type, control_type_localized, class_name, name, automation_id, left, top, width, height, left, top, "", runtime_id));
            // path.push(format!("/{}[LocalizedControlType=\"{}\"][ClassName=\"{}\"][Name=\"{}\"][AutomationId=\"{}\"][x={}][y={}][width={}][height={}][lx={}][ly={}][position()={}]\n", ctrl_type, ctrl_type_localized, class_name, name, automation_id, left, top, width, height, left, top, ""));
        }

        // path.reverse();
        format!("{}", path.join(""))

    }

}


// #[derive(Debug, Clone)]
// pub struct UIElementProps {
//     pub name: String,
//     pub classname: String,
//     pub control_type: String,
//     pub localized_control_type: String,
//     pub framework_id: String,
//     pub runtime_id: Vec<i32>,
//     pub automation_id: String,
//     pub handle: isize,
//     pub bounding_rect: uiautomation::types::Rect,
//     pub bounding_rect_size: i32,
//     pub level: usize,
//     pub z_order: usize,
// }

// impl UIElementProps {
//     pub fn new(from_element: UIElement, level: usize, z_order: usize) -> Self {
//         let mut elem = UIElementProps::from(from_element);
//         elem.z_order = z_order;
//         elem.level = level;
//         elem
//     }
// }

// impl From<UIElement> for UIElementProps {
//     fn from(item: UIElement) -> Self {

//         let name: String = item.get_name().unwrap_or("".to_string());
//         let classname: String = item.get_classname().unwrap_or("".to_string());
        
//         let mut control_type: String = "".to_string();
//         if let Ok(ctrl_type) =  item.get_control_type() {
//             control_type = ctrl_type.to_string();    
//         }

//         let localized_control_type: String = item.get_localized_control_type().unwrap_or("".to_string());
//         let framework_id: String = item.get_framework_id().unwrap_or("".to_string());
//         let runtime_id: Vec<i32> = item.get_runtime_id().unwrap_or(Vec::new());
//         let automation_id: String = item.get_automation_id().unwrap_or("".to_string());
//         let handle : isize = item.get_native_window_handle().unwrap_or(Handle::from(0 as isize)).into();
//         let bounding_rect: uiautomation::types::Rect = item.get_bounding_rectangle().unwrap_or(uiautomation::types::Rect::new(0, 0, 0, 0));
//         let bounding_rect_size: i32 = (bounding_rect.get_right() - bounding_rect.get_left()) * (bounding_rect.get_bottom() - bounding_rect.get_top());            
        
//         UIElementProps {
//             name,
//             classname,
//             control_type,
//             localized_control_type,
//             framework_id,
//             runtime_id,
//             automation_id,
//             handle,
//             bounding_rect,
//             bounding_rect_size,
//             level: 0,
//             z_order: 0,
//         }
//     }
// }

#[derive(Debug, Clone)]
pub struct SaveUIElement {
    pub element: UIElement,
    pub bounding_rect_size: i32,
    pub level: usize,
    pub z_order: usize,

}

unsafe impl Send for SaveUIElement {}
unsafe impl Sync for SaveUIElement {}

impl SaveUIElement {
    pub fn new(element: UIElement, level: usize, z_order: usize) -> Self {
        let bounding_rect: uiautomation::types::Rect = element.get_bounding_rectangle().unwrap_or(uiautomation::types::Rect::new(0, 0, 0, 0));
        let bounding_rect_size: i32 = (bounding_rect.get_right() - bounding_rect.get_left()) * (bounding_rect.get_bottom() - bounding_rect.get_top());            
        SaveUIElement { element, bounding_rect_size, level, z_order }
    }

    pub fn get_element(&self) -> &UIElement {
        &self.element
    }
}


pub fn get_all_elements(tx: Sender<UITree>, max_depth: Option<usize>)  {   
    
    let automation = UIAutomation::new().unwrap();
    // control view walker
    let walker = automation.get_control_view_walker().unwrap();

    // allocate a new ui elements vector with a capacity of 10000 elements
    let mut ui_elements: Vec<UIElementInTree> = Vec::with_capacity(10000);

    // let mut xml_writer = XMLDomWriter::new();

    // get the desktop and all UI elements below the desktop
    let root = automation.get_root_element().unwrap();
    let runtime_id = root.get_runtime_id().unwrap_or(vec![0, 0, 0, 0]).iter().map(|x| x.to_string()).collect::<Vec<String>>().join("-");
    let item = format!("'{}' {} ({} | {} | {})", root.get_name().unwrap(), root.get_localized_control_type().unwrap(), root.get_classname().unwrap(), root.get_framework_id().unwrap(), runtime_id);
    let ui_elem_props = SaveUIElement::new(root.clone(), 0, 999);
    let mut tree = UITreeMap::new(item, runtime_id.clone(), ui_elem_props.clone());
    let ui_elem_in_tree = UIElementInTree::new(ui_elem_props, 0);    
    // let mut ui_elements: Vec<UIElementInTree> = vec![ui_elem_in_tree];
    ui_elements.push(ui_elem_in_tree);

    if let Ok(_first_child) = walker.get_first_child(&root) {     
        // itarate over all child ui elements
        get_element(&mut tree, &mut ui_elements,  0, &walker, &root,  0, 0, max_depth); //xml_root,
    }

    // sorting the elements by z_order and then by ascending size of the bounding rectangle
    printfmt!("Sorting UI elements by size and z-order...");
    ui_elements.sort_by(|a, b| a.get_element_props().bounding_rect_size.cmp(&b.get_element_props().bounding_rect_size));
    ui_elements.sort_by(|a, b| a.get_element_props().z_order.cmp(&b.get_element_props().z_order));

    // pack the tree and ui_elements vector into a single struct
    let ui_tree = UITree::new(tree, ui_elements);

    // send the tree containing all UI elements back to the main thread
    printfmt!("Sending UI tree with {} elements to the main thread...", ui_tree.get_elements().len());
    tx.send(ui_tree).unwrap();

}


fn get_element(mut tree: &mut UITreeMap<SaveUIElement>, mut ui_elements: &mut Vec<UIElementInTree>, parent: usize, walker: &UITreeWalker, element: &UIElement, level: usize, mut z_order: usize, max_depth: Option<usize>)  { //xml_dom_node: &mut XMLDomNode,
    if let Some(limit) = max_depth {
        if level > limit {
            return;
        }    
    }

    let runtime_id = element.get_runtime_id().unwrap_or(vec![0, 0, 0, 0]).iter().map(|x| x.to_string()).collect::<Vec<String>>().join("-");
    // let runtime_id = "runtime_id".to_string(); // placeholder for runtime_id, as it is not used in the current implementation
    let item = format!("'{}' {} ({} | {} | {})", element.get_name().unwrap(), element.get_localized_control_type().unwrap(), element.get_classname().unwrap(), element.get_framework_id().unwrap(), runtime_id);
    let ui_elem_props: SaveUIElement;

    if level == 0 {
        // manually setting the z_order for the root element
        ui_elem_props = SaveUIElement::new(element.clone(), level, 999);
    } else {
        ui_elem_props = SaveUIElement::new(element.clone(), level, z_order);
    }
    
    let parent = tree.add_child(parent, item.as_str(), runtime_id.as_str(), ui_elem_props.clone());
    let ui_elem_in_tree = UIElementInTree::new(ui_elem_props, parent);
    ui_elements.push(ui_elem_in_tree);
    // pass through the XML DOM node to avoid computation of the XML DOM tree during performance testing
    // let curr_xml_dom_node = xml_dom_node;
    // let curr_xml_dom_node = xml_dom_node.add_child(XMLDomNode::new(element.get_classname().unwrap().as_str()));
    // curr_xml_dom_node.set_attribute("Name", element.get_name().unwrap_or("No name defined".to_string()).as_str());

    // Walking the children of the current element
    if let Ok(child) = walker.get_first_child(&element) {
        // getting child elements
        // printfmt!("Found child element: {}", child.get_name().unwrap_or("Unknown".to_string()));
        get_element(&mut tree, &mut ui_elements, parent, walker, &child, level + 1, z_order, max_depth); // curr_xml_dom_node,
        let mut next = child;
        // walking siblings
        while let Ok(sibling) = walker.get_next_sibling(&next) {
            // incrementing z_order for each sibling
            if level + 1 == 1 {
                z_order += 1;
            }
            // printfmt!("Found sibling element: {}", sibling.get_name().unwrap_or("Unknown".to_string()));
            get_element(&mut tree, &mut ui_elements, parent, walker, &sibling,   level + 1, z_order, max_depth); // curr_xml_dom_node,
            next = sibling;
        }
    }    
    
}

// Function that tries to match the original C++ XPath format
fn match_original_format(xpath: &str) -> String {
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
        // let tag = &line[1..tag_end]; // 
        let tag_extracted = line.chars().next().map(|c| &line[c.len_utf8()..tag_end]);
        match tag_extracted {
            None => {
                printfmt!("Skipping malformed line: {}", line);
                continue; // Skip lines that don't have a valid tag
            },
            Some(tag_content) => {
                // leak the tag content outside the match scope to avoid lifetime issues
                tag = tag_content;
            },
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
            if let Some(class_value) = get_attr_value("ClassName", line) {
                if class_value == "LandmarkTarget" {
                    if let Some(class_attr) = extract_attr("ClassName", line) {
                        element.push_str(&class_attr);
                    }
                }
                // Skip ClassName for Group elements with other classes like "NamedContainerAutomationPeer"
            }
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
    
    // Reverse the elements to go from root to specific element
    // elements.reverse();
    
    // Join all elements into a single XPath string
    elements.join("")
}