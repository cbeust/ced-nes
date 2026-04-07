use tokio::sync::broadcast::{Receiver, Sender};
use std::time::SystemTime;
use tracing::info;
use crate::config::Config;
use crate::constants::OPERANDS_6502;
use crate::disassembly::{Disassemble, RunDisassemblyLine};
use crate::labels::Labels;
use crate::messages::{LogMsg, ToCpuUi, ToLogging};

pub struct Logging {
    pub receiver: Receiver<ToLogging>,
    sender: Option<Sender<ToCpuUi>>,
    pub config: Config,
    active: bool,
    last_received_message: SystemTime,
    labels: Labels,
}

impl Logging {
    pub fn new(config: Config, receiver: Receiver<ToLogging>, sender: Option<Sender<ToCpuUi>>,
        labels: Labels)
            -> Self {
        Self {
            receiver, config,
            sender,
            active: false,
            labels,
            last_received_message: SystemTime::now(),
        }
    }

    pub fn run(&mut self) {
        let mut run = true;
        while run {
            if let Ok(message) = self.receiver.try_recv() {
                self.last_received_message = SystemTime::now();
                if ! self.active {
                    if let Some(sender) = &self.sender {
                        let _ = sender.send(ToCpuUi::LogStarted);
                    }
                }
                self.active = true;
                match message {
                    ToLogging::Log(log_msg) => {
                        self.log(log_msg, &self.labels);
                    }
                    ToLogging::End => {
                        run = true;
                    }
                    ToLogging::Exit => {
                        run = false;
                    }
                }
            }
            if self.active {
                if let Ok(t) = self.last_received_message.elapsed() {
                    if t.as_millis() > 10 {
                        if let Some(sender) = &self.sender {
                            sender.send(ToCpuUi::LogEnded).unwrap();
                        }
                        self.active = false;
                    }
                }
            }
        }
        println!("Logging thread exiting");
    }

    #[allow(unused_variables)]
    fn log(&self, LogMsg { global_cycles, instruction_cycles, pc, operand, byte1, byte2, is_indexed,
        memory_content, resolved_address, resolved_value, resolved_read, a, x, y, p, s }: LogMsg,
        labels: &Labels)
    {
        let operands = OPERANDS_6502;

        let disassembly_line = Disassemble::disassemble2(&operands, pc,
            &operand, byte1, byte2, labels);

        let d = RunDisassemblyLine::new(global_cycles, disassembly_line,
            resolved_address, resolved_value, resolved_read, operand.size,
            a, x, y, p, s);
        let stack: Vec<u16> = Vec::new(); // self.format_stack();
        // println!("{} {} {}", d.to_asm(), self.p, stack);
        // println!("{}", d.to_csv());

        if self.config.trace_to_file.is_some() && self.config.csv {
            info!("{}", d.to_csv());
        } else {
            for log in d.to_log(labels) {
                info!("{} {} {:?}", log, p, stack);
            }
        }
    }
}