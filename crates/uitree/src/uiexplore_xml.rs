use crate::conversion::ConvertFromControlType;

use crate::save_ui_element::{SaveUIElement, get_ui_automation_instance};

// use crate::commons::FileWriter;
use crate::{printfmt, UITreeMap};
use xmlutil::{XpathQueryResult}; //Xot
use xmlutil::xml::{XMLDomWriter, XMLDomNode};
use xmlutil::xpath_gen::get_xpath_full_from_runtime_id; //get_xpath_from_runtime_id, 
use xmlutil::xpath_eval::eval_xpath;

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::mpsc::{channel, Sender, Receiver};

use uiautomation::core::UIAutomation;
use uiautomation::{UIElement, UITreeWalker};


#[derive(Debug, Clone)]
pub struct UIElementInTree {
    runtime_id: Vec<i32>,
    element_props: SaveUIElement,
    tree_index: usize,
}

impl UIElementInTree {
    pub fn new(element_props: SaveUIElement, tree_index: usize) -> Self {
        let rt_id = element_props.get_runtime_id().clone();
        UIElementInTree {runtime_id: rt_id, element_props, tree_index}
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

#[derive(Debug, Clone)]
pub struct UITree {
    tree: UITreeMap<SaveUIElement>,
    xml_dom_tree: String,
    ui_elements: Vec<UIElementInTree>,
}

impl UITree {
    pub fn new(tree: UITreeMap<SaveUIElement>, xml_dom_tree: String, ui_elements: Vec<UIElementInTree>) -> Self {
        UITree {tree, xml_dom_tree, ui_elements} 
    }

    pub fn get_tree(&self) -> &UITreeMap<SaveUIElement> {
        &self.tree
    }

    pub fn get_tree_mut(&mut self) -> &mut UITreeMap<SaveUIElement> {
        &mut self.tree
    }

    pub fn get_xml_dom_tree(&self) -> &str {
        &self.xml_dom_tree
    }

    pub fn get_elements(&self) -> &Vec<UIElementInTree> {
        &self.ui_elements
    }

    pub fn get_elements_mut(&mut self) -> &mut Vec<UIElementInTree> {
        &mut self.ui_elements
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
    
    pub fn get_xpath_for_element(&self, index: usize, simple_path: bool) -> String {

        let node = &self.tree.node(index);
        let save_ui_elem = &node.data;
        // let rt_id = save_ui_elem.get_element().get_runtime_id().unwrap_or(vec![0, 0, 0, 0]).iter().map(|x| x.to_string()).collect::<Vec<String>>().join("-");
        let rt_id = save_ui_elem.get_element().get_runtime_id().iter().map(|x| x.to_string()).collect::<Vec<String>>().join("-");
        get_xpath_full_from_runtime_id(rt_id.as_str(), self.get_xml_dom_tree(), simple_path)

    }

    pub fn get_element_by_xpath(&self, xpath: &str) -> Option<&SaveUIElement> {

        // Patch the xpath with /@RtID if it is missing
        let xpath = if !xpath.ends_with("/@RtID") {xpath.to_string() + "/@RtID"} else {xpath.to_string()};

        let xpath_result = eval_xpath(xpath, self.get_xml_dom_tree().to_string());
        
        match xpath_result.get_result_count() {
            0 => return None,
            1 => {
                let items = xpath_result.get_result_items();
                let default_result = &XpathQueryResult::default();
                let itm = items.get(0).unwrap_or(default_result);
                let runtime_id = itm.get_item_value();
                let ui_elem = self.get_tree().get_element_by_runtime_id(runtime_id).unwrap();
                let ui_elem = ui_elem.data.get_element();
                return Some(ui_elem);
            },
            _ => {
                printfmt!("Warning: XPath expression returned {} results, expected only 1 result. Returning the first result.", xpath_result.get_result_count());
                return None;
            }
        }


    }

}

impl UITree {

    // pub fn refresh_tree(&mut self) {
    //     todo!("Implement tree refreshing");
    // }

    pub fn append_or_replace_subtree(&mut self, parent_index: usize, mut subtree: UITree) -> Result<usize, String> {
        // Append the subtree to the current tree at the specified parent index
        // Return the index of the new subtree root in the current tree
        // printfmt!("Parent index to append subtree: {}", parent_index);
        // printfmt!("Appending or replacing subtree with root: {}", subtree.get_tree().node(subtree.root()).name);
        let subtree_root = subtree.root();
        let subtree_node = subtree.get_tree().node(subtree_root);
        let subtree_save_ui_elem = &subtree_node.data;
        let subtree_runtime_id = subtree_save_ui_elem.get_element().get_runtime_id().iter().map(|x| x.to_string()).collect::<Vec<String>>().join("-");

        // Check if the parent index exists in the current tree
        if !self.get_tree().has_node(parent_index) {
            return Err("Parent index does not exist in the current tree".to_string());
        }

        // Check if the subtree root already exists in the current tree
        if self.get_tree().get_element_by_runtime_id(&subtree_runtime_id).is_some() {
            // Find the existing node index and remove it along with its children
            let existing_node = self.get_tree().get_element_by_runtime_id(&subtree_runtime_id).unwrap();
            let existing_node_index = existing_node.index;
            // printfmt!("Subtree root already exists in the current tree at index {}. Replacing existing subtree.", existing_node_index);
            // Remove the existing node and its children
            self.get_tree_mut().remove_node(existing_node_index)?;
        }

        // Add the root of the subtree to the current tree
        let tree_mut = self.get_tree_mut();
        let new_index = tree_mut.add_child(parent_index, &subtree_node.name, &subtree_runtime_id, subtree_save_ui_elem.clone());
        // printfmt!("Added subtree root to current tree at index {}", new_index);

        // replace the ui_elements vector with the new elements from the subtree
        remove_in_place(self.get_elements_mut(), subtree.get_elements_mut());

        // add (move) all elements from the subtree to the current tree's ui_elements vector
        self.get_elements_mut().append(subtree.get_elements_mut());

        // sorting the elements by z_order and then by ascending size of the bounding rectangle
        printfmt!("Sorting UI elements by size and z-order...");
        self.get_elements_mut().sort_by(|a, b| a.get_element_props().get_bounding_rect_size().cmp(&b.get_element_props().get_bounding_rect_size()));
        self.get_elements_mut().sort_by(|a, b| a.get_element_props().get_z_order().cmp(&b.get_element_props().get_z_order()));

        // Recursively add all children of the subtree root to the treemap
        self.append_children(new_index, &mut subtree, subtree_root)?;


        // Merging the xml_dom_tree strings into a single xml_dom_tree string
        // using xot create to merge the xml trees
        // 1. get the xml_dom_tree of the current tree
        let current_xml_dom_tree = self.get_xml_dom_tree();
        // 2. get the xml_dom_tree of the subtree
        let subtree_xml_dom_tree = subtree.get_xml_dom_tree();
        printfmt!("Merging XML DOM trees... adding new subtree: {}", subtree_xml_dom_tree);
        let new_xml_dom_tree = append_or_replace_node_by_rt_id(current_xml_dom_tree, subtree_xml_dom_tree, &subtree_runtime_id);
        self.xml_dom_tree = new_xml_dom_tree;


        Ok(new_index)
    }

    fn append_children(&mut self, parent_index: usize, mut subtree: &mut UITree, subtree_index: usize) -> Result<(), String> {
        let children = subtree.get_tree().children(subtree_index).to_vec();
        // printfmt!("Appending {} children to parent index {}", children.len(), parent_index);
        for child_index in children {
            let child_node = subtree.get_tree().node(child_index);
            let child_save_ui_elem = &child_node.data;
            let child_runtime_id = child_save_ui_elem.get_element().get_runtime_id().iter().map(|x| x.to_string()).collect::<Vec<String>>().join("-");

            // Add the child to the current tree
            let new_child_index = self.get_tree_mut().add_child(parent_index, &child_node.name, &child_runtime_id, child_save_ui_elem.clone());

            // Recursively add the child's children
            self.append_children(new_child_index, &mut subtree, child_index)?;
        }
        Ok(())
    }

}

fn remove_in_place(orig: &mut Vec<UIElementInTree>, check: &Vec<UIElementInTree>) {
    let set: HashSet<_> = check.iter().cloned().collect();
    orig.retain(|x| !set.contains(x));
}

fn append_or_replace_node_by_rt_id(current_xml_dom_tree: &str, xml_dom_subtree: &str, target_node_rt_id: &str) -> String {
    
        // 3. parse both xml_dom_trees into xot documents
        // 4. find the subtree's parent node in the current tree's xot document using the parent_index
        // 5. if subtree's parent node exists, use the xot .replace() method to replace it with the subtree's root node
        // 6. if subtree's parent node does not exist, use the xot .append() method to append the subtree's root node to the current tree's root node
        // 7. serialize the modified current tree's xot document back into a string and update the current tree's xml_dom_tree

    
    let target = target_node_rt_id.to_string();

    let mut xot = xot::Xot::new();
    let root = xot.parse(current_xml_dom_tree).unwrap();
    let doc = xot.document_element(root).unwrap();
 
    if let Some(existing_node) = find_node_by_rt_id(&mut xot, doc, &target) {
        let new_subtree = xot.parse(xml_dom_subtree).unwrap();
        let new_subtree_doc = xot.document_element(new_subtree).unwrap();
        xot.replace(existing_node, new_subtree_doc).unwrap();

        return xot.serialize_xml_string(Default::default(), root).unwrap();
    } else {
        let new_node = xot.parse(xml_dom_subtree).unwrap();
        let new_node_doc = xot.document_element(new_node).unwrap();
        xot.append(doc, new_node_doc).unwrap();

        return xot.serialize_xml_string(Default::default(), root).unwrap();
    }

}


fn find_node_by_rt_id(xot: &mut xot::Xot, doc: xot::Node, target: &String) -> Option<xot::Node> {
    
    let rt_id_a = xot.add_name("RtID");
    let descendants = xot.descendants(doc);
    let rt_id_default = "n/a".to_string();
    for desc in descendants {
        let desc_attrs = xot.attributes(desc);
        let rt_id = desc_attrs.get(rt_id_a).unwrap_or(&rt_id_default);
        if rt_id == target {
            return Some(desc);
        }
    }
    None
}

pub fn get_all_elements_xml(tx: Sender<UITree>, root_element: Option<SaveUIElement>, max_depth: Option<usize>, calling_window_caption: Option<String>) {   
    
    let automation = get_ui_automation_instance().unwrap();
    // control view walker
    let walker = automation.get_control_view_walker().unwrap();

    // allocate a new ui elements vector with a capacity of 10000 elements
    let mut ui_elements: Vec<UIElementInTree> = Vec::with_capacity(10000);

    let mut xml_writer = XMLDomWriter::new();

    // get the root ui element passed in or the desktop and all UI elements below the desktop
    let root = if let Some(elem) = root_element {elem.get_ui_automation_ui_element().unwrap()} else {automation.get_root_element().unwrap()};
    // let root = automation.get_root_element().unwrap();
    
    // TODO: get z-order of root element
    // let z_order = if let Some(elem) = root_element {elem.get_z_order()} else {999};
    // TODO: get level of root element
    // let level = if let Some(elem) = root_element {elem.get_level()} else {0};

    let runtime_id = root.get_runtime_id().unwrap_or(vec![0, 0, 0, 0]).iter().map(|x| x.to_string()).collect::<Vec<String>>().join("-");
    let item = format!("'{}' {} ({} | {} | {})", root.get_name().unwrap(), root.get_localized_control_type().unwrap(), root.get_classname().unwrap(), root.get_framework_id().unwrap(), runtime_id);
    let ui_elem_props = SaveUIElement::new(root.clone(), 0, 999);
    let mut tree = UITreeMap::new(item, runtime_id.clone(), ui_elem_props.clone());
    let ui_elem_in_tree = UIElementInTree::new(ui_elem_props, 0);    
    // let mut ui_elements: Vec<UIElementInTree> = vec![ui_elem_in_tree];
    ui_elements.push(ui_elem_in_tree);
    xml_writer.set_root(XMLDomNode::new(root.get_control_type().unwrap().as_str()));
    let xml_root = xml_writer.get_root_mut().unwrap();
    xml_root.set_attribute("RtID", runtime_id.as_str());
    xml_root.set_attribute("z-order", 999.to_string().as_str());
    xml_root.set_attribute("Name", root.get_name().unwrap_or("No name defined".to_string()).as_str());
    let control_type = root.get_control_type();
    match control_type {
        Ok(ct) => {
            xml_root.set_attribute("ControlType", ct.as_str());
        },
        Err(_) => {
            xml_root.set_attribute("ControlType", "No control type defined");
        }
    }

    if let Ok(_first_child) = walker.get_first_child(&root) {     
        // itarate over all child ui elements
        get_element(&mut tree, &mut ui_elements,  0, &walker, &root, xml_root, 0, 0, max_depth, calling_window_caption);
    }

    // creating the XML DOM tree
    let xml_dom_tree = xml_writer.to_string().unwrap();

    // sorting the elements by z_order and then by ascending size of the bounding rectangle
    printfmt!("Sorting UI elements by size and z-order...");
    ui_elements.sort_by(|a, b| a.get_element_props().get_bounding_rect_size().cmp(&b.get_element_props().get_bounding_rect_size()));
    ui_elements.sort_by(|a, b| a.get_element_props().get_z_order().cmp(&b.get_element_props().get_z_order()));

    // DEBUG ONLY
    // let mut fw = FileWriter::new("uiexplorer_xml");
    // fw.write(&xml_dom_tree.as_str());

    // pack the tree and ui_elements vector into a single struct
    let ui_tree = UITree::new(tree, xml_dom_tree, ui_elements);

    // send the tree containing all UI elements back to the main thread
    printfmt!("Sending UI tree with {} elements to the main thread...", ui_tree.get_elements().len());
    match tx.send(ui_tree) {
        Ok(_) => {printfmt!("UI tree sent successfully.");}
        Err(e) => {printfmt!("Error sending UI tree: {:?}", e);}
    };

}



pub fn get_all_elements_par_xml(tx: Sender<UITree>, max_depth: Option<usize>, calling_window_caption: Option<String>) {   
    
    let automation = UIAutomation::new().unwrap();
    // control view walker
    let walker = automation.get_control_view_walker().unwrap();

    // allocate a new ui elements vector with a capacity of 10000 elements
    let mut ui_elements: Vec<UIElementInTree> = Vec::with_capacity(10000);

    let mut xml_writer = XMLDomWriter::new();

    // get the desktop and all UI elements below the desktop
    let root = automation.get_root_element().unwrap();
    let runtime_id = root.get_runtime_id().unwrap_or(vec![0, 0, 0, 0]).iter().map(|x| x.to_string()).collect::<Vec<String>>().join("-");
    let item = format!("'{}' {} ({} | {} | {})", root.get_name().unwrap(), root.get_localized_control_type().unwrap(), root.get_classname().unwrap(), root.get_framework_id().unwrap(), runtime_id);
    let ui_elem_props = SaveUIElement::new(root.clone(), 0, 999);
    let mut tree = UITreeMap::new(item, runtime_id.clone(), ui_elem_props.clone());
    let ui_elem_in_tree = UIElementInTree::new(ui_elem_props, 0);    
    // let mut ui_elements: Vec<UIElementInTree> = vec![ui_elem_in_tree];
    ui_elements.push(ui_elem_in_tree);
    xml_writer.set_root(XMLDomNode::new(root.get_control_type().unwrap().as_str()));
    let xml_root = xml_writer.get_root_mut().unwrap();
    xml_root.set_attribute("RtID", runtime_id.as_str());
    xml_root.set_attribute("Name", root.get_name().unwrap_or("No name defined".to_string()).as_str());
    let control_type = root.get_control_type();
    match control_type {
        Ok(ct) => {
            xml_root.set_attribute("ControlType", ct.as_str());
        },
        Err(_) => {
            xml_root.set_attribute("ControlType", "No control type defined");
        }
    }   

    if let Ok(_first_child) = walker.get_first_child(&root) {     
        // itarate over all child ui elements
        let calling_window_caption_1 = calling_window_caption.clone();
        // FIXME: the z-order ist not correct in the parallel version, needs analysis and fixing
        get_element(&mut tree, &mut ui_elements,  0, &walker, &root, xml_root, 0, 0, Some(1 as usize), calling_window_caption_1);
        // get_element(&mut tree, &mut ui_elements,  0, &walker, &_first_child, xml_root, 0, 0, Some(2 as usize), calling_window_caption_1);
    }

    // creating the XML DOM tree
    let xml_dom_tree = xml_writer.to_string().unwrap();

    // sorting the elements by z_order and then by ascending size of the bounding rectangle
    printfmt!("Sorting UI elements by size and z-order...");
    ui_elements.sort_by(|a, b| a.get_element_props().get_bounding_rect_size().cmp(&b.get_element_props().get_bounding_rect_size()));
    ui_elements.sort_by(|a, b| a.get_element_props().get_z_order().cmp(&b.get_element_props().get_z_order()));

    // DEBUG ONLY
    // let mut fw = FileWriter::new("uiexplorer_xml");
    // fw.write(&xml_dom_tree.as_str());

    // pack the tree and ui_elements vector into a single struct
    let mut ui_tree = UITree::new(tree, xml_dom_tree, ui_elements);
    // printfmt!("This is the top level tree we are processing:\n{}", ui_tree.get_xml_dom_tree());


    // special handling to skip duplicate root node in processing
    // take the frist child of the root instead of the root itself
    let root_idx = ui_tree.get_tree().root();
    let root_first_child  = ui_tree.get_tree().children(root_idx).into_iter().take(1).next();
    let mut root_first_child_idx: usize = 0;
    match root_first_child {
        None => {
            printfmt!("No child elements found under the root element. Sending empty UI tree.");
            match tx.send(ui_tree.clone()) {
                Ok(_) => {printfmt!("UI tree sent successfully.");}
                Err(e) => {printfmt!("Error sending UI tree: {:?}", e);}
            };
        },
        Some(val) => {
            root_first_child_idx = *val;
        }
    }
    // let root_first_child_idx = ui_tree.get_tree().children(root_idx).into_iter().take(1).next().unwrap_or_else(|| {
    //     todo!("Implement error handling for case where no child elements found under the root element.");
        
    // }); 

    let child_indices = ui_tree.get_tree().children(root_first_child_idx);
    let mut child_elements = Vec::new();
    // printfmt!("children to process in parallel: {}", child_indices.len());
    for &child_index in child_indices {
        let child_node = ui_tree.get_tree().node(child_index);
        let child_save_ui_elem = &child_node.data;
        child_elements.push(child_save_ui_elem.get_element().clone());
    }

    // Process child_elements in parallel to build the full tree
    let (tx_par, rx_par): (Sender<_>, Receiver<UITree>) = channel();
    let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();
    for element in child_elements.clone() {
        // Spawn a new thread for each element to process it in parallel
        let tx_par_clone = tx_par.clone();
         let calling_window_caption_n = calling_window_caption.clone();
        //  printfmt!("Spawning thread to process element: '{}'", element.get_name());
        let handle = std::thread::spawn(move || {
            // get_all_elements_par_xml(tx, None, None);
            get_all_elements_xml(tx_par_clone, Some(element), max_depth, calling_window_caption_n);
        });
        handles.push(handle);
    }

    // get the subtrees from the threads
    // printfmt!("Collecting subtrees from threads...");
    let mut subtrees = Vec::new();
    for _i in child_elements {
        let subtree: UITree = rx_par.recv().unwrap();
        subtrees.push(subtree);
    }

    // ensure all threads have completed
    // printfmt!("Waiting for all threads to complete...");
    for handle in handles {
        handle.join().unwrap();
    }

    // append the subtrees to the main tree
    // printfmt!("Appending {} subtrees to the main tree...", subtrees.len());
    for subtree in subtrees {
        // !("This is the tree we are appending:\n{}", subtree.get_xml_dom_tree());
        match ui_tree.append_or_replace_subtree(ui_tree.get_tree().root(), subtree) {
            Ok(_) => {},
            Err(e) => {printfmt!("Error appending subtree: {}", e);}
        }
        printfmt!("UI tree has now {} elements", ui_tree.get_elements().len());
    }
    
    // TODO: merging the xml_dom_tree strings into a single xml_dom_tree string
    // using xot create to merge the xml trees


    // send the tree containing all UI elements back to the main thread
    printfmt!("Sending UI tree with {} elements to the main thread...", ui_tree.get_elements().len());
    match tx.send(ui_tree) {
        Ok(_) => {printfmt!("UI tree sent successfully.");}
        Err(e) => {printfmt!("Error sending UI tree: {:?}", e);}
    };

}


fn get_element(mut tree: &mut UITreeMap<SaveUIElement>, mut ui_elements: &mut Vec<UIElementInTree>, parent: usize, walker: &UITreeWalker, element: &UIElement, xml_dom_node: &mut XMLDomNode, level: usize, mut z_order: usize, max_depth: Option<usize>, calling_window_caption: Option<String>)  {
    if let Some(limit) = max_depth {
        if level > limit {
            return;
        }    
    }

    if let Some(caption) = &calling_window_caption {
        if let Ok(name) = element.get_name() {
            if name == *caption {
                // printfmt!("Skipping element with caption: {}", caption);
                return;
            }
        }
    }

    let runtime_id = element.get_runtime_id().unwrap_or(vec![0, 0, 0, 0]).iter().map(|x| x.to_string()).collect::<Vec<String>>().join("-");
    let item = format!("'{}' {} ({} | {} | {})", element.get_name().unwrap_or_default(), element.get_localized_control_type().unwrap_or_default(), element.get_classname().unwrap_or_default(), element.get_framework_id().unwrap_or_default(), runtime_id);
    let ui_elem_props: SaveUIElement;

    if level == 0 {
        // manually setting the z_order for the root element
        ui_elem_props = SaveUIElement::new(element.clone(), level, 999);
    } else {
        ui_elem_props = SaveUIElement::new(element.clone(), level, z_order);
    }
    
    let parent = tree.add_child(parent, item.as_str(), &runtime_id.as_str(), ui_elem_props.clone());
    let ui_elem_in_tree = UIElementInTree::new(ui_elem_props, parent);
    ui_elements.push(ui_elem_in_tree);
        
    let curr_xml_dom_node = xml_dom_node.add_child(XMLDomNode::new(element.get_control_type().unwrap().as_str()));
    curr_xml_dom_node.set_attribute("RtID", runtime_id.as_str());
    if level == 0 {
        // manually setting the z_order for the root element
            curr_xml_dom_node.set_attribute("z-order", "999");
    } else {
            curr_xml_dom_node.set_attribute("z-order", z_order.to_string().as_str());
    }
    curr_xml_dom_node.set_attribute("Name", element.get_name().unwrap_or("No name defined".to_string()).as_str());
    let control_type = element.get_control_type();
    match control_type {
        Ok(ct) => {
            curr_xml_dom_node.set_attribute("ControlType", ct.as_str());
        },
        Err(_) => {
            curr_xml_dom_node.set_attribute("ControlType", "No control type defined");
        }
    }
    

    // Walking the children of the current element
    if let Ok(child) = walker.get_first_child(&element) {
        // getting child elements
        // printfmt!("Found child element: {}", child.get_name().unwrap_or("Unknown".to_string()));
        get_element(&mut tree, &mut ui_elements, parent, walker, &child, curr_xml_dom_node, level + 1, z_order, max_depth, calling_window_caption.clone());
        let mut next = child;
        // walking siblings
        while let Ok(sibling) = walker.get_next_sibling(&next) {
            // incrementing z_order for each sibling
            if level + 1 == 1 {
                z_order += 1;
            }
            // printfmt!("Found sibling element: {}", sibling.get_name().unwrap_or("Unknown".to_string()));
            get_element(&mut tree, &mut ui_elements, parent, walker, &sibling, curr_xml_dom_node,  level + 1, z_order, max_depth, calling_window_caption.clone());
            next = sibling;
        }
    }    
    
}



