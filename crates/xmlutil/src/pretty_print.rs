use xot::{Xot, output};

pub(crate) fn pretty_print_xml(xmlsrc: &str) -> String {
    let mut xot = Xot::new();
    if let Ok(root) = xot.parse(xmlsrc) {
        xot.serialize_xml_string(
            output::xml::Parameters {
                indentation: Some(Default::default()),
                ..Default::default()
            },
            root,
        )
        .unwrap_or_else(|_| xmlsrc.to_owned())
        .trim_end()
        .to_owned()
    } else {
        xmlsrc.to_owned()
    }
}
