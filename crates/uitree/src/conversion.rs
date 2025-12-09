use uiautomation::controls::ControlType;

pub trait ConvertFromControlType {
    fn as_str<'a>(&self) -> &'a str;
}

impl ConvertFromControlType for ControlType {
    fn as_str<'a>(&self) -> &'a str {
        match &self {
            ControlType::Button => "Button",
            ControlType::Calendar => "Calendar",
            ControlType::CheckBox => "CheckBox",
            ControlType::ComboBox => "ComboBox",
            ControlType::Edit => "Edit",
            ControlType::Hyperlink => "Hyperlink",
            ControlType::Image => "Image",
            ControlType::ListItem => "ListItem",
            ControlType::List => "List",
            ControlType::Menu => "Menu",
            ControlType::MenuBar => "MenuBar",
            ControlType::MenuItem => "MenuItem",
            ControlType::ProgressBar => "ProgressBar",
            ControlType::RadioButton => "RadioButton",
            ControlType::ScrollBar => "ScrollBar",
            ControlType::Slider => "Slider",
            ControlType::Spinner => "Spinner",
            ControlType::StatusBar => "StatusBar",
            ControlType::Tab => "Tab",
            ControlType::TabItem => "TabItem",
            ControlType::Text => "Text",
            ControlType::ToolBar => "ToolBar",
            ControlType::ToolTip => "ToolTip",
            ControlType::Tree => "Tree",
            ControlType::TreeItem => "TreeItem",
            ControlType::Custom => "Custom",
            ControlType::Group => "Group",
            ControlType::Thumb => "Thumb",
            ControlType::DataGrid => "DataGrid",
            ControlType::DataItem => "DataItem",
            ControlType::Document => "Document",
            ControlType::SplitButton => "SplitButton",
            ControlType::Window => "Window",
            ControlType::Pane => "Pane",
            ControlType::Header => "Header",
            ControlType::HeaderItem => "HeaderItem",
            ControlType::Table => "Table",
            ControlType::TitleBar => "TitleBar",
            ControlType::Separator => "Separator",
            ControlType::SemanticZoom => "SemanticZoom",
            ControlType::AppBar => "AppBar",
        }
    }
}

#[allow(dead_code)]
pub trait ConvertToControlType {
    fn from_str(item: &str) -> ControlType;
}

impl ConvertToControlType for ControlType {
    fn from_str(item: &str) -> Self {
        match item {
            "Button"  => ControlType::Button,
            "Calendar"  => ControlType::Calendar,
            "CheckBox"  => ControlType::CheckBox,
            "ComboBox"  => ControlType::ComboBox,
            "Edit"  => ControlType::Edit,
            "Hyperlink"  => ControlType::Hyperlink,
            "Image"  => ControlType::Image,
            "ListItem"  => ControlType::ListItem,
            "List"  => ControlType::List,
            "Menu"  => ControlType::Menu,
            "MenuBar"  => ControlType::MenuBar,
            "MenuItem"  => ControlType::MenuItem,
            "ProgressBar"  => ControlType::ProgressBar,
            "RadioButton"  => ControlType::RadioButton,
            "ScrollBar"  => ControlType::ScrollBar,
            "Slider"  => ControlType::Slider,
            "Spinner"  => ControlType::Spinner,
            "StatusBar"  => ControlType::StatusBar,
            "Tab"  => ControlType::Tab,
            "TabItem"  => ControlType::TabItem,
            "Text"  => ControlType::Text,
            "ToolBar"  => ControlType::ToolBar,
            "ToolTip"  => ControlType::ToolTip,
            "Tree"  => ControlType::Tree,
            "TreeItem"  => ControlType::TreeItem,
            "Custom"  => ControlType::Custom,
            "Group"  => ControlType::Group,
            "Thumb"  => ControlType::Thumb,
            "DataGrid"  => ControlType::DataGrid,
            "DataItem"  => ControlType::DataItem,
            "Document"  => ControlType::Document,
            "SplitButton"  => ControlType::SplitButton,
            "Window"  => ControlType::Window,
            "Pane"  => ControlType::Pane,
            "Header"  => ControlType::Header,
            "HeaderItem"  => ControlType::HeaderItem,
            "Table"  => ControlType::Table,
            "TitleBar"  => ControlType::TitleBar,
            "Separator"  => ControlType::Separator,
            "SemanticZoom"  => ControlType::SemanticZoom,
            "AppBar"  => ControlType::AppBar,
            _ => ControlType::Custom, // Default case
        }
    }
}
