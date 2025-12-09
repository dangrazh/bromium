use crate::windriver::Element;
// use crate::xpath::get_path_to_element;
// use crate::xpath::XpathElement;
use uitree::UITreeXML;
use uiautomation::{UIAutomation, UIElement}; // controls::ControlType, 
use log::{debug, error, info, warn}; // trace, 


// enum FindResult {
//     FoundSingle(UIElement),
//     FoundMultiple(Vec<UIElement>),
//     NotFound,
// }

fn get_ui_automation_instance() -> Option<UIAutomation> {
    debug!("Creating UIAutomation instance");

    let uia: UIAutomation;
    let uia_res = UIAutomation::new();
    
    match uia_res {
        Ok(uia_ok) => {
            uia = uia_ok;
            info!("UIAutomation instance created successfully");
        },
        Err(e) => {
            warn!("Failed to create UIAutomation instance, trying direct method: {:?}", e);
            let uia_direct_res = UIAutomation::new_direct();
            match uia_direct_res {
                Ok(uia_direct_ok) => {
                    uia = uia_direct_ok;
                    info!("UIAutomation instance created successfully using direct method.");
                },
                Err(e_direct) => {
                    error!("Failed to create UIAutomation instance using direct method: {:?}", e_direct);
                    return None; // Return None if we cannot create a UIAutomation instance
                }
            }
        }
        
    }
    Some(uia)

}

#[allow(dead_code)]
pub fn get_ui_element_by_xpath(xpath: String, _ui_tree: &UITreeXML) -> Option<Element> {
    debug!("UIAutomation::get_element_by_xpath called, xpath: {}", xpath);
    todo!();
}

// pub fn get_element_by_xpath(xpath: String, ui_tree: &UITree) -> Option<Element> {
//     debug!("UIAutomation::get_element_by_xpath called, xpath: {}", xpath);
// // Returns the Windows UI Automation API UI element of the window at the given xpath. As an xpath
// // is a string representation of the UI element, it is not a valid xpath in the XML sense.
// // The search is following a three step approach:
// // 1. A UI element is searched by its exact xpath.
// // 2. If the xpath does not provide a unique way to identify an elemt, the element is 
// //     searched for in the entire UI sub-tree.
// //     2.1. If there is a single matching element, this element is returned (irrespective if the xpath is a 100% match).
// //     2.2. If there are multiple matching elements, each found element is checked if the xpath
// //         matches and if a matching xpath is found the respective element is returned.
// // 3. if no matching element is found, None is returned.

    
//     let mut input = xpath.as_str();
//     let path_to_element: Vec<XpathElement<'_>>;
    
//     // Start with a depth of 2 as we are always looking for an element next level down in the UI tree
//     let mut search_depth = 2; 
    
//     if let Ok(path_returned) = get_path_to_element(&mut input) {
//         trace!("Path to element parsed successfully: {:?}", path_returned);
//         path_to_element = path_returned;
//     } else {
//         error!("Failed to get path to element.");
//         return None;
//     }

//     let uia = get_ui_automation_instance().unwrap();

//     let mut root = uia.get_root_element().unwrap();
//     'outer: for element in &path_to_element {
//         trace!("Looking for Element: {:?}", element);
//         let found = get_next_element(root.clone(), &element.clone(), search_depth);
//         match found {
//             FindResult::FoundSingle(found_element) => {
//                 trace!("Element found: {:?}", found_element);
//                 root = found_element;
//             },
//             FindResult::FoundMultiple(found_elements) => {
//                 trace!("Found multiple elements: {:?}", found_elements);
//                 // trying the lucky punch and just search the target element (i.e. the last one in the xpath)
//                 search_depth = 99;
//                 let final_element = path_to_element.last().unwrap();
//                 trace!("Looking for Element: {:?}", final_element);
//                 let found = get_next_element(root.clone(), &final_element.clone(), search_depth);
//                 match found {
//                     FindResult::FoundSingle(found_element) => {
//                         trace!("Element found: {:?}", found_element);
//                         root = found_element;
//                         break; // Exit the loop after finding the target element
//                     },
//                     FindResult::FoundMultiple(found_elements) => {
//                         trace!("Found again multiple elements: {:?}", found_elements);
//                         // loop through the found elements and construct a new xpath for each element
//                         // and check if the xpath matches the target element
//                         for found_element in found_elements {
//                             if let Ok(optional_point) = found_element.get_clickable_point() {
//                                 let clickable_point = optional_point.unwrap_or_default();
//                                 let point: windows::Win32::Foundation::POINT = clickable_point.into();
//                                 trace!("Found element at: {:?}", point);

//                                 //TODO: replace this with a function that generates the xpath from the UIElementInTree
//                                 // based on the point coordinates and the UITree structure pased to the function
//                                 // let xpath_candidate = generate_xpath(point.get_x(), point.get_y());
//                                 let mut xpath_candidate = String::from("not found");
//                                 if let Some(ui_element_in_tree) = crate::rectangle::get_point_bounding_rect(&point, ui_tree.get_elements()) {
//                                     xpath_candidate = ui_tree.get_xpath_for_element(ui_element_in_tree.get_tree_index());
//                                 }
                                
//                                 if xpath_candidate == xpath {
//                                     trace!("Found target element: {:?}", found_element);
//                                     root = found_element;
//                                     break 'outer; // Exit the inner and outer loop after finding the target element
//                                 } else {
//                                     trace!("Found element but not matching xpath: {:?}", xpath_candidate);
//                                     //skip this element
//                                 }
//                             } else {
//                                 trace!("Failed to get clickable point for element: {:?}", found_element);
//                                 //skip this element
//                             }
//                         }
                        
