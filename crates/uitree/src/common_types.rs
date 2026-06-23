use crate::save_ui_element::SaveUIElement;
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
