use crate::save_ui_element::SaveUIElement;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone)]
pub struct UIElementInTree {
    element_props: SaveUIElement,
    tree_index: usize,
}

impl UIElementInTree {
    pub fn new(element_props: SaveUIElement, tree_index: usize) -> Self {
        UIElementInTree {
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
        self.element_props.identity() == other.element_props.identity()
    }
}

impl Eq for UIElementInTree {}

impl Hash for UIElementInTree {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.element_props.identity().hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collection_identity_preserves_different_handle_aliases() {
        let a = UIElementInTree::new(
            SaveUIElement::fixture(3, "PopupHost", "Pane").with_handle(111),
            1,
        );
        let b = UIElementInTree::new(SaveUIElement::fixture(3, "", "Pane").with_handle(222), 2);
        let elements: std::collections::HashSet<_> = [a.clone(), b, a].into_iter().collect();
        assert_eq!(elements.len(), 2);
    }
}
