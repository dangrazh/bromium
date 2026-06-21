use xot::{Node, Xot};

pub struct XMLDomManager {
    dom_manager: Xot,
    document: Option<Node>,
}

impl Default for XMLDomManager {
    fn default() -> Self {
        Self::new()
    }
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
            Err(xot::Error::InvalidOperation(
                "Root node is not set".to_string(),
            ))
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
