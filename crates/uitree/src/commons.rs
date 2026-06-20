#![allow(dead_code)]
use chrono::Utc;

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};

pub struct FileWriter {
    outfile_writer: BufWriter<File>,
}

impl FileWriter {
    pub fn new(outfile_prefix: &str) -> Self {
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

    pub fn write(&mut self, content: &str) {
        self.outfile_writer
            .write_all(content.as_bytes())
            .expect("Unable to write to file");
    }
}
