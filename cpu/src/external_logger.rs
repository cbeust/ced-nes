use crate::disassembly::{Disassemble, RunDisassemblyLine};
use crate::labels::Labels;
use crate::messages::LogMsg;
use crate::operand::Operand;

pub trait IExternalLogger: Send  + Sync {
    fn log(&mut self, log_msg: LogMsg, labels: &Labels, operands: &[Operand; 256]) -> Vec<String>;
}

#[derive(Clone, Copy, Default)]
pub struct NoExternalLogger;

impl IExternalLogger for NoExternalLogger {
    fn log(&mut self, _log_msg: LogMsg, _labels: &Labels, _operands: &[Operand; 256])
        -> Vec<String>
    {
        Vec::new()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultLogger;

impl IExternalLogger for DefaultLogger {
    fn log(&mut self, log_msg: LogMsg, labels: &Labels, operands: &[Operand; 256]) -> Vec<String> {
        let pc = log_msg.pc;
        let operand = log_msg.operand;
        let disassembly_line = Disassemble::disassemble2(operands, pc,
            &operand,
            log_msg.byte1, log_msg.byte2,
            labels
        );
        let cycles = log_msg.global_cycles;
        let d = RunDisassemblyLine::new(cycles, disassembly_line,
            log_msg.resolved_address, log_msg.resolved_value, log_msg.resolved_read,
            operand.cycles,
            log_msg.a, log_msg.x, log_msg.y, log_msg.p, log_msg.s);
        // let stack = self.format_stack();
        // println!("{} {} {}", d.to_asm(), self.p, stack);
        // println!("{}", d.to_csv());
        // if config.csv {
        //     info!("{}", d.to_csv());
        // } else {
            d.to_log(labels)
        // }
    }
}