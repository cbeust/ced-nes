use std::collections::HashSet;
use std::process::exit;
use std::sync::{Arc, RwLock};
use tracing::{debug, info};
use crate::config::Config;
use crate::constants;
use crate::constants::*;
use crate::cpu::{StatusFlags, LOG_ASYNC};
use crate::external_logger::{IExternalLogger};
use crate::log_file::LogFile;
use crate::memory::Memory;
use crate::messages::LogMsg;

pub const CPU2_DEBUG: bool = false;
pub const DEBUG2_ASM: bool = false;

const STACK_ADDRESS: u16 = 0x100;

pub struct Cpu2Memory {
    ram: [u8; 0x10000],
    pub cycles: Vec<(u16, u8, String)>,
}

impl Default for Cpu2Memory {
    fn default() -> Self {
        Cpu2Memory {
            ram: [0; 0x10000],
            cycles: Vec::new(),
        }
    }
}

impl Memory for Cpu2Memory {
    fn get(&mut self, address: u16) -> u8 {
        self.cycles.push((address, self.ram[address as usize], "read".to_string()));
        self.ram[address as usize]
    }

    fn set(&mut self, address: u16, value: u8) {
        self.cycles.push((address, value, "write".to_string()));
        self.ram[address as usize] = value;
    }

    fn set_force(&mut self, address: u16, value: u8) {
        self.ram[address as usize] = value;
    }

    fn get_direct(&mut self, address: u16) -> u8 {
        self.ram[address as usize]
    }

    fn main_memory(&mut self) -> Vec<u8> {
        todo!()
    }
}

#[derive(Default)]
struct LogInfo {
    a: u8, x: u8, y: u8, s: u8, pc: u16, p: u8
}

/// https://www.nesdev.org/6502_cpu.txt
pub struct Cpu2<T: Memory> {
    pub memory: T,
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub _pc: u16,
    pub s: u8,
    pub p: StatusFlags,
    /// 0...7
    current_cycle: usize,
    pub cycles: u128,
    no_increment: bool,
    low_byte: usize,
    pointer: u16,
    finished: bool,
    current_opcode: u8,
    current_value: u8,
    current_address: u16,
    page_crossed: bool,
    log_info: LogInfo,
    // Number of cycles in the current instruction. Only accurate when finished = true
    instruction_cycles: u8,

    // (low, high)
    pending_interrupt: Option<(u16, u16)>,
    pc_was_changed: bool,
    pub log_file: LogFile,
}

impl<T: Memory> Cpu2<T> {
    pub fn pc(&self) -> u16 {
        self._pc
    }

    pub fn set_pc(&mut self, value: u16) {
        // if value < 0x100 {
        //     info!("SETTING PC TOO LOW: {value:04X}");
        // }
        self._pc = value;
    }

    fn inc_pc(&mut self) {
        let v = self._pc.wrapping_add(1);
        self.set_pc(v);
    }
}

impl<T: Memory> Cpu2<T> {
    pub fn new(memory: T, config: &Config,
               logger: Option<Box<dyn IExternalLogger>>) -> Cpu2<T>
    {
        let log_file = LogFile::new(&config.trace_file_asm, Arc::new(RwLock::new(logger)),
                                    LOG_ASYNC, &config.labels,
                                    OPERANDS_6502.clone());

        Cpu2 {
            memory,
            log_file,
            a: 0,
            x: 0,
            y: 0,
            _pc: 0,
            s: 0,
            p: StatusFlags::new(),

            current_cycle: 1,
            cycles: 8,
            no_increment: false,
            low_byte: 0,
            pointer: 0,
            finished: true,
            current_opcode: 0,
            current_value: 0,
            current_address: 0,
            page_crossed: false,
            pending_interrupt: None,
            log_info: LogInfo::default(),
            instruction_cycles: 0,
            pc_was_changed: false,
        }
    }

    pub fn one_cycle(&mut self, _config: &mut Config, _breakpoints: &HashSet<u16>)
        -> (bool, u8)
    {
        let mut result = (self.finished, self.tick());
        while result.1 == 0 {
            result = (self.finished, self.tick());
        }
        // println!("CPU TICK");
        // info!(target: "asm", "CPU TICK info");
        // debug!(target: "asm", "CPU TICK debug");
        result
    }

    pub fn run_one_instruction(&mut self) -> u8 {
        let mut result = 0;
        loop {
            self.tick();
            if ! self.no_increment { result += 1; }
            if self.finished {
                break;
            }
        }
        result
    }

