// #![allow(dead_code)]

use quick_xml::events::{BytesCData, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::writer::Writer;
use quick_xml::Error;
use std::io::Cursor;

#[derive(Debug, Clone)]
struct XMLAttribute {
    name: String,
    value: String,
}

impl XMLAttribute {
    fn new(name: &str, value: &str) -> Self {
        XMLAttribute {
            name: name.to_string(),
            value: value.to_string(),
        }
    }
    
}

#[derive(Debug, Clone)]
pub struct XMLAttributes {
    attributes: Vec<XMLAttribute>,
}

impl XMLAttributes {
    pub fn new() -> Self {
        XMLAttributes {
            attributes: Vec::new(),
        }
    }

    pub fn add(&mut self, name: &str, value: &str) {
        self.attributes.push(XMLAttribute::new(name, value));
    }

    pub fn into_iter(self) -> impl Iterator<Item = Result<(String, String), Error>> {
        self.attributes.into_iter().map(|attr| Ok((attr.name, attr.value)))
    }

    pub fn is_empty(&self) -> bool {
        self.attributes.is_empty()
    }
    
    pub fn len(&self) -> usize {
        self.attributes.len()
    }

    pub fn clear(&mut self) {
        self.attributes.clear();
    }

    pub fn get(&self, name: &str) -> Option<&String> {
        self.attributes.iter().find(|attr| attr.name == name).map(|attr| &attr.value)
    }

    pub fn remove(&mut self, name: &str) -> Option<(String, String)>  {
        if let Some(pos) = self.attributes.iter().position(|attr| attr.name == name) {
            let removed = self.attributes.remove(pos);
            Some((removed.name, removed.value))
        } else {
            None
        }
    }

    pub fn set(&mut self, name: &str, value: &str) {
        if let Some(attr) = self.attributes.iter_mut().find(|attr| attr.name == name) {
            attr.value = value.to_string();
        } else {
            self.add(name, value);
        }
    }

    pub fn get_all(&self) -> Vec<(String, String)> {
        self.attributes.iter().map(|attr| (attr.name.clone(), attr.value.clone())).collect()
    }

}

#[derive(Clone)]
pub struct XMLWriter {
   writer: Writer<Cursor<Vec<u8>>>,
}

impl XMLWriter {
    pub fn new() -> Self {
        let writer = Writer::new(Cursor::new(Vec::new()));
        XMLWriter { writer }
    }

    pub fn write_start_element(&mut self, name: &str) -> Result<(), Error> {
        let start = BytesStart::new(name);
        self.writer.write_event(Event::Start(start))?;
        Ok(())
    }

    pub fn write_end_element(&mut self, name: &str) -> Result<(), Error> {
        let end = BytesEnd::new(name);
        self.writer.write_event(Event::End(end))?;
        Ok(())
    }

    pub fn write_text(&mut self, content: &str) -> Result<(), Error> {
        self.writer.write_event(Event::Text(BytesText::new(content)))?;
        Ok(())
    }

    pub fn write_cdata(&mut self, content: &str) -> Result<(), Error> {
        self.writer.write_event(Event::CData(BytesCData::new(content)))?;
        Ok(())
    }

    pub fn write_element(&mut self, name: &str, content: &str, attrs: Option<XMLAttributes>) -> Result<(), Error> {
        let mut start = BytesStart::new(name);
        if let Some(attributes) = attrs {
            let mut key: &str;
            let mut value: &str;
            for attr in attributes.into_iter() {
                let attrs = attr?;
                key = attrs.0.as_str();
                value = attrs.1.as_str();
                start.push_attribute((key, value));
            }

        }

        self.writer.write_event(Event::Start(start))?;
        self.writer.write_event(Event::Text(BytesText::new(content)))?;
        self.writer.write_event(Event::End(BytesEnd::new(name)))?;
        Ok(())
    }

    pub fn get_xml_raw(self) -> Vec<u8> {
        self.writer.into_inner().into_inner()
    }
    
    pub fn get_xml_string(self) -> String {
        String::from_utf8(self.get_xml_raw()).unwrap()
    }

}

#[derive(Debug, Clone)]
pub struct XMLDomNode {
    pub name: String,
    pub attributes: XMLAttributes,
    pub text: Option<String>,
    pub children: Vec<XMLDomNode>,
}

impl XMLDomNode {
    pub fn new(name: &str) -> Self {
        XMLDomNode {
            name: name.to_string(),
            attributes: XMLAttributes::new(),
            text: None,
            children: Vec::new(),
        }
    }

    pub fn with_text(mut self, text: &str) -> Self {
        self.text = Some(text.to_string());
        self
    }

    pub fn add_child(&mut self, child: XMLDomNode) -> &mut Self {
        self.children.push(child);
        self.children.last_mut().unwrap()
    }

    pub fn set_attribute(&mut self, name: &str, value: &str) {
        self.attributes.set(name, value);
    }

    pub fn get_first_child(&self) -> Option<&XMLDomNode> {
        if let Some(first) = self.children.first() {
            return Some(first);
        } else {
            // println!("Warning: No children found for node: {:?}", self);
            return None;
        }
        // self.children.first()
    }
}

pub struct XMLDomWriter {
    root: Option<XMLDomNode>,
}

impl XMLDomWriter {
    pub fn new() -> Self {
        XMLDomWriter { root: None }
    }

    pub fn set_root(&mut self, node: XMLDomNode) {
        self.root = Some(node);
    }

    pub fn get_root_mut(&mut self) -> Option<&mut XMLDomNode> {
        self.root.as_mut()
    }

    pub fn to_string(&self) -> Result<String, Error> {
        let mut writer = XMLWriter::new();
        if let Some(ref root) = self.root {
            // special handling to eliminate duplicate root node from the XML output
            // take the frist child of the root instead of the root itself
            if let Some(ref first_child) = root.get_first_child() {
                Self::write_node(&mut writer, first_child)?;
            } else {
                // write the root node itself, as there is no duplicate root node
                // this is a special case and should not happen in normal usage
                println!("Warning: Root node has no children, writing root node itself: {:?}", root);
                Self::write_node(&mut writer, root)?;
            }

            // println!("Warning: Root node has no children, writing root node itself: {:?}", root);
            // Self::write_node(&mut writer, root)?;
        }
        Ok(writer.get_xml_string())
    }

    fn write_node(writer: &mut XMLWriter, node: &XMLDomNode) -> Result<(), Error> {
        let mut start = BytesStart::new(node.name.as_str());
        for (k, v) in node.attributes.get_all() {
            start.push_attribute((k.as_str(), v.as_str()));
        }
        writer.writer.write_event(Event::Start(start))?;
        if let Some(ref text) = node.text {
            writer.write_text(text)?;
        }
        for child in &node.children {
            Self::write_node(writer, child)?;
        }
        writer.writer.write_event(Event::End(BytesEnd::new(node.name.as_str())))?;
        Ok(())
    }
}

