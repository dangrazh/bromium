
mod macros;

use std::time::Instant;
use chrono::Utc;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
// use std::path::PathBuf;


use std::sync::mpsc::channel;
use std::thread;
use std::sync::mpsc::{Receiver, Sender};
// use uitree::{UITree, get_all_elements};
// use uitree::{UITreeIter, get_all_elements_iterative};
// use uitree::{UITreeXML, get_all_elements_xml};
use uitree::{UITreeXML, get_all_elements_xml, get_all_elements_par_xml};

struct FileWriter {
    // outfile_name: PathBuf,
    outfile_writer: BufWriter<File>,
}

impl FileWriter {
    fn new(outfile_prefix: &str) -> Self {
        
        let tmstmp = Utc::now().format("%Y%m%d%H%M%S").to_string();
        let filename = if outfile_prefix.contains("xml") {
            format!("uitree_{}_{}.xml", outfile_prefix, tmstmp)
        } else {
            format!("uitree_{}_{}.txt", outfile_prefix, tmstmp)
        };

                
        let err_msg = format!("Unable to create file: {}", filename);

        let f = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&filename)
            .expect(&err_msg);
        let outfile_writer = BufWriter::new(f);

        FileWriter { outfile_writer }
    }

    fn write(&mut self, content: &str) {
        self.outfile_writer.write_all(content.as_bytes())
            .expect("Unable to write to file");
    }
    
}

fn main() {

    // create file writers
    // let mut file_writer_recursive = FileWriter::new("recursive_uitree");
    // let mut file_writer_iterative = FileWriter::new("iterative_uitree");
    let mut file_writer_xml = FileWriter::new("xml_uitree");
    let mut file_writer_par_xml = FileWriter::new("xml_parallel_uitree");

    /*
    // recursive
    
    let (tx, rx): (Sender<_>, Receiver<UITree>) = channel();
    printfmt!("Spawning separate thread to get ui tree");
    let start = Instant::now();
    thread::spawn(|| {
        get_all_elements(tx, None);
    });
    printfmt!("Spawned separate thread to get ui tree");
    
    let ui_tree: UITree = rx.recv().unwrap();
    let elapsed = start.elapsed();
    printfmt!("Time taken to get ui tree recursive: {:#?}", elapsed);
    // printfmt!("done getting ui tree");
    printfmt!("No of elemetns in UI Tree: {:#}", ui_tree.get_elements().len());
    
    // ui_tree.for_each(|_index, element| {
    //     // printfmt!("Element: {:#?}", element);
    //     // write to file
    //     file_writer_recursive.write(&format!("{:#?}\n", element));
    // });
    */
    
    //*****
    // XML Dom tree
    //*****
    let (tx_xml, rx_xml): (Sender<_>, Receiver<UITreeXML>) = channel();
    printfmt!("Spawning separate thread to get ui tree in XML format");
    let start_xml = Instant::now();
    thread::spawn(move || {
        get_all_elements_xml(tx_xml, None, None, None);
    });
    printfmt!("Spawned separate thread to get ui tree in XML format");
    let ui_tree_xml: UITreeXML = rx_xml.recv().unwrap();
    let elapsed_xml = start_xml.elapsed();
    printfmt!("Time taken to get ui tree in XML format: {:#?}", elapsed_xml);
    printfmt!("No of elemetns in UI Tree XML: {:#}", ui_tree_xml.get_elements().len());
    file_writer_xml.write(&ui_tree_xml.get_xml_dom_tree());
    // dbg!(ui_tree_xml);
    // printfmt!("XML DOM tree: {}", xml_dom_tree);
    
    //*****
    // XML Dom tree - parallel
    //*****
    let (tx_par_xml, rx_par_xml): (Sender<_>, Receiver<UITreeXML>) = channel();
    printfmt!("Spawning separate thread to get ui tree in XML format");
    let start_par_xml = Instant::now();
    thread::spawn(move || {
        get_all_elements_par_xml(tx_par_xml, None, None);
    });
    printfmt!("Spawned separate thread to parallel get ui tree in XML format");
    let ui_tree_par_xml: UITreeXML = rx_par_xml.recv().unwrap();
    let elapsed_par_xml = start_par_xml.elapsed();
    printfmt!("Time taken to get ui tree in parallel in XML format: {:#?}", elapsed_par_xml);
    printfmt!("No of elemetns in UI Tree XML: {:#}", ui_tree_par_xml.get_elements().len());
    file_writer_par_xml.write(&ui_tree_par_xml.get_xml_dom_tree());
    // dbg!(ui_tree_par_xml);
    // printfmt!("XML DOM tree: {}", xml_dom_tree);
    




    // // iterative
    // let (tx_iter, rx_iter): (Sender<_>, Receiver<UITreeIter>) = channel();
    // printfmt!("Spawning separate thread to get ui tree iteratively");
    // let start_iter = Instant::now();
    // thread::spawn(move || {
    //     get_all_elements_iterative(tx_iter, None);
    // });
    // printfmt!("Spawned separate thread to get ui tree iteratively");
    
    // let ui_tree_iter: UITreeIter = rx_iter.recv().unwrap();
    // let elapsed_iter = start_iter.elapsed();
    // printfmt!("Time taken to get ui tree iteratively: {:#?}", elapsed_iter);
    // // printfmt!("done getting ui tree iteratively");
    // printfmt!("No of elemetns in UI Tree Iter: {:#}", ui_tree_iter.get_elements().len());
    
    // ui_tree_iter.for_each(|_index, element| {
    //     // printfmt!("Element: {:#?}", element);
    //     // write to file
    //     file_writer_iterative.write(&format!("{:#?}\n", element));
    // });
    
}