//                         trace!("No matching element found for xpath: {:?}", xpath);
//                         return None; // Return None if we find multiple elements again
                        
//                     },
//                     FindResult::NotFound => {
//                         info!("Element not found: {:?}", final_element);
//                         return None;
//                     }
//                 } 
//             },
//             FindResult::NotFound => {
//                 info!("Element not found: {:?}", element);
//                 return None;
//             }
//         }
//     }



//     // If we reach here, we have found the element
//     let name = root.get_name().unwrap_or("".to_string());
//     let xpath = "".to_string(); // Placeholder for the xpath, as we don't have a function to generate it from the element
//     let handle: isize = root.get_native_window_handle().unwrap_or_default().into();
//     let runtimeid: Vec<i32> = root.get_runtime_id().unwrap_or_default();
//     let bounding_rectangle = root.get_bounding_rectangle().unwrap_or_default();
//     let (left, top, right, bottom) =(
//         bounding_rectangle.get_left(),
//         bounding_rectangle.get_top(),
//         bounding_rectangle.get_right(),
//         bounding_rectangle.get_bottom(),
//     );
    
//     let element = Element::new(name, xpath, handle, runtimeid, (left, top, right, bottom));
//     info!("Final Element: {:?}", element);
//     Some(element)
// }

// fn get_next_element(root: UIElement, element: &XpathElement<'_>, depth: u32 ) -> FindResult {
//     debug!("UIAutomation::get_next_element called.");
//     // let uia = UIAutomation::new().unwrap();
//     let uia = get_ui_automation_instance().unwrap();
//     let matcher = uia.create_matcher().from(root).depth(depth);

//     let control_type = ControlType::from_str(element.control_type);
//     let matcher = matcher.control_type(control_type);

//     let matcher = if element.name.is_some() {matcher.name(element.name.unwrap())} else {matcher};
//     let matcher = if element.classname.is_some() {matcher.classname(element.classname.unwrap())} else {matcher};

//     // TODO: add a filter function for automationid
//     // let matcher = if element.automationid.is_some() {matcher.automationid(element.automationid)} else {matcher};
//     // let matcher = matcher.filter_fn(
//     //     Box::new(|e: &UIElement| {
//     //         let framework_id = e.get_framework_id()?;
//     //         let class_name = e.get_classname()?;
        
//     //         Ok("Win32" == framework_id && class_name.starts_with("Shell"))
//     //     }
//     // ));

//     // trace!("Matcher: {:?}", matcher);
    
//     if let Ok(found_elements) = matcher.find_all() { 
//         if found_elements.len() == 1 {
//             trace!("Found exactly one element: {:?}", found_elements);
//             return FindResult::FoundSingle(found_elements[0].clone());
//         } else {
//             trace!("Found multiple elements: {:?}", found_elements);
//             return FindResult::FoundMultiple(found_elements);
//         }
//     } else {
//         info!("No elements found.");
//         return FindResult::NotFound;
//     }
    
// }


// pub fn get_ui_element_by_xpath(xpath: String, ui_tree: &UITree) -> Option<UIElement> {
//     debug!("UIAutomation::get_ui_element_by_xpath called, xpath: {}", xpath);


//     let ui_elem = get_element_by_xpath(xpath.clone(), ui_tree);
//     if ui_elem.is_none() {
//         return None;
//     }
//     let element = ui_elem.unwrap();

//     let runtime_id = element.get_runtime_id();

//     get_ui_element_by_runtimeid(runtime_id)

    
// }



struct RuntimeIdFilter(Vec<i32>);

impl uiautomation::filters::MatcherFilter for RuntimeIdFilter {
    fn judge(&self, element: &UIElement) -> uiautomation::Result<bool> {
        // self is the element we are looking for
        // element is the element we are checking against
        let id = element.get_runtime_id()?;
        Ok(id == self.0)
    }
}


pub fn get_ui_element_by_runtimeid(runtime_id: Vec<i32>) -> Option<UIElement> {
    debug!("Searching for element with runtime id: {:?}", runtime_id);
    // let automation = UIAutomation::new().unwrap();
    let uia = get_ui_automation_instance().unwrap();
    let matcher = uia.create_matcher().timeout(0).filter(Box::new(RuntimeIdFilter(runtime_id))).depth(99);
    let element = matcher.find_first();
    
    match element {
        Ok(e) => {
            info!("Element found by runtime id: {:?}", e);
            Some(e)
        },
        Err(e) => {
            error!("Error finding element by runtime id: {:?}", e);
            None
        }
    }
    
}


mod tests {
    // use super::get_ui_automation_instance;
     #[allow(unused_imports)]
     use log::debug;

    #[test]
    fn test_ui_automation_creation_sta() {
        debug!("UIAutomation::test_ui_automation_creation_sta called.");

        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        
        // Initialize COM library for the current thread with STA (Single Threaded Apartment) model
        // This is done to force the runtime error when uiautomation is initialized with MTA (Multi Threaded Apartment) model
        let _result = unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
        }; 

        // Create a UIAutomation instance
        
        let uia = super::get_ui_automation_instance();
        assert!(uia.is_some(), "Failed to create UIAutomation instance");

    }

    #[test]
    fn test_ui_automation_creation_mta() {
        debug!("UIAutomation::test_ui_automation_creation_mta called.");

        // Create a UIAutomation instance
        let uia = super::get_ui_automation_instance();
        assert!(uia.is_some(), "Failed to create UIAutomation instance");

    }

}