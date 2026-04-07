use crate::ppu::{CURRENT_CYCLE, CURRENT_SCANLINE, SCANLINES};
use cpu::external_logger::IExternalLogger;
use cpu::labels::Labels;
use cpu::messages::LogMsg;
use cpu::operand::Operand;
use once_cell::sync::Lazy;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::RwLock;
use crate::emulator::CYCLES;

pub struct MesenLogger {
    count: u32,
    file: File,
}

pub static LOG_CYCLE: Lazy<RwLock<u16>> = Lazy::new(|| RwLock::new(0));
pub static LOG_SCANLINE: Lazy<RwLock<u16>> = Lazy::new(|| RwLock::new(0));

impl Default for MesenLogger {
    fn default() -> Self {
        let file_name = [dirs::home_dir().unwrap().to_str().unwrap(),
            "t", "trace-ced-mesen.txt"
        ]
            .iter().fold(PathBuf::new(), |mut path, segment| {
            path.push(segment);
            path
        });
        let file = File::create(file_name).expect("File created");
        Self {
            count: 0,
            file,
        }
    }
}

impl IExternalLogger for MesenLogger {
    fn log(&mut self, log: LogMsg, labels: &Labels, _operands: &[Operand; 256]) -> Vec<String> {
        let mut line = String::new();
        let byte = log.byte1;
        let word = log.byte1 as u16 | (log.byte2 as u16) << 8;
        let dis = match log.operand.size {
            3 => {
                &format!("{} {}", log.operand.name,
                    log.operand.addressing_type.to_string(log.pc, byte, word, labels))
            }
            2 => {
                &format!("{} {}", log.operand.name,
                    log.operand.addressing_type.to_string(log.pc, byte, word, labels))
            }
            _ => { log.operand.name }
        };
        let maybe_value = match log.memory_content {
            None => { "".into() }
            Some(v) => {
                if log.is_indexed {
                    format!(" [${:04X}] = ${v:02X}", log.resolved_address.unwrap())
                } else {
                    format!(" = ${v:02X}")
                }
            }
        };
        let flag_n = if log.p & 0b1000_0000 != 0 { "N" } else { "n" };
        let flag_v = if log.p & 0b0100_0000 != 0 { "V" } else { "v" };
        let flag_d = if log.p & 0b0000_1000 != 0 { "D" } else { "d" };
        let flag_i = "I";
        let flag_z = if log.p & 0b0000_0010 != 0 { "Z" } else { "z" };
        let flag_c = if log.p & 0b0000_0001 != 0 { "C" } else { "c" };
        let flags = format!("{flag_n}{flag_v}--{flag_d}{flag_i}{flag_z}{flag_c}");
        let field2 = format!("{dis}{maybe_value}");
        let registers = format!("A:{:02X} X:{:02X} Y:{:02X} S:{:02X} P:{flags}",
            log.a, log.x, log.y, log.s);

        let cs = *LOG_SCANLINE.read().unwrap();
        let v = if cs == 261 { -1 } else { cs as i16 };
        let beam = format!("V:{v:<3} H:{:<3}", *LOG_CYCLE.read().unwrap());

        self.count += 1;

        //
        // Put it all together
        //
        let cycles = log.global_cycles;
        line.push_str(&format!("{:04X}  {field2:32} {registers} {beam} Cycle:{cycles}",
            log.pc));
        // Append the line to self.file
        vec![line]
        // if let Err(e) = writeln!(self.file, "{}", line) {
        //     eprintln!("Failed to write log line: {}", e);
        // }
        // println!("{line}");
    }
}
