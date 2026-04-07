use std::{fs, thread};
use std::fs::File;
use std::io::Write;
use std::string::ToString;
use std::sync::{Arc, OnceLock, RwLock};
use crossbeam::channel::{Sender, unbounded};
use tracing::info;
use crate::external_logger::IExternalLogger;
use crate::labels::Labels;
use crate::messages::LogMsg;
use crate::operand::Operand;

pub struct LogFile {
    // buffer: Arc<RwLock<Vec<String>>>,
    asyn: bool,
    tx: Sender<LogMessage>,
    logger: Arc<RwLock<Option<Box<dyn IExternalLogger + 'static>>>>
}

const NL: &[u8] = "\n".as_bytes();

static LABELS: OnceLock<Labels> = OnceLock::new();
static OPERANDS: OnceLock<[Operand; 256]> = OnceLock::new();

enum LogMessage {
    NewLog(LogMsg, bool)
}

impl LogFile {
    pub(crate) fn new(file_name: &str, logger: Arc<RwLock<Option<Box<dyn IExternalLogger + 'static>>>>,
        asyn: bool, labels: &Labels, operands: [Operand; 256])
        -> Self
    {
        let _ = LABELS.get_or_init(|| labels.clone());
        let _ = OPERANDS.get_or_init(|| operands);
        let c = unbounded();
        let buffer = Arc::new(RwLock::new(Vec::new()));
        if asyn {
            match File::create(file_name) {
                Ok(_) => {}
                Err(error) => {
                    panic!("Couldn't create file {}: {}", &file_name, error);
                }
            };
            let buffer2 = buffer.clone();
            let file_name2 = file_name.to_string();
            // let mut logger2 = logger.clone();
            let logger = logger.clone();
            let receiver = c.1.clone();
            thread::spawn(move || {
                let mut stop = false;
                while !stop {
                    match receiver.recv() {
                        Ok(m) => {
                            match m {
                                LogMessage::NewLog(log_msg, asyn) => {
                                    Self::received_new_log(
                                        &mut logger.write().unwrap().as_mut().unwrap(),
                                        buffer2.clone(), log_msg,
                                        file_name2.clone(), asyn);
                                }
                            }
                        }
                        Err(_) => { stop = true; }
                    }
                }
            });

        }

        Self {
            // buffer,
            // logger,
            asyn,
            logger: logger.clone(),
            tx: c.0,
        }
    }

    pub fn log(&mut self, log_msg: LogMsg) {
        if self.asyn {
            self.tx.send(LogMessage::NewLog(log_msg, self.asyn)).unwrap();
        } else {
            let mut guard = self.logger.write().unwrap();
            let logger = guard.as_mut().unwrap();
            let strings = logger.log(log_msg, LABELS.get().unwrap(),
                OPERANDS.get().unwrap());
            for s in strings {
                info!(target: "asm", "{s}");
            }
        }
    }

    fn received_new_log(logger: &mut Box<dyn IExternalLogger>,
        buffer: Arc<RwLock<Vec<String>>>, log_msg: LogMsg, file_name: String, asyn: bool)
    {
        let strings = logger.log(log_msg, LABELS.get().unwrap(),
            OPERANDS.get().unwrap());
        if asyn {
            for s in strings {
                buffer.write().unwrap().push(s);
            }
            let b = buffer.read().unwrap();
            if b.len() > 100_000 {
                match fs::OpenOptions::new().append(true).open(&file_name) {
                    Ok(mut file) => {
                        file.write_all(b.join("\n").as_bytes())
                            .expect("Can write to file");
                        file.write_all(NL).unwrap();
                    }
                    Err(error) => {
                        panic!("Couldn't append to file {}: {}", file_name, error);
                    }
                }
                buffer.write().unwrap().clear();
            }
        } else {
            for s in strings {
                info!(target: "asm", "{s}");
            }
        }
    }

}

