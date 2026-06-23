pub type UIHashMap<K, V, S = std::hash::RandomState> = std::collections::HashMap<K, V, S>;

mod error;
pub use error::UITreeError;

mod tree_map;
use tree_map::UITreeMap;

mod save_ui_element;
pub use save_ui_element::SaveUIElement;
/// Backward-compatible alias for the canonical `SaveUIElement` type.
pub type SaveUIElementXML = SaveUIElement;

mod common_types;
pub use common_types::UIElementInTree;
/// Backward-compatible alias — all three tree walkers now share one `UIElementInTree`.
pub type UIElementInTreeXML = UIElementInTree;
/// Backward-compatible alias — all three tree walkers now share one `UIElementInTree`.
pub type UIElementInTreeIter = UIElementInTree;

mod uiexplore_xml;
pub use uiexplore_xml::{UITree, get_all_elements_par_xml, get_all_elements_xml};

/// Deprecated: use `UITree` directly.
pub type UITreeXML = UITree;

mod uiexplore;
pub use uiexplore::get_all_elements;

mod uiexplore_iter;
pub use uiexplore_iter::get_all_elements_iterative;
