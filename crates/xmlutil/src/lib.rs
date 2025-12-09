pub mod xpath_gen;
pub mod xpath_eval;
pub mod xml;
pub mod pretty_print;
pub mod xml_dom_manager;

pub use xpath_gen::*;
pub use xpath_eval::*;
pub use xml::*;
// pub use pretty_print::*;

pub use xml_dom_manager::*;



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_xpath() {
        
        let xpath = "/doc/section1/para";
        let xml = r#"<doc>
                                    <section1>
                                        <heading>A Level-1 Heading</heading>
                                        <para RtId="1-1">The first paragraph.</para>
                                        <para RtId="1-2">The second paragraph.</para>
                                        <section2>
                                        <heading type="myHeading">A Level-2 Heading</heading>
                                        <para>The first paragraph after the sub-heading.</para>
                                        </section2>
                                    </section1>
                                    </doc>
                                    "#;

        let res: XpathResult = xpath_eval::eval_xpath(xpath.to_owned(), xml.to_owned());
        
        assert_eq!(res.get_result_count(), 2);
        // println!("Result items: {:?}", res.get_result_items());
        assert_eq!(res.get_result_items().get(1).unwrap_or(&XpathQueryResult::default()).get_item_xml(), &"<para RtId=\"1-2\">The second paragraph.</para>".to_string());
        assert_eq!(res.get_result_items().get(1).unwrap_or(&XpathQueryResult::default()).get_item_value(), &"The second paragraph.".to_string());

    }

    #[test]
    fn test_eval_xpath_no_result() {
        
        let xpath = "/doc/section1/para[@RtId='nonexistent']";
        let xml = r#"<doc>
                                    <section1>
                                        <heading>A Level-1 Heading</heading>
                                        <para RtId="1-1">The first paragraph.</para>
                                        <para RtId="1-2">The second paragraph.</para>
                                        <section2>
                                        <heading type="myHeading">A Level-2 Heading</heading>
                                        <para>The first paragraph after the sub-heading.</para>
                                        </section2>
                                    </section1>
                                    </doc>
                                    "#;

        let res: XpathResult = xpath_eval::eval_xpath(xpath.to_owned(), xml.to_owned());
        
        assert_eq!(res.get_result_count(), 0);
        assert_eq!(res.get_result_items().len(), 0);
        assert_eq!(res.get_result_items().get(0).unwrap_or(&XpathQueryResult::default()).get_item_xml(), &"".to_string());
        assert_eq!(res.get_result_items().get(0).unwrap_or(&XpathQueryResult::default()).get_item_value(), &"".to_string());
    }
    
    #[test]
    fn test_eval_xpath_attribute() {
        
        let xpath = "/doc/section1/para[@RtId='1-2']";
        let xml = r#"<doc>
                                    <section1>
                                        <heading>A Level-1 Heading</heading>
                                        <para RtId="1-1">The first paragraph.</para>
                                        <para RtId="1-2">The second paragraph.</para>
                                        <section2>
                                        <heading type="myHeading">A Level-2 Heading</heading>
                                        <para>The first paragraph after the sub-heading.</para>
                                        </section2>
                                    </section1>
                                    </doc>
                                    "#;

        let res: XpathResult = xpath_eval::eval_xpath(xpath.to_owned(), xml.to_owned());
        
        assert_eq!(res.get_result_count(), 1);
        assert_eq!(res.get_result_items().len(), 1);
        assert_eq!(res.get_result_items().get(0).unwrap_or(&XpathQueryResult::default()).get_item_xml(), &"<para RtId=\"1-2\">The second paragraph.</para>".to_string());
        assert_eq!(res.get_result_items().get(0).unwrap_or(&XpathQueryResult::default()).get_item_value(), &"The second paragraph.".to_string());
    }

    #[test]
    fn test_eval_xpath_return_attribute() {
        
        let xpath = "/doc/section1/para[@RtId='1-2']/@RtId";
        let xml = r#"<doc>
                                    <section1>
                                        <heading>A Level-1 Heading</heading>
                                        <para RtId="1-1">The first paragraph.</para>
                                        <para RtId="1-2">The second paragraph.</para>
                                        <section2>
                                        <heading type="myHeading">A Level-2 Heading</heading>
                                        <para>The first paragraph after the sub-heading.</para>
                                        </section2>
                                    </section1>
                                    </doc>
                                    "#;

        let res: XpathResult = xpath_eval::eval_xpath(xpath.to_owned(), xml.to_owned());
        
        assert_eq!(res.get_result_count(), 1);
        assert_eq!(res.get_result_items().len(), 1);
        assert_eq!(res.get_result_items().get(0).unwrap_or(&XpathQueryResult::default()).get_item_xml(), &"Attribute RtId=\"1-2\"".to_string());
        assert_eq!(res.get_result_items().get(0).unwrap_or(&XpathQueryResult::default()).get_item_value(), &"1-2".to_string());
    }
}
