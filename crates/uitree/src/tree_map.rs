//! A generic tree structure with fast key-value lookup (not collision safe!)
#![allow(dead_code)]

use crate::{UIHashMap, UIHashSet};

// A generic node in a UITreeMap
#[derive(Debug, Clone, Default)]
pub struct UITreeNode<T> {
    pub name: String,
    pub index: usize,
    pub parent: usize,
    pub children: Vec<usize>,
    pub data: T,
}

impl<T: Default> UITreeNode<T> {
    pub fn new(data: T) -> Self {
        Self {
            name: String::new(),
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
    name_to_index: UIHashMap<String, usize>, // Name-to-index map for optional lookups
    rtid_to_index: UIHashMap<String, usize>,
}

impl<T> UITreeMap<T> {
    pub fn new(root_name: String, rt_id: String, root_data: T) -> Self {
        let root = UITreeNode {
            name: root_name.clone(),
            index: 0,
            parent: 0,
            children: Vec::new(),
            data: root_data,
        };

        let mut name_to_index = UIHashMap::default();
        let mut rtid_to_index = UIHashMap::default();
        
        name_to_index.insert(root_name, 0);
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

    pub fn nodes(&self) -> &Vec<UITreeNode<T>> {
        &self.nodes
    }


    pub fn add_child(&mut self, parent: usize, name: &str, rt_id: &str, data: T) -> usize {
        let index = self.nodes.len();
        let node = UITreeNode {
            name: name.to_string(),
            index,
            parent,
            children: Vec::with_capacity(15),
            data,
        };

        self.name_to_index.insert(name.to_string(), index);
        self.rtid_to_index.insert(rt_id.to_string(), index);
        self.nodes[parent].children.push(index);
        self.nodes.push(node);
        index
    }

    pub fn remove_node(&mut self, index: usize) -> Result<() , String> 
    where
        T: Default,
    {
        if index == 0 || index >= self.nodes.len() {
            println!("Error: Attempting to remove index: {} on TreeMap with {} nodes", index, self.nodes.len());
            return Err("Cannot remove root or invalid index".to_string());
        }

        // Remove from hash maps
        let _name_to_index_removal = self.name_to_index.remove_entry(&self.nodes[index].name);
        let _rtid_to_index_removal = self.rtid_to_index.remove_entry(&self.nodes[index].name);


        // Remove from parent's children
        let parent_index = self.nodes[index].parent;
        if let Some(pos) = self.nodes[parent_index].children.iter().position(|&x| x == index) {
            self.nodes[parent_index].children.remove(pos);
        }

        // recursively remove all children and handle hash maps
        let children = self.nodes[index].children.clone();
        for &child_index in &children {
            let _name_to_index_removal = self.name_to_index.remove_entry(&self.nodes[child_index].name);
            let _rtid_to_index_removal = self.rtid_to_index.remove_entry(&self.nodes[child_index].name);
            self.remove_node(child_index)?;
            
        }
        
        // Remove all children references
        self.nodes[index].children.clear(); 
        

        // We leave the node in the vector to keep indices stable
        // but we replace it with an emptpy placeholder
        self.nodes[index] = UITreeNode::new(T::default());
        
        Ok(())
        
    }

    // pub fn append_subtree(&mut self, parent: usize, subtree: &UITreeMap<T>) -> usize 
    // where
    //     T: Clone,
    // {
    //     todo!("Implement subtree appending");
    //     // Steps to implement:
    //     // 1. Clone the root of the subtree and add it as a child of the parent
    //     // 2. Recursively add all children, updating their parent references
    //     // 3. Update the name_to_index and rtid_to_index maps accordingly
    //     // 4. Return the index of the new subtree root in the current tree
    // }



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

    pub fn get_element_by_name(&self, name: &str) -> Option<&UITreeNode<T>> {

        let mut ret_val: Option<&UITreeNode<T>> = None;

        if let Some(idx) = self.name_to_index.get(name) {
            ret_val = Some(self.node(*idx));
        }

        ret_val
    }

    pub fn get_element_by_runtime_id(&self, runtime_id: &str) -> Option<&UITreeNode<T>> {

        let mut ret_val: Option<&UITreeNode<T>> = None;

        if let Some(idx) = self.rtid_to_index.get(runtime_id) {
            ret_val = Some(self.node(*idx));
        }

        ret_val

    }

    /// Walks the tree and calls the callback on each node's data, immutably
    pub fn for_each<F>(&self, mut callback: F)
    where
        F: FnMut(usize, &T),
    {
        let mut visited = UIHashSet::new();
        self.for_each_recursive(self.root(), &mut callback, &mut visited);
    }

    /// Internal helper for recursive traversal.
    fn for_each_recursive<F>(&self, index: usize, callback: &mut F, visited: &mut UIHashSet<usize>)
    where
        F: FnMut(usize, &T),
    {
        if visited.contains(&index) {
            return; // Prevent cycles
        }
        visited.insert(index);

        let node = &self.nodes[index];
        callback(index, &node.data);

        for &child in &node.children {
            self.for_each_recursive(child, callback, visited);
        }
    }

    pub fn debug_tree<F>(&self, index: usize, indent: usize, display: &F, visited: &mut UIHashSet<usize>)
    where
        F: Fn(&T) -> String,
    {
        if visited.contains(&index) {
            println!("{}(Cycle detected at node {})", " ".repeat(indent), index);
            return;
        }
        visited.insert(index);

        let node = &self.nodes[index];
        let prefix = " ".repeat(indent);
        println!("{}{}: {}", prefix, &node.name, display(&node.data));

        for &child in &node.children {
            self.debug_tree(child, indent + 2, display, visited);
        }
    }

    pub fn debug_with<F>(&self, f: &mut std::fmt::Formatter<'_>, display: &F) -> std::fmt::Result
    where
        F: Fn(&T) -> String,
    {
        let mut visited = UIHashSet::default();
        self.debug_fmt_node_with(f, self.root(), 0, display, &mut visited)
    }

    fn debug_fmt_node_with<F>(
        &self,
        f: &mut std::fmt::Formatter<'_>,
        index: usize,
        indent: usize,
        display: &F,
        visited: &mut UIHashSet<usize>,
    ) -> std::fmt::Result
    where
        F: Fn(&T) -> String,
    {
        if visited.contains(&index) {
            writeln!(f, "{}(Cycle detected at node {})", " ".repeat(indent), index)?;
            return Ok(());
        }
        visited.insert(index);

        let node = &self.nodes[index];
        let prefix = " ".repeat(indent);
        writeln!(f, "{}{}: {}", prefix, node.name, display(&node.data))?;

        for &child in &node.children {
            self.debug_fmt_node_with(f, child, indent + 2, display, visited)?;
        }

        Ok(())
    }
}

pub trait UITree {
    type Data;

    fn tree_mut(&mut self) -> &mut UITreeMap<Self::Data>;
    fn tree(&self) -> &UITreeMap<Self::Data>;

    fn root(&self) -> usize {
        0
    }

    fn add_child<'a>(&'a mut self, parent: usize, name: &str, rt_id: &str, data: Self::Data) -> UITreeCursor<'a, Self::Data> {
        let child_index = self.tree_mut().add_child(parent, name, rt_id, data);
        UITreeCursor {
            tree: self.tree_mut(),
            parent_index: parent,
            current_index: child_index,
        }
    }

    fn debug_tree(&self, display: impl Fn(&Self::Data) -> String) {
        let mut visited = UIHashSet::new();
        self.tree().debug_tree(self.root(), 0, &display, &mut visited);
    }

}

// Cursor for chaining child and sibling additions
pub struct UITreeCursor<'a, T> {
    tree: &'a mut UITreeMap<T>,
    parent_index: usize,
    current_index: usize,
}

impl<'a, T: Default> UITreeCursor<'a, T> {
    pub fn new(tree: &'a mut UITreeMap<T>, parent_index: usize, current_index: usize) -> Self {
        Self {
            tree,
            parent_index,
            current_index,
        }
    }

    /// Add a child to the current node.
    pub fn add_child(mut self, name: &str, rt_id: &str, data: T) -> Self {
        let child_index = self.tree.add_child(self.current_index, name, rt_id, data);
        self.parent_index = self.current_index;
        self.current_index = child_index;
        self
    }

    /// Add a sibling to the current node.
    pub fn add_sibling(mut self, name: &str, rt_id: &str, data: T) -> Self {
        let sibling_index = self.tree.add_child(self.parent_index, name, rt_id, data);
        self.current_index = sibling_index;
        self
    }

    /// Return the parent node of this node.
    pub fn up(mut self) -> Self {
        let parent_node = self.tree.node(self.parent_index);
        self.parent_index = parent_node.parent;
        self.current_index = parent_node.index;
        self
    }

    /// Return the index of the current node.
    pub fn index(&self) -> usize {
        self.current_index
    }
}