    /// Return the number of cycles executed
    pub fn tick(&mut self) -> u8 {
        let mut result: u8 = 1;
        use constants::*;
        // Save the registers for logging

        if self.finished {
            if let Some((low, high)) = self.pending_interrupt {
                self.handle_interrupt_full(false, low, high);
                self.pending_interrupt = None;
                info!("PC after interrupt:{:04X}", self.pc());
            }

            self.instruction_cycles = 0;
            self.pc_was_changed = false;
            self.current_opcode = self.memory.get(self.pc());
            self.log_info = LogInfo {
                a: self.a,
                x: self.x,
                y: self.y,
                s: self.s,
                pc: self.pc(),
                p: self.p.value(),
            };

            self.inc_pc();
            self.current_cycle = 1;
            self.no_increment = false;
            self.finished = false;
        }
        let op = self.current_opcode;

        match op {
            CLD | CLC | CLV | CLI | SED | SEC | SEI => {
                match self.current_cycle {
                    2 => {
                        let _ = self.memory.get(self.pc()) as usize;
                        match op {
                            CLD => { self.p.set_d(false); }
                            CLC => { self.p.set_c(false); }
                            CLI => { self.p.set_i(false); }
                            CLV => { self.p.set_v(false); }
                            SED => { self.p.set_d(true); }
                            SEC => { self.p.set_c(true); }
                            SEI => { self.p.set_i(true); }
                            _ => { panic!("Should never happen"); }
                        }
                        self.finished = true;
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }
            NOP => {
                match self.current_cycle {
                    2 => {
                        let _ = self.memory.get(self.pc()) as usize;
                        self.finished = true;
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            BRK => {
                // info!("Found BRK at pc:{:04X}", self.pc());
                self.handle_interrupt(true, 0xfffe, 0xffff);
            }

            RTI => {
                match self.current_cycle {
                    2 => {
                        //         2    PC     R  read next instruction byte (and throw it away)
                        let _ = self.memory.get(self.pc());
                    }
                    3 => {
                        //         3  $0100,S  R  increment S
                        let _ = self.memory.get(STACK_ADDRESS + self.s as u16);
                    }
                    4 => {
                        //         4  $0100,S  R  pull P from stack, increment S
                        let p = self.pop_stack();
                        self.p.set_value(p);
                    }
                    5 => {
                        //         5  $0100,S  R  pull PCL from stack, increment S
                        let pcl = self.pop_stack() as u16;
                        self.set_pc((self.pc() & 0xff00) | pcl);
                    } 6 => {
                        //         6  $0100,S  R  pull PCH from stack
                        let pch = self.pop_stack() as u16;
                        self.set_pc((self.pc() & 0xff) | (pch << 8));
                        debug!(target: "cpu", "RTI 6 pc={:04X}", self.pc());
                        self.finished = true;
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            RTS => {
                match self.current_cycle {
                    2 => {
                        //         2    PC     R  read next instruction byte (and throw it away)
                        let _ = self.memory.get(self.pc());
                    }
                    3 => {
                        //         3  $0100,S  R  increment S
                        let _ = self.memory.get(STACK_ADDRESS + self.s as u16);
                    }
                    4 => {
                        //         4  $0100,S  R  pull PCL from stack, increment S
                        let pcl = self.pop_stack() as u16;
                        self.set_pc((self.pc() & 0xff00) | pcl);
                    }
                    5 => {
                        //         5  $0100,S  R  pull PCH from stack
                        let pch = self.pop_stack() as u16;
                        self.set_pc((self.pc() & 0xff) | (pch << 8));
                    }
                    6 => {
                        //         6    PC     R  increment PC
                        let _ = self.memory.get(self.pc());
                        self.set_pc(self.pc().wrapping_add(1));
                        self.finished = true;
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            PHA | PHP => {
                match self.current_cycle {
                    2 => {
                        //         2    PC     R  read next instruction byte (and throw it away)
                        let _ = self.memory.get(self.pc());
                    }
                    3 => {
                        //         3  $0100,S  W  push register on stack, decrement S
                        let value = if op == PHA {
                            self.a
                        } else if op == PHP {
                            self.p.set_b(true);
                            self.p.value()
                        } else {
                            panic!("Should never happen");
                        };
                        self.push_to_stack(value);
                        self.finished = true;
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            PLA | PLP => {
                match self.current_cycle {
                    2 => {
                        //         2    PC     R  read next instruction byte (and throw it away)
                        let _ = self.memory.get(self.pc());
                    }
                    3 => {
                        //         3  $0100,S  R  increment S
                        let _ = self.memory.get(STACK_ADDRESS + self.s as u16);
                    }
                    4 => {
                        //         4  $0100,S  R  pull register from stack
                        if op == PLA {
                            self.a = self.pop_stack();
                            self.set_nz(self.a);
                        } else if op == PLP {
                            self.p.set_b(false);
                            let value = self.pop_stack();
                            self.p.set_value(value);
                        } else {
                            panic!("Should never happen");
                        };
                        self.finished = true;
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            //
            // Absolute Indirect
            //
            JMP_IND => {
                match self.current_cycle {
                    2 => {
                        //         2     PC      R  fetch pointer address low, increment PC
                        self.low_byte = self.memory.get(self.pc()) as usize;
                        debug!(target: "cpu", "2 JMP_IND low_byte:{:02X}", self.low_byte);
                        self.inc_pc();
                    }
                    3 => {
                        //         3     PC      R  fetch pointer address high, increment PC
                        let high = self.memory.get(self.pc()) as u16;
                        self.current_address = (high << 8) | self.low_byte as u16;
                        debug!(target: "cpu", "3 JMP_IND high_byte:{:02X} address:{:04X}",
                            self.low_byte, self.current_address);
                        self.inc_pc();
                    }
                    4 => {
                        //         4   pointer   R  fetch low address to latch
                        self.low_byte = self.memory.get(self.current_address) as usize;
                        debug!(target: "cpu", "4 JMP_IND low_byte:{:02X}", self.low_byte);
                    }
                    5 => {
                        //         5  pointer+1* R  fetch PCH, copy latch to PCL
                        let ah = self.current_address & 0xff00;
                        let al = self.current_address.wrapping_add(1) & 0xff;
                        let a = ah | al;
                        let high = self.memory.get(a) as u16;
                        self.set_pc((high << 8) | self.low_byte as u16);
                        debug!(target: "cpu", "5 JMP_IND high:{high:02X} a:{a:04X} pc:{:04X}", self.pc());
                        self.finished = true;
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            JMP => {
                match self.current_cycle {
                    2 => {
                        self.low_byte = self.memory.get(self.pc()) as usize;
                        self.inc_pc();
                    }
                    3 => {
                        let high = (self.memory.get(self.pc()) as u16) << 8;
                        self.set_pc(high | self.low_byte as u16);
                        self.finished = true;
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            JSR => {
                match self.current_cycle {
                    2 => {
                        //         2    PC     R  fetch low address byte, increment PC
                        self.low_byte = self.memory.get(self.pc()) as usize;
                        debug!(target: "cpu", "2 low_byte:{:02X}", self.low_byte);
                        self.inc_pc();
                    }
                    3 => {
                        //         3  $0100,S  R  internal operation (predecrement S?)
                        let _ = self.memory.get(STACK_ADDRESS.wrapping_add(self.s as u16));
                    }
                    4 => {
                        //         4  $0100,S  W  push PCH on stack, decrement S
                        self.push_to_stack(((self.pc() & 0xff00) >> 8) as u8);
                    }
                    5 => {
                        //         5  $0100,S  W  push PCL on stack, decrement S
                        self.push_to_stack(self.pc() as u8);
                    }
                    6 => {
                        //         6    PC     R  copy low address byte to PCL, fetch high address
                        //                        byte to PCH
                        let high_byte = self.memory.get(self.pc()) as usize;
                        self.set_pc((high_byte as u16) << 8 | self.low_byte as u16);
                        debug!(target: "cpu", "6 pc:{:04X} high_byte: {:04X}", self.pc(), high_byte);
                        self.finished = true;
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            TXS | TSX | DEX | DEY | INX | INY | TXA | TAX | TYA | TAY => {
                match self.current_cycle {
                    2 => {
                        self.current_value = self.memory.get(self.pc());
                        match op {
                            TSX => {
                                self.x = self.s;
                                self.set_nz(self.x)
                            }
                            TXS => {
                                self.s = self.x;
                            }
                            TAX => {
                                self.x = self.a;
                                self.set_nz(self.x)
                            }
                            TXA => {
                                self.a = self.x;
                                self.set_nz(self.a)
                            }
                            TYA => {
                                self.a = self.y;
                                self.set_nz(self.a)
                            }
                            TAY => {
                                self.y = self.a;
                                self.set_nz(self.y)
                            }
                            DEX => {
                                self.x = self.x.wrapping_sub(1);
                                self.set_nz(self.x)
                            }
                            DEY => {
                                self.y = self.y.wrapping_sub(1);
                                self.set_nz(self.y)
                            }
                            INX => {
                                self.x = self.x.wrapping_add(1);
                                self.set_nz(self.x)
                            }
                            INY => {
                                self.y = self.y.wrapping_add(1);
                                self.set_nz(self.y)
                            }
                            _ => {}
                        }
                        self.finished = true;
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            //
            // Implied
            //
            LSR | ROL | ROR | ASL => {
                match self.current_cycle {
                    2 => {
                        let _ = self.memory.get(self.pc());
                        self.a = match op {
                            ASL => {
                                self.a = self.asl(self.a);
                                self.a
                            }
                            LSR => {
                                let v = self.a;
                                let bit0 = v & 1;
                                self.p.set_c(bit0 != 0);
                                let result = v >> 1;
                                result
                            }
                            ROL => {
                                let v = self.a;
                                let result = (v << 1) | self.p.c() as u8;
                                self.p.set_c(v & 0x80 != 0);
                                result
                            }
                            ROR => {
                                let v = self.a;
                                let bit0 = v & 1;
                                let result = (v >> 1) | (self.p.c() as u8) << 7;
                                self.p.set_c(bit0 != 0);
                                result
                            }
                            _ => { panic!(""); }
                        };
                        self.set_nz(self.a);
                        self.finished = true;
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            CPY_IMM | CPX_IMM => {
                match self.current_cycle {
                    2 => {
                        self.current_value = self.memory.get(self.pc());
                        self.inc_pc();
                        let value = match op {
                            CPY_IMM => { self.y }
                            CPX_IMM => { self.x }
                            _ => { panic!("CP"); }
                        };
                        let v = self.cmp(value, self.current_value);
                        self.set_nz(v);
                        self.finished = true;
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            //
            // Read instructions
            // Immediate
            //
            LDA_IMM | LDX_IMM | LDY_IMM | EOR_IMM | AND_IMM | ORA_IMM | ADC_IMM | SBC_IMM | CMP_IMM
            => {
                match self.current_cycle {
                    2 => {
                        self.current_value = self.memory.get(self.pc());
                        self.inc_pc();
                        self.read(false);
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            //
            // Read instructions
            // Absolute
            //
            LDA_ABS | LDX_ABS | LDY_ABS | EOR_ABS | AND_ABS | ORA_ABS | ADC_ABS | SBC_ABS | CMP_ABS
            | CPX_ABS | CPY_ABS | BIT_ABS => {
                match self.current_cycle {
                    2 => {
                        self.low_byte = self.memory.get(self.pc()) as usize;
                        self.inc_pc();
                    }
                    3 => {
                        let high_byte = self.memory.get(self.pc()) as usize;
                        self.inc_pc();
                        self.current_address = (high_byte as u16) << 8 | self.low_byte as u16;
                    }
                    4 => {
                        self.read(true);
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            //
            // Read Modify Write instructions
            // Absolute
            //
            ASL_ABS | LSR_ABS | ROL_ABS | ROR_ABS | INC_ABS | DEC_ABS => {
                match self.current_cycle {
                    2 => {
                        self.low_byte = self.memory.get(self.pc()) as usize;
                        self.inc_pc();
                    }
                    3 => {
                        let high_byte = self.memory.get(self.pc()) as usize;
                        self.current_address = (high_byte as u16) << 8 | self.low_byte as u16;
                        self.inc_pc();
                    }
                    4 => {
                        self.current_value = self.memory.get(self.current_address);
                    }
                    5 => {
                        self.memory.set(self.current_address, self.current_value);
                    }
                    6 => {
                        self.read_modify_write();
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            //
            // Write instructions
            // Absolute
            //
            STA_ABS | STX_ABS |STY_ABS => {
                match self.current_cycle {
                    2 => {
                        self.low_byte = self.memory.get(self.pc()) as usize;
                        self.inc_pc();
                    }
                    3 => {
                        let high_byte = self.memory.get(self.pc()) as usize;
                        self.inc_pc();
                        self.current_address = (high_byte as u16) << 8 | self.low_byte as u16;
                    }
                    4 => {
                        self.write();
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            //
            // Read
            // Zero Page
            //
            LDA_ZP | LDX_ZP | LDY_ZP | EOR_ZP | AND_ZP | ORA_ZP | ADC_ZP | SBC_ZP | CMP_ZP | BIT_ZP
                | CPX_ZP | CPY_ZP => {
                match self.current_cycle {
                    2 => {
                        self.current_address = self.memory.get(self.pc()) as u16;
                        self.inc_pc();
                        debug!(target: "cpu", "1 low_byte:{:02X} pc:{:04X}", self.low_byte, self.pc());
                    }
                    3 => {
                        self.read(true);
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            //
            // Read Modify Write instructions
            // Zero page
            //
            ASL_ZP | LSR_ZP | ROL_ZP | ROR_ZP | INC_ZP | DEC_ZP => {
                match self.current_cycle {
                    2 => {
                        //         2    PC     R  fetch address, increment PC
                        self.current_address = self.memory.get(self.pc()) as u16;
                        self.inc_pc();
                        debug!(target: "cpu", "1 low_byte:{:02X} pc:{:04X}", self.low_byte, self.pc());
                    }
                    3 => {
                        //         3  address  R  read from effective address
                        self.current_value = self.memory.get(self.current_address);
                    }
                    4 => {
                        //         4  address  W  write the value back to effective address,
                        //                        and do the operation on it
                        self.memory.set(self.current_address, self.current_value);
                    }
                    5 => {
                        //         5  address  W  write the new value to effective address
                        self.read_modify_write();
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            //
            // Write instructions
            // Zero page
            //
            STA_ZP | STX_ZP | STY_ZP => {
                match self.current_cycle {
                    2 => {
                        //         2    PC     R  fetch address, increment PC
                        self.current_address = self.memory.get(self.pc()) as u16;
                        self.inc_pc();
                    }
                    3 => {
                        //         3  address  W  write register to effective address
                        self.write();
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            //
            // Read
            // Zero page indexed addressing
            //
            LDA_ZP_X | LDY_ZP_X | EOR_ZP_X | AND_ZP_X | ORA_ZP_X | ADC_ZP_X | SBC_ZP_X | CMP_ZP_X
                | LDX_ZP_Y => {
                match self.current_cycle {
                    2 => {
                        //        2     PC      R  fetch address, increment PC
                        self.current_address = self.memory.get(self.pc()) as u16;
                        debug!(target: "cpu", "2 address:{:04X}", self.current_address);
                        self.inc_pc();
                    }
                    3 => {
                        //         3   address   R  read from address, add index register to it
                        let _ = self.memory.get(self.current_address);
                        let increment = if op == LDX_ZP_Y { self.y } else { self.x };
                        self.current_address = (self.current_address + increment as u16) & 0xff;
                        debug!(target: "cpu", "3 address:{:04X}", self.current_address);
                    }
                    4 => {
                        //         4  address+I* R  read from effective address
                        self.read(true);
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            //
            // Read Modify Write instructions
            // Zero page indexed addressing
            //
            ASL_ZP_X | LSR_ZP_X | ROL_ZP_X | ROR_ZP_X | INC_ZP_X | DEC_ZP_X => {
                match self.current_cycle {
                    2 => {
                        //         2     PC      R  fetch address, increment PC
                        self.current_address = self.memory.get(self.pc()) as u16;
                        self.inc_pc();
                    }
                    3 => {
                        //         3   address   R  read from address, add index register X to it
                        let _ = self.memory.get(self.current_address);
                        self.current_address = (self.current_address + self.x as u16) & 0xff;
                    }
                    4 => {
                        //         4  address+X* R  read from effective address
                        self.current_value = self.memory.get(self.current_address);
                    }
                    5 => {
                        //         5  address+X* W  write the value back to effective address,
                        //                          and do the operation on it
                        self.memory.set(self.current_address, self.current_value);
                    }
                    6 => {
                        self.read_modify_write();
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            //
            // Write instructions
            // Zero page indexed addressing
            //
            STA_ZP_X | STY_ZP_X | STX_ZP_Y => {
                match self.current_cycle {
                    2 => {
                        //         2     PC      R  fetch address, increment PC
                        self.current_address = self.memory.get(self.pc()) as u16;
                        self.inc_pc();
                    }
                    3 => {
                        //         3   address   R  read from address, add index register to it
                        let _ = self.memory.get(self.current_address);
                        let increment = if op == STX_ZP_Y { self.y } else { self.x };
                        self.current_address = (self.current_address + increment as u16) & 0xff;
                    }
                    4 => {
                        //         4  address+I* W  write to effective address
                        self.write();
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            //
            // Read
            // Indexed indirect (X)
            //
            LDA_IND_X | ORA_IND_X | EOR_IND_X | AND_IND_X | ADC_IND_X | CMP_IND_X | SBC_IND_X => {
                match self.current_cycle {
                    2 => {
                        self.low_byte = self.memory.get(self.pc()) as usize;
                        self.inc_pc();
                        debug!(target: "cpu", "1 low_byte:{:02X} pc:{:04X}", self.low_byte, self.pc());
                    }
                    3 => {
                        let _ = self.memory.get(self.low_byte as u16);
                        self.current_address = (self.low_byte as u16 + self.x as u16) & 0xff;
                        debug!(target: "cpu", "2 mem at low_byte:{:02X} X:{:02X} address:{:04X}",
                            self.memory.get(self.low_byte as u16),
                            self.x, self.current_address);
                    }
                    4 => {
                        self.low_byte = self.memory.get(self.current_address) as usize;
                        debug!(target: "cpu", "3 low_byte:{:02X}", self.low_byte);
                    }
                    5 => {
                        let high_byte = self.memory.get(self.current_address.wrapping_add(1) & 0xff)
                            as usize;
                        self.current_address = (high_byte as u16) << 8 | self.low_byte as u16;
                        debug!(target: "cpu", "4 high_byte:{:02X}", high_byte);
                    }
                    6 => {
                        self.read(true);
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            //
            // Write
            // Indexed Indirect (X)
            //
            STA_IND_X => {
                match self.current_cycle {
                    2 => {
                        //         2      PC       R  fetch pointer address, increment PC
                        self.pointer = self.memory.get(self.pc()) as u16;
                        debug!(target: "cpu", "2 pointer:{:02X}", self.pointer);
                        self.inc_pc();
                    }
                    3 => {
                        //         3    pointer    R  read from the address, add X to it
                        let _ = self.memory.get(self.pointer);
                        self.current_address = (self.pointer + self.x as u16) & 0xff;
                        debug!(target: "cpu", "3 address:{:04X}", self.current_address);
                    }
                    4 => {
                        //         4   pointer+X   R  fetch effective address low
                        self.low_byte = self.memory.get(self.current_address) as usize;
                        debug!(target: "cpu", "4 low_byte:{:02X}", self.low_byte);
                    }
                    5 => {
                        //         5  pointer+X+1  R  fetch effective address high
                        let high_byte = self.memory.get((self.current_address + 1) & 0xff) as usize;
                        self.current_address = (high_byte as u16) << 8 | self.low_byte as u16;
                        debug!(target: "cpu", "5 high_byte:{:02X}", high_byte);                }
                    6 => {
                        //         6    address    W  write to effective address
                        debug!(target: "cpu", "6 address:{:05X}", self.current_address);
                        self.write();
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            //
            // Read
            // Absolute indexed addressing
            //
            LDA_ABS_X | LDY_ABS_X | EOR_ABS_X | AND_ABS_X | ORA_ABS_X | ADC_ABS_X | SBC_ABS_X
                | CMP_ABS_X | LDA_ABS_Y | LDX_ABS_Y | EOR_ABS_Y | AND_ABS_Y | ORA_ABS_Y
                | ADC_ABS_Y | SBC_ABS_Y | CMP_ABS_Y => {
                match self.current_cycle {
                    2 => {
                        //         2     PC      R  fetch low byte of address, increment PC
                        self.low_byte = self.memory.get(self.pc()) as usize;
                        self.inc_pc();
                    }
                    3 => {
                        //         3     PC      R  fetch high byte of address,
                        //                          add index register to low address byte,
                        //                          increment PC
                        let high_byte = self.memory.get(self.pc()) as usize;
                        let increment = match op {
                            LDA_ABS_X | LDY_ABS_X | EOR_ABS_X | AND_ABS_X | ORA_ABS_X | ADC_ABS_X |
                                SBC_ABS_X | CMP_ABS_X => { self.x as u16 }
                            LDA_ABS_Y | LDX_ABS_Y | EOR_ABS_Y | AND_ABS_Y | ORA_ABS_Y | ADC_ABS_Y |
                            SBC_ABS_Y | CMP_ABS_Y => { self.y as u16 }
                            _ => { panic!("Should never happen"); }
                        };
                        let old = (high_byte as u16) << 8 | self.low_byte as u16;
                        // let low_byte = self.low_byte as u16 + increment;
                        let new = old.wrapping_add(increment);
                        self.inc_pc();
                        self.page_crossed = ((old ^ new) & 0xff00) > 0;
                        if self.page_crossed {
                            self.current_address = new.wrapping_sub(0x100);
                        } else {
                            self.current_address = new;
                        }
                    }
                    4 => {
                        //         4  address+I* R  read from effective address,
                        //                          fix the high byte of effective address
                        self.current_value = self.memory.get(self.current_address);
                    }
                    5 => {
                        //         5+ address+I  R  re-read from effective address
                        if self.page_crossed {
                            self.current_address = self.current_address.wrapping_add(0x100);
                        } else {
                            self.no_increment = true;
                        }
                        self.read(self.page_crossed);
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            //
            // Read modify write
            // Absolute indexed addressing
            //
            ASL_ABS_X | LSR_ABS_X | ROL_ABS_X | ROR_ABS_X | INC_ABS_X | DEC_ABS_X => {
                match self.current_cycle {
                    2 => {
                        //         2    PC       R  fetch low byte of address, increment PC
                        self.low_byte = self.memory.get(self.pc()) as usize;
                        self.inc_pc();
                    }
                    3 => {
                        //         3    PC       R  fetch high byte of address,
                        //                          add index register X to low address byte,
                        //                          increment PC
                        let high_byte = self.memory.get(self.pc()) as usize;

                        let old = (high_byte as u16) << 8 | self.low_byte as u16;
                        self.low_byte = (self.low_byte + self.x as usize) & 0xff;
                        let new = old + self.x as u16;
                        self.page_crossed = ((old ^ new) & 0xff00) > 0;
                        if self.page_crossed {
                            self.current_address = new.wrapping_sub(0x100);
                        } else {
                            self.current_address = new;
                        }
                        self.inc_pc();
                    }
                    4 => {
                        //         4  address+X* R  read from effective address,
                        //                          fix the high byte of effective address
                        self.current_value = self.memory.get(self.current_address);
                        if self.page_crossed {
                            self.current_address = self.current_address.wrapping_add(0x100);
                        }
                    }
                    5 => {
                        //         5  address+X  R  re-read from effective address
                        self.current_value = self.memory.get(self.current_address);
                    }
                    6 => {
                        //         6  address+X  W  write the value back to effective address,
                        //                          and do the operation on it
                        self.memory.set(self.current_address, self.current_value);
                    }
                    7 => {
                        //         7  address+X  W  write the new value to effective address
                        self.read_modify_write();
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            //
            // Write instructions
            // Absolute Indexed addressing
            //
            STA_ABS_X | STA_ABS_Y => {
                match self.current_cycle {
                    2 => {
                        //         2    PC       R  fetch low byte of address, increment PC
                        self.low_byte = self.memory.get(self.pc()) as usize;
                        self.inc_pc();
                    }
                    3 => {
                        //         3    PC       R  fetch high byte of address,
                        //                          add index register to low address byte,
                        //                          increment PC
                        let high_byte = self.memory.get(self.pc()) as usize;

                        let old = (high_byte as u16) << 8 | self.low_byte as u16;
                        let increment = if op == STA_ABS_X { self.x } else if op == STA_ABS_Y { self.y }
                            else { panic!("WRONG OPCODE") } as usize;
                        self.low_byte = (self.low_byte + increment) & 0xff;
                        let new = old + increment as u16;
                        self.page_crossed = ((old ^ new) & 0xff00) > 0;
                        if self.page_crossed {
                            self.current_address = new.wrapping_sub(0x100);
                        } else {
                            self.current_address = new;
                        }
                        self.inc_pc();
                    }
                    4 => {
                        //         4  address+X* R  read from effective address,
                        //                          fix the high byte of effective address
                        self.current_value = self.memory.get(self.current_address);
                        if self.page_crossed {
                            self.current_address = self.current_address.wrapping_add(0x100);
                        }
                    }
                    5 => {
                        //         5  address+I  W  write to effective address
                        self.write();
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            //
            // Read
            // Indirect indexed (Y)
            //
            LDA_IND_Y | EOR_IND_Y | AND_IND_Y | ORA_IND_Y | ADC_IND_Y | SBC_IND_Y | CMP_IND_Y => {
                match self.current_cycle {
                    2 => {
                        //         2      PC       R  fetch pointer address, increment PC
                        self.pointer = self.memory.get(self.pc()) as u16;
                        debug!(target: "cpu", "2 pointer: {:04X}/{}", self.pointer, self.pointer);
                        self.inc_pc();
                    }
                    3 => {
                        //         3    pointer    R  fetch effective address low
                        self.low_byte = self.memory.get(self.pointer) as usize;
                        debug!(target: "cpu", "3 low_byte: {:02X}/{}", self.low_byte, self.low_byte);
                    }
                    4 => {
                        //         4   pointer+1   R  fetch effective address high,
                        //                            add Y to low byte of effective address
                        let high_byte = self.memory.get((self.pointer + 1) & 0xff) as usize;
                        let old = (high_byte << 8) | self.low_byte;
                        self.low_byte = (self.low_byte + self.y as usize) & 0xff;
                        let new = old + self.y as usize;
                        self.page_crossed = ((old ^ new) & 0xff00) > 0;
                        if self.page_crossed {
                            self.current_address = (new as u16).wrapping_sub(0x100);
                        } else {
                            self.current_address = new as u16;
                        }
                        debug!(target: "cpu", "4 high_byte: {:02X}/{}", high_byte, high_byte);
                    }
                    5 => {
                        //         5   address+Y*  R  read from effective address,
                        //                            fix high byte of effective address
                        self.current_value = self.memory.get(self.current_address);
                        debug!(target: "cpu", "5 current_value:{:02X}", self.current_value);
                    }
                    6 => {
                        if self.page_crossed {
                            self.current_value =
                                self.memory.get(self.current_address.wrapping_add(0x100));
                        } else {
                            self.no_increment = true;
                        }
                        debug!(target: "cpu", "6 current_value:{:02X}", self.current_value);
                        self.read(false);
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            // Write
            // Indirect indexed (Y)
            //
            STA_IND_Y => {
                match self.current_cycle {
                    2 => {
                        //         2      PC       R  fetch pointer address, increment PC
                        self.pointer = self.memory.get(self.pc()) as u16;
                        debug!(target: "cpu", "2 pointer: {:04X}/{}", self.pointer, self.pointer);
                        self.inc_pc();
                    }
                    3 => {
                        //         3    pointer    R  fetch effective address low
                        self.low_byte = self.memory.get(self.pointer) as usize;
                        debug!(target: "cpu", "3 low_byte: {:02X}/{}", self.low_byte, self.low_byte);
                    }
                    4 => {
                        //         4   pointer+1   R  fetch effective address high,
                        //                            add Y to low byte of effective address
                        let high_byte = self.memory.get((self.pointer + 1) & 0xff) as usize;
                        let old = (high_byte << 8) | self.low_byte;
                        self.low_byte = (self.low_byte + self.y as usize) & 0xff;
                        let new = old + self.y as usize;
                        self.page_crossed = ((old ^ new) & 0xff00) > 0;
                        if self.page_crossed {
                            self.current_address = (new as u16).wrapping_sub(0x100);
                        } else {
                            self.current_address = new as u16;
                        }
                        debug!(target: "cpu", "4 high_byte: {:02X}/{}", high_byte, high_byte);
                    }
                    5 => {
                        //         5   address+Y*  R  read from effective address,
                        //                            fix high byte of effective address
                        self.current_value = self.memory.get(self.current_address);
                        debug!(target: "cpu", "5 current_value:{:02X} page_crossed:{}", self.current_value, self.page_crossed);
                    }
                    6 => {
                        //         6   address+Y   W  write to effective address
                        if self.page_crossed {
                            self.current_address = self.current_address.wrapping_add(0x100);
                        }
                        debug!(target: "cpu", "6 current_value:{:02X}", self.current_value);
                        self.write();
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }
            //
            // Relative addressing
            //
            BCC | BCS | BNE | BEQ | BPL | BMI | BVC | BVS => {
                match self.current_cycle {
                    2 => {
                        //         2     PC      R  fetch operand, increment PC
                        self.low_byte = self.memory.get(self.pc()) as usize;
                        debug!(target: "cpu", "2 low_byte: {:02X}", self.low_byte);
                        self.inc_pc();
                    }
                    3 => {
                        //         3     PC      R  Fetch opcode of next instruction,
                        //                          If branch is taken, add operand to PCL.
                        //                          Otherwise increment PC.
                        let branch_taken = match op {
                            BCC => { !self.p.c() }
                            BCS => { self.p.c() }
                            BNE => { !self.p.z() }
                            BEQ => { self.p.z() }
                            BPL => { !self.p.n() }
                            BMI => { self.p.n() }
                            BVC => { !self.p.v() }
                            BVS => { self.p.v() }
                            _ => panic!("Should never happen")
                        };
                        if branch_taken {
                            // println!("Branch taken for op:{} Z:{}", op, self.p.z());
                            self.current_address = (self.pc() as u16).wrapping_add(self.low_byte as u16);
                            let _opcode = self.memory.get(self.pc());
                            let old = self.pc();
                            let new = if self.low_byte < 0x80 {
                                self.pc().wrapping_add(self.low_byte as u16)
                            } else {
                                self.pc().wrapping_sub((256 - self.low_byte) as u16)
                            };
                            debug!(target: "cpu", "3 old:{old:04X} new:{new:04X}");
                            self.page_crossed = ((old ^ new) & 0xff00) > 0;
                            self.current_address = new;

                            let pcl = ((self.pc() & 0xff) + self.low_byte as u16) & 0xff;
                            self.set_pc((self.pc() & 0xff00) | pcl);
                            self.pc_was_changed = true;

                            debug!(target: "cpu", "3 branch taken: old:{old:04X} new:{new:04X} cross:{}",
                                self.page_crossed);

                        } else {
                            debug!(target: "cpu", "3 branch not taken: pc:{:04X}", self.pc());
                            self.page_crossed = false;
                            self.no_increment = true;
                        }
                    }
                    4 => {
                        if self.page_crossed {
                            let _opcode = self.memory.get(self.pc());
                            self.set_pc(self.current_address);
                            debug!(target: "cpu", "4 pc: {:04X}", self.pc());
                        } else {
                            self.no_increment = true;
                        }
                        self.finished = true;
                    }
                    _ => if self.current_cycle != 1 {
                        panic!("Cycle {} should not happen", self.current_cycle)
                    }
                }
            }

            _ => {
                println!("Unknown opcode {op:02X} at PC:{:04X}", self.pc());
                exit(1);
            }
        }

        if self.no_increment {
            result = 0;
        }

        self.instruction_cycles += result;

        if self.finished && DEBUG2_ASM {
            let resolved_address: Option<u16> = None;
            let resolved_value: Option<u8> = None;
            let resolved_read = true;
            let resolved_before_memory: Option<u8> = None;
            let is_indexed = false;

            let pc = self.log_info.pc;
            let byte1 = self.memory.get_direct(pc.wrapping_add(1));
            let byte2 = self.memory.get_direct(pc.wrapping_add(2));
            let operand = OPERANDS_6502[op as usize];

            // info!(target: "asm", "instruction cycles:{}", self.instruction_cycles);
            let log_msg = LogMsg::new(self.cycles, self.instruction_cycles, pc, operand.clone(),
                                      byte1, byte2, is_indexed, resolved_before_memory,
                                      resolved_address, resolved_value, resolved_read,
                                      self.log_info.a, self.log_info.x, self.log_info.y,
                                      self.log_info.p, self.log_info.s);
            self.cycles += self.instruction_cycles as u128;
            self.instruction_cycles = 0;
            self.log_file.log(log_msg);
            // let strings = self.logger.log(log_msg, &Labels::default(), &OPERANDS_6502);
            // for s in strings {
            //     info!(target: "asm", "{s}");
            //     // println!("{s}");
            // }
        }
        self.current_cycle += 1;

        result
    }

    fn set_nz(&mut self, value: u8) {
        self.p.set_z(value == 0);
        self.p.set_n(value & 0x80 != 0);
    }

    fn add(&mut self, v: u8) {
        let result: u16 = self.a as u16 + v as u16 + self.p.c() as u16;
        // NOTE: Parentheses are important here! Remove them and carry6 is incorrectly calculated
        let carry6 = (self.a & 0x7f) + (v & 0x7f) + self.p.c() as u8;
        self.p.set_c(result & 0x100 != 0);
        self.p.set_v(self.p.c() ^ (carry6 & 0x80 != 0));
        let result2 = result as u8;
        self.set_nz(result2);
        self.a = result2;
    }

    fn sbc(&mut self, v: u8) {
        // println!("SBC A={:02X} V={:02X}", self.a, v);

        if self.p.d() {
            let c = if self.p.c() as u8 == 0 { 1 } else { 0 };
            let diff: u16 = (self.a as u16).wrapping_sub(v as u16).wrapping_sub(c as u16);
            let mut al: u8 = (self.a & 0x0f).wrapping_sub(v & 0x0f).wrapping_sub(c);
            if al &0x80 != 0 {
                al = al.wrapping_sub(6);
            }
            let mut ah: u8 = (self.a >> 4).wrapping_sub(v >> 4)
                .wrapping_sub(if al & 0x80 != 0 {1} else {0});
            self.p.set_z((diff & 0xff) == 0);
            self.p.set_n(diff & 0x80 != 0);
            self.p.set_v((self.a ^ v) & (self.a ^ ((diff & 0xff) as u8)) & 0x80 != 0);
            self.p.set_c(diff & 0xff00 == 0);
            if ah & 0x80 != 0 {
                ah = ah.wrapping_sub(6);
            }
            self.a = (ah << 4) | (al & 0x0f);
        } else {
            self.add(v ^ 0xff);
        }
    }

    fn rol(&mut self, v: u8) -> u8 {
        let result = (v << 1) | self.p.c() as u8;
        self.p.set_c(v & 0x80 != 0);
        result
    }

    fn ror(&mut self, v: u8) -> u8 {
        let bit0 = v & 1;
        let result = (v >> 1) | (self.p.c() as u8) << 7;
        self.p.set_c(bit0 != 0);
        result
    }

    fn bit(&mut self, value: u8) -> u8 {
        self.p.set_z(value & self.a == 0);
        self.p.set_n(value & 0x80 != 0);
        self.p.set_v(value & 0x40 != 0);
        value & self.a
    }

    pub(crate) fn push_to_stack(&mut self, a: u8) {
        self.memory.set(STACK_ADDRESS.wrapping_add(self.s as u16), a);
        debug!(target: "cpu", "Pushed to stack [{:02X}] -> {a:02X}", self.s);
        self.s = self.s.wrapping_sub(1);
    }

    pub(crate) fn pop_stack(&mut self) -> u8 {
        self.s = self.s.wrapping_add(1);
        let result = self.memory.get(STACK_ADDRESS.wrapping_add(self.s as u16));
        debug!(target: "cpu", "Popped stack [{:02X}] -> {result:02X}", self.s);

        result
    }

    fn read_modify_write(&mut self) {
        let value = match self.current_opcode {
            ASL_ABS | ASL_ZP | ASL_ZP_X | ASL_ABS_X => {
                self.p.set_c(self.current_value & 0x80 != 0);
                self.current_value.wrapping_shl(1)
            }
            LSR_ABS | LSR_ZP | LSR_ZP_X | LSR_ABS_X => {
                self.p.set_c(self.current_value & 1 != 0);
                self.current_value >> 1
            }
            ROL_ABS | ROL_ZP | ROL_ZP_X | ROL_ABS_X => {
                self.rol(self.current_value)
            }
            ROR_ABS | ROR_ZP | ROR_ZP_X | ROR_ABS_X => {
                self.ror(self.current_value)
            }
            DEC_ABS | DEC_ZP | DEC_ZP_X | DEC_ABS_X => {
                self.current_value.wrapping_sub(1)
            }
            INC_ABS | INC_ZP | INC_ZP_X | INC_ABS_X => {
                self.current_value.wrapping_add(1)
            }
            _ => panic!("0 Should not happen"),
        };
        self.memory.set(self.current_address, value);
        self.set_nz(value);
        self.finished = true;
    }

    fn read(&mut self, reread: bool) {
        let mut nz = true;
        let mut value = if reread { self.memory.get(self.current_address) }
            else { self.current_value };
        match self.current_opcode {
            LDA_ABS | LDA_ZP | LDA_ZP_X | LDA_ABS_X | LDA_ABS_Y | LDA_IND_X | LDA_IND_Y | LDA_IMM
                => { self.a = value; }
            LDX_ABS | LDX_ZP | LDX_ABS_Y | LDX_IMM | LDX_ZP_Y => { self.x = value; }
            LDY_ABS | LDY_ZP | LDY_ZP_X | LDY_ABS_X | LDY_IMM => { self.y = value; }
            EOR_ABS | EOR_ZP | EOR_ZP_X | EOR_ABS_X | EOR_ABS_Y | EOR_IND_X | EOR_IND_Y | EOR_IMM
                => {
                self.a = self.a ^ value;
                value = self.a;
            }
            AND_ABS | AND_ZP | AND_ZP_X | AND_ABS_X | AND_ABS_Y | AND_IND_X | AND_IND_Y | AND_IMM
                => {
                self.a = self.a & value;
                value = self.a;
            }
            ORA_ABS | ORA_ZP | ORA_ZP_X | ORA_ABS_X | ORA_ABS_Y | ORA_IND_X | ORA_IND_Y | ORA_IMM
                => {
                self.a = self.a | value;
                value = self.a;
            }
            ADC_ABS | ADC_ZP | ADC_ZP_X | ADC_ABS_X | ADC_ABS_Y | ADC_IND_X | ADC_IND_Y | ADC_IMM
                => {
                self.add(value);
                value = self.a;
            }
            SBC_ABS | SBC_ZP | SBC_ZP_X | SBC_ABS_X | SBC_ABS_Y | SBC_IND_X | SBC_IND_Y | SBC_IMM
            => {
                self.sbc(value);
                value = self.a;
            }
            CMP_ABS | CMP_ZP | CMP_ZP_X | CMP_ABS_X | CMP_ABS_Y | CMP_IND_X | CMP_IND_Y | CMP_IMM
            => {
                value = self.cmp(self.a, value);
            }
            CPX_ZP | CPX_ABS => {
                value = self.cmp(self.x, value);
            }
            CPY_ZP | CPY_ABS => {
                value = self.cmp(self.y, value);
            }
            BIT_ABS | BIT_ZP => {
                nz = false;
                value = self.bit(value);
            }
            _ => panic!("1 Should not happen"),
        }
        if nz {
            self.set_nz(value);
        }
        self.finished = true;
    }

    fn write(&mut self) {
        match self.current_opcode {
            STA_ABS | STA_ZP | STA_ZP_X | STA_ABS_X | STA_ABS_Y | STA_IND_X | STA_IND_Y =>
                { self.memory.set(self.current_address, self.a); }
            STX_ABS | STX_ZP | STX_ZP_Y => { self.memory.set(self.current_address, self.x); }
            STY_ABS | STY_ZP | STY_ZP_X => { self.memory.set(self.current_address, self.y); }
            _ => { panic!("Should never happen"); }
        }
        self.finished = true;
    }

    // fn cmp(&mut self, register: u8, v: u8) {
    //     // let tmp: i8 = 0;
    //     let tmp: i8 = (register as i16 - v as i16) as i8;
    //     self.p.set_c(register >= v);
    //     self.p.set_z(tmp == 0);
    //     self.p.set_n(tmp < 0);
    // }

    fn cmp(&mut self, register: u8, v: u8) -> u8 {
        self.p.set_c(register >= v);
        (register as i16 - v as i16) as u8
    }

    pub fn nmi(&mut self) {
        debug!(target: "cpu", "NMI received");
        self.pending_interrupt = Some((0xfffa, 0xfffb));
    }

    pub fn irq(&mut self) {
        debug!(target: "cpu", "IRQ received");
        self.pending_interrupt = Some((0xfffe, 0xffff));
    }

    fn handle_interrupt_full(&mut self, brk: bool, low: u16, high: u16) {
        self.current_cycle = 2;
        while self.current_cycle <= 7 {
            self.handle_interrupt(brk, low, high);
            self.current_cycle += 1;
        }
        self.current_cycle = 1;
    }

    fn asl(&mut self, v: u8) -> u8 {
        self.p.set_c(v & 0x80 != 0);
        let result: u8 = v << 1;
        self.set_nz(result);
        result
    }

    fn handle_interrupt(&mut self, _brk: bool, low: u16, high: u16) {
        match self.current_cycle {
            2 => {
                //         2    PC     R  read next instruction byte (and throw it away),
                //                        increment PC
                let _ = self.memory.get(self.pc()) as usize;
                // self.inc_pc();
            }
            3 => {
                //         3  $0100,S  W  push PCH on stack (with B flag set), decrement S
                self.push_to_stack(((self.pc() & 0xff00) >> 8) as u8);
                self.p.set_b(true);
            }
            4 => {
                //         4  $0100,S  W  push PCL on stack, decrement S
                self.push_to_stack((self.pc() & 0xff) as u8);
            }
            5 => {
                //         5  $0100,S  W  push P on stack, decrement S
                self.push_to_stack(self.p.value());
                self.p.set_i(true);
            }
            6 => {
                //         6   $FFFE   R  fetch PCL
                info!("Reading low:{low:004X}");
                self.low_byte =  self.memory.get(low) as usize;
                debug!(target: "cpu", "6 pc: {:04X}", self.pc());
            }
            7 => {
                //         7   $FFFF   R  fetch PCH
                let high = (self.memory.get(high) as u16) << 8;
                self.set_pc(self.low_byte as u16 | high);
                info!("NMI Jumping to PC:{:04X}", self.pc());
                self.finished = true;
                debug!(target: "cpu", "7 pc: {:04X}", self.pc());
            }
            _ => if self.current_cycle != 1 {
                panic!("Cycle {} should not happen", self.current_cycle)
            }
        }
    }
}
