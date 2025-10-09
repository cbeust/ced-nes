use std::{fs, thread};
use std::fs::File;
use std::io::Write;
use std::string::ToString;
use std::sync::RwLock;
use crossbeam::channel::{Sender, unbounded};
use lazy_static::lazy_static;

struct LogFile {
    _buffer: Vec<String>,
    tx: Sender<LogMessage>,
}

const NL: &[u8] = "\n".as_bytes();

enum LogMessage {
    LogLine(String)
}

impl LogFile {
    fn new(file_name: &str) -> Self {
        let c = unbounded();
        let fn2 = file_name.to_string();

        match File::create(file_name.clone()) {
            Ok(_) => { }
            Err(error) => {
                panic!("Couldn't create file {}: {}", &file_name, error);
            }
        };

        let result = Self {
            _buffer: Vec::new(),
            tx: c.0,
        };

        thread::spawn(move || {
            let mut buffer: Vec<String> = Vec::new();
            let mut stop = false;
            while ! stop {
                match c.1.recv() {
                    Ok(m) => {
                        match m {
                            LogMessage::LogLine(_) => {
                                LogFile::receive_log(&mut buffer, &fn2);
                            }
                        }
                    }
                    Err(_) => { stop = true; }
                }
            }
        });

        result
    }

    fn log(&mut self, s: String) {
        let _ = self.tx.send(LogMessage::LogLine(s));
    }

    fn receive_log(buffer: &mut Vec<String>, file_name: &str) {
        // buffer.push(s.to_string());
        if buffer.len() > 10_000 {
            match fs::OpenOptions::new().append(true).open(&file_name) {
                Ok(mut file) => {
                    for l in &mut *buffer {
                        file.write_all(l.as_bytes()).expect("Couldn't write to file");
                        file.write_all(NL).unwrap();
                    }
                }
                Err(error) => {
                    panic!("Couldn't append to file {}: {}", file_name, error);
                }
            }
            buffer.clear();
        }
    }

}

