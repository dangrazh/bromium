
pub type UIHashMap<K, V, S = std::hash::RandomState> = std::collections::HashMap<K, V, S>;
type UIHashSet<T, S = std::hash::RandomState> = std::collections::HashSet<T, S>;

mod macros;

mod tree_map;
use tree_map::UITreeMap;

mod uiexplore;
pub use uiexplore::{SaveUIElement, UITree, UIElementInTree, get_all_elements};

mod uiexplore_xml;
pub use uiexplore_xml::{UITree as UITreeXML, UIElementInTree as UIElementInTreeXML, get_all_elements_xml, get_all_elements_par_xml}; // SaveUIElement as SaveUIElementXML, 


mod uiexplore_iter;
pub use uiexplore_iter::{SaveUIElement as SaveUIElementIter, UITree as UITreeIter, UIElementInTree as UIElementInTreeIter, get_all_elements_iterative};

mod conversion;
pub use conversion::{ConvertFromControlType, ConvertToControlType};

mod commons;

mod save_ui_element;
pub use save_ui_element::SaveUIElement as SaveUIElementXML;