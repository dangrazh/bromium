#![allow(dead_code)]
use xot::{Xot, Node};

pub struct XMLDomManager {
    dom_manager: Xot,
    document: Option<Node>,
}

impl XMLDomManager {
    pub fn new() -> Self {
        XMLDomManager {
            dom_manager: Xot::new(),
            document: None,
        }
    }

    pub fn get_dom_manager(&self) -> &Xot {
        &self.dom_manager
    }

    pub fn get_document(&self) -> Option<&Node> {
        self.document.as_ref()
    }

    pub fn set_root_node(&mut self, xml: &str) -> Result<(), xot::Error> {
        let doc = self.dom_manager.parse(xml)?;
        let root = self.dom_manager.document_element(doc)?;
        self.remove_all_children(&root)?;
        self.document = Some(root);
        Ok(())
    }

    pub fn add_sub_tree(&mut self, xml: &str) -> Result<(), xot::Error> {
        if let Some(root_node) = &self.document {
            let new_doc = self.dom_manager.parse(xml)?;
            let new_sub_tree = self.dom_manager.document_element(new_doc)?;
            self.dom_manager.append(*root_node, new_sub_tree)?;
            Ok(())
        } else {
            Err(xot::Error::InvalidOperation("Root node is not set".to_string()))
        }
    }
    
    pub fn add_child_node(&mut self, parent: &Node, child: &Node) -> Result<(), xot::Error> {
        self.dom_manager.append(*parent, *child)?;
        Ok(())
    }

    pub fn remove_all_children(&mut self, parent: &Node) -> Result<(), xot::Error> {
        while let Some(child) = self.dom_manager.first_child(*parent) {
            self.dom_manager.remove(child)?;
        }
        Ok(())
    }
}

/* 
fn add_child_node(parent: &Node, child: &Node) -> Result<(), xot::Error> {

    let mut xot = Xot::new();
    let root_doc = xot.parse(xml).unwrap();
    let root_node = xot.document_element(root_doc).unwrap();
    
    let new_doc = xot.parse(xml2).unwrap();
    let new_sub_tree = xot.document_element(new_doc).unwrap();
    xot.append(root_node, new_sub_tree).unwrap();

    let new_doc2 = xot.parse(xml3).unwrap();
    let new_sub_tree2 = xot.document_element(new_doc2).unwrap();
    xot.append(root_node, new_sub_tree2).unwrap();

    parent.append_child(child)
}
*/