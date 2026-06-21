//! A generic tree structure with fast key-value lookup.

use crate::UIHashMap;

#[derive(Debug, thiserror::Error)]
pub enum TreeMapError {
    #[error("Cannot remove root or invalid index {0} (tree has {1} nodes)")]
    InvalidIndex(usize, usize),
}

// A generic node in a UITreeMap
#[derive(Debug, Clone, Default)]
pub struct UITreeNode<T> {
    pub name: String,
    pub runtime_id: String,
    pub index: usize,
    pub parent: usize,
    pub children: Vec<usize>,
    pub data: T,
}

impl<T: Default> UITreeNode<T> {
    pub fn new(data: T) -> Self {
        Self {
            name: String::new(),
            runtime_id: String::new(),
            index: 0,
            parent: 0,
            children: Vec::new(),
            data,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UITreeMap<T> {
    nodes: Vec<UITreeNode<T>>,
    name_to_index: UIHashMap<String, Vec<usize>>, // Name-to-indices map (names are not unique)
    rtid_to_index: UIHashMap<String, usize>,
}

impl<T> UITreeMap<T> {
    pub fn new(root_name: String, rt_id: String, root_data: T) -> Self {
        let root = UITreeNode {
            name: root_name.clone(),
            runtime_id: rt_id.clone(),
            index: 0,
            parent: 0,
            children: Vec::new(),
            data: root_data,
        };

        let mut name_to_index: UIHashMap<String, Vec<usize>> = UIHashMap::default();
        let mut rtid_to_index = UIHashMap::default();

        name_to_index.entry(root_name).or_default().push(0);
        rtid_to_index.insert(rt_id, 0);

        Self {
            nodes: vec![root],
            name_to_index,
            rtid_to_index,
        }
    }

    pub fn root(&self) -> usize {
        0 // Root is always index 0
    }

    pub fn children(&self, index: usize) -> &[usize] {
        &self.nodes[index].children
    }

    pub fn node(&self, index: usize) -> &UITreeNode<T> {
        &self.nodes[index]
    }

    pub fn node_mut(&mut self, index: usize) -> &mut UITreeNode<T> {
        &mut self.nodes[index]
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn has_node(&self, index: usize) -> bool {
        index < self.nodes.len()
    }

    pub fn nodes(&self) -> &[UITreeNode<T>] {
        &self.nodes
    }

    pub fn add_child(&mut self, parent: usize, name: &str, rt_id: &str, data: T) -> usize {
        let index = self.nodes.len();
        let node = UITreeNode {
            name: name.to_string(),
            runtime_id: rt_id.to_string(),
            index,
            parent,
            children: Vec::with_capacity(15),
            data,
        };

        self.name_to_index
            .entry(name.to_string())
            .or_default()
            .push(index);
        self.rtid_to_index.insert(rt_id.to_string(), index);
        self.nodes[parent].children.push(index);
        self.nodes.push(node);
        index
    }

    pub fn remove_node(&mut self, index: usize) -> Result<(), TreeMapError>
    where
        T: Default,
    {
        if index == 0 || index >= self.nodes.len() {
            log::warn!(
                "Attempting to remove index: {} on TreeMap with {} nodes",
                index,
                self.nodes.len()
            );
            return Err(TreeMapError::InvalidIndex(index, self.nodes.len()));
        }

        // Remove from hash maps
        let name = &self.nodes[index].name;
        if let Some(indices) = self.name_to_index.get_mut(name) {
            indices.retain(|&i| i != index);
            if indices.is_empty() {
                self.name_to_index.remove(name);
            }
        }
        let _rtid_to_index_removal = self
            .rtid_to_index
            .remove_entry(&self.nodes[index].runtime_id);

        // Remove from parent's children
        let parent_index = self.nodes[index].parent;
        if let Some(pos) = self.nodes[parent_index]
            .children
            .iter()
            .position(|&x| x == index)
        {
            self.nodes[parent_index].children.remove(pos);
        }

        // recursively remove all children (remove_node handles hash maps for each child)
        let children = self.nodes[index].children.clone();
        for &child_index in &children {
            self.remove_node(child_index)?;
        }

        // Remove all children references
        self.nodes[index].children.clear();

        // We leave the node in the vector to keep indices stable
        // but we replace it with an emptpy placeholder
        self.nodes[index] = UITreeNode::new(T::default());

        Ok(())
    }

    pub fn get_path_to_element(&self, index: usize) -> Vec<usize> {
        let mut path = Vec::new();
        let mut current_index = index;
        while current_index != 0 {
            path.push(current_index);
            current_index = self.nodes[current_index].parent;
        }
        path.reverse(); // Reverse to get the path from root to the node
        path
    }

    /// Returns the first node with the given name, or `None` if no match exists.
    /// Multiple nodes may share the same name — use `get_elements_by_name` for all matches.
    pub fn get_element_by_name(&self, name: &str) -> Option<&UITreeNode<T>> {
        self.name_to_index
            .get(name)
            .and_then(|indices| indices.first())
            .map(|idx| self.node(*idx))
    }

    /// Returns all nodes with the given name.
    pub fn get_elements_by_name(&self, name: &str) -> Vec<&UITreeNode<T>> {
        self.name_to_index
            .get(name)
            .map(|indices| indices.iter().map(|idx| self.node(*idx)).collect())
            .unwrap_or_default()
    }

    pub fn get_element_by_runtime_id(&self, runtime_id: &str) -> Option<&UITreeNode<T>> {
        self.rtid_to_index
            .get(runtime_id)
            .map(|idx| self.node(*idx))
    }

    /// Walks the tree and calls the callback on each node's data, immutably.
    pub fn for_each<F>(&self, mut callback: F)
    where
        F: FnMut(usize, &T),
    {
        self.for_each_recursive(self.root(), &mut callback, 0);
    }

    fn for_each_recursive<F>(&self, index: usize, callback: &mut F, depth: usize)
    where
        F: FnMut(usize, &T),
    {
        debug_assert!(depth <= self.nodes.len(), "cycle detected in UITreeMap");
        if depth > self.nodes.len() {
            return;
        }

        let node = &self.nodes[index];
        callback(index, &node.data);

        for &child in &node.children {
            self.for_each_recursive(child, callback, depth + 1);
        }
    }

    pub fn debug_tree_map<F>(&self, index: usize, indent: usize, display: &F, depth: usize)
    where
        F: Fn(&T) -> String,
    {
        if depth > self.nodes.len() {
            println!(
                "{}(Max depth exceeded at node {})",
                " ".repeat(indent),
                index
            );
            return;
        }

        let node = &self.nodes[index];
        let prefix = " ".repeat(indent);
        println!("{}{}: {}", prefix, &node.name, display(&node.data));

        for &child in &node.children {
            self.debug_tree_map(child, indent + 2, display, depth + 1);
        }
    }

    pub fn debug_with<F>(&self, f: &mut std::fmt::Formatter<'_>, display: &F) -> std::fmt::Result
    where
        F: Fn(&T) -> String,
    {
        self.debug_fmt_node_with(f, self.root(), 0, display, 0)
    }

    fn debug_fmt_node_with<F>(
        &self,
        f: &mut std::fmt::Formatter<'_>,
        index: usize,
        indent: usize,
        display: &F,
        depth: usize,
    ) -> std::fmt::Result
    where
        F: Fn(&T) -> String,
    {
        if depth > self.nodes.len() {
            writeln!(
                f,
                "{}(Max depth exceeded at node {})",
                " ".repeat(indent),
                index
            )?;
            return Ok(());
        }

        let node = &self.nodes[index];
        let prefix = " ".repeat(indent);
        writeln!(f, "{}{}: {}", prefix, node.name, display(&node.data))?;

        for &child in &node.children {
            self.debug_fmt_node_with(f, child, indent + 2, display, depth + 1)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tree() -> UITreeMap<String> {
        let mut tree = UITreeMap::new("Root".into(), "rt-root".into(), "root-data".into());
        tree.add_child(0, "Child1", "rt-c1", "c1-data".into());
        tree.add_child(0, "Child2", "rt-c2", "c2-data".into());
        tree.add_child(1, "GrandChild1", "rt-gc1", "gc1-data".into());
        tree
    }

    #[test]
    fn test_new_tree_has_root() {
        let tree = UITreeMap::new("Root".into(), "rt-root".into(), "data".to_string());
        assert_eq!(tree.root(), 0);
        assert_eq!(tree.node_count(), 1);
        assert_eq!(tree.node(0).name, "Root");
        assert_eq!(tree.node(0).runtime_id, "rt-root");
    }

    #[test]
    fn test_add_child() {
        let mut tree = UITreeMap::new("Root".into(), "rt-root".into(), "data".to_string());
        let idx = tree.add_child(0, "Child", "rt-child", "child-data".into());
        assert_eq!(idx, 1);
        assert_eq!(tree.node_count(), 2);
        assert_eq!(tree.node(idx).name, "Child");
        assert_eq!(tree.node(idx).parent, 0);
        assert!(tree.children(0).contains(&idx));
    }

    #[test]
    fn test_add_multiple_children() {
        let tree = sample_tree();
        assert_eq!(tree.node_count(), 4);
        assert_eq!(tree.children(0), &[1, 2]);
        assert_eq!(tree.children(1), &[3]);
        assert!(tree.children(2).is_empty());
    }

    #[test]
    fn test_get_element_by_name() {
        let tree = sample_tree();
        let node = tree.get_element_by_name("Child1").unwrap();
        assert_eq!(node.index, 1);
        assert_eq!(node.data, "c1-data");
        assert!(tree.get_element_by_name("Nonexistent").is_none());
    }

    #[test]
    fn test_get_element_by_runtime_id() {
        let tree = sample_tree();
        let node = tree.get_element_by_runtime_id("rt-gc1").unwrap();
        assert_eq!(node.index, 3);
        assert_eq!(node.name, "GrandChild1");
        assert!(tree.get_element_by_runtime_id("rt-missing").is_none());
    }

    #[test]
    fn test_get_path_to_element() {
        let tree = sample_tree();
        let path = tree.get_path_to_element(3);
        assert_eq!(path, vec![1, 3]);
    }

    #[test]
    fn test_get_path_to_root_is_empty() {
        let tree = sample_tree();
        let path = tree.get_path_to_element(0);
        assert!(path.is_empty());
    }

    #[test]
    fn test_remove_node() {
        let mut tree = sample_tree();
        tree.remove_node(2).unwrap();
        assert!(!tree.children(0).contains(&2));
        assert!(tree.get_element_by_name("Child2").is_none());
        assert!(tree.get_element_by_runtime_id("rt-c2").is_none());
    }

    #[test]
    fn test_remove_node_cascades_to_children() {
        let mut tree = sample_tree();
        tree.remove_node(1).unwrap();
        assert!(!tree.children(0).contains(&1));
        assert!(tree.get_element_by_name("Child1").is_none());
        assert!(tree.get_element_by_name("GrandChild1").is_none());
        assert!(tree.get_element_by_runtime_id("rt-gc1").is_none());
    }

    #[test]
    fn test_remove_root_fails() {
        let mut tree = sample_tree();
        let result = tree.remove_node(0);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TreeMapError::InvalidIndex(0, _)
        ));
    }

    #[test]
    fn test_remove_invalid_index_fails() {
        let mut tree = sample_tree();
        let result = tree.remove_node(99);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TreeMapError::InvalidIndex(99, _)
        ));
    }

    #[test]
    fn test_for_each_visits_all() {
        let tree = sample_tree();
        let mut visited = Vec::new();
        tree.for_each(|idx, _data| {
            visited.push(idx);
        });
        assert_eq!(visited.len(), 4);
        assert!(visited.contains(&0));
        assert!(visited.contains(&1));
        assert!(visited.contains(&2));
        assert!(visited.contains(&3));
    }

    #[test]
    fn test_has_node() {
        let tree = sample_tree();
        assert!(tree.has_node(0));
        assert!(tree.has_node(3));
        assert!(!tree.has_node(4));
    }

    #[test]
    fn test_duplicate_names_not_lost() {
        let mut tree = UITreeMap::new("Root".into(), "rt-root".into(), "root".to_string());
        tree.add_child(0, "Button", "rt-b1", "b1-data".into());
        tree.add_child(0, "Button", "rt-b2", "b2-data".into());
        tree.add_child(0, "Button", "rt-b3", "b3-data".into());

        // get_element_by_name returns the first inserted
        let first = tree.get_element_by_name("Button").unwrap();
        assert_eq!(first.index, 1);

        // get_elements_by_name returns all three
        let all = tree.get_elements_by_name("Button");
        assert_eq!(all.len(), 3);
        let indices: Vec<usize> = all.iter().map(|n| n.index).collect();
        assert_eq!(indices, vec![1, 2, 3]);
    }

    #[test]
    fn test_remove_duplicate_name_preserves_others() {
        let mut tree = UITreeMap::new("Root".into(), "rt-root".into(), "root".to_string());
        tree.add_child(0, "Button", "rt-b1", "b1-data".into());
        tree.add_child(0, "Button", "rt-b2", "b2-data".into());

        // Remove the first "Button" (index 1)
        tree.remove_node(1).unwrap();

        // The second "Button" (index 2) should still be findable
        let remaining = tree.get_element_by_name("Button").unwrap();
        assert_eq!(remaining.index, 2);

        let all = tree.get_elements_by_name("Button");
        assert_eq!(all.len(), 1);
    }
}
