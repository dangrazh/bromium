use chrono::Utc;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::time::{Duration, Instant};

use std::sync::mpsc::channel;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use uitree::{UITreeError, UITreeXML, get_all_elements_par_xml, get_all_elements_xml};

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
        self.outfile_writer
            .write_all(content.as_bytes())
            .expect("Unable to write to file");
    }
}

fn main() {
    // create file writers
    // let mut file_writer_recursive = FileWriter::new("recursive_uitree");
    let mut file_writer_xml = FileWriter::new("xml_uitree");
    let mut file_writer_par_xml = FileWriter::new("xml_parallel_uitree");

    // XML Dom tree
    let (tx_xml, rx_xml): (Sender<_>, Receiver<Result<UITreeXML, UITreeError>>) = channel();
    println!("Spawning separate thread to get ui tree in XML format");
    let start_xml = Instant::now();
    thread::spawn(move || {
        get_all_elements_xml(tx_xml, None, None, None, None, None);
    });
    println!("Spawned separate thread to get ui tree in XML format");
    let ui_tree_xml: UITreeXML = rx_xml
        .recv_timeout(Duration::from_secs(120))
        .expect("UI tree XML refresh timed out or failed")
        .expect("UI tree XML build failed");
    let elapsed_xml = start_xml.elapsed();
    println!(
        "Time taken to get ui tree in XML format: {:#?}",
        elapsed_xml
    );
    println!(
        "No of elemetns in UI Tree XML: {:#}",
        ui_tree_xml.get_elements().len()
    );
    file_writer_xml.write(ui_tree_xml.get_xml_dom_tree());
    // dbg!(ui_tree_xml);
    // println!("XML DOM tree: {}", xml_dom_tree);

    //*****
    // XML Dom tree - parallel
    //*****
    let (tx_par_xml, rx_par_xml): (Sender<_>, Receiver<Result<UITreeXML, UITreeError>>) = channel();
    println!("Spawning separate thread to get ui tree in XML format");
    let start_par_xml = Instant::now();
    thread::spawn(move || {
        get_all_elements_par_xml(tx_par_xml, None, None, None, None);
    });
    println!("Spawned separate thread to parallel get ui tree in XML format");
    let ui_tree_par_xml: UITreeXML = rx_par_xml
        .recv_timeout(Duration::from_secs(120))
        .expect("Parallel UI tree XML refresh timed out or failed")
        .expect("Parallel UI tree XML build failed");
    let elapsed_par_xml = start_par_xml.elapsed();
    println!(
        "Time taken to get ui tree in parallel in XML format: {:#?}",
        elapsed_par_xml
    );
    println!(
        "No of elemetns in UI Tree XML: {:#}",
        ui_tree_par_xml.get_elements().len()
    );
    file_writer_par_xml.write(ui_tree_par_xml.get_xml_dom_tree());
}
