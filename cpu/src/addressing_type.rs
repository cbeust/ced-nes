#[derive(PartialEq, Debug, Copy, Clone)]

#[allow(non_camel_case_types)]
pub enum AddressingType {
    Immediate, Zp, Zp_X, Zp_Y, Absolute, Absolute_X, Absolute_Y, Indirect_X, Indirect_Y, Register_A,
    Indirect, Relative, Zpi, Zp_Relative, Indirect_Abs_X, Unknown
}

fn h(v: u8, labels: &Labels) -> String {
    if let Some(label) = labels.get(&(v as u16)) {
        label.into()
    } else {
        format!("${:02X}", v)
    }
}

fn hh(v: u16, labels: &Labels) -> String {
    if let Some(label) = labels.get(&(v)) {
        label.into()
    } else {
        format!("${:04X}", v)
    }
}

use std::collections::HashMap;
use std::fmt::{Display, Formatter, Result};
use crate::addressing_type::AddressingType::{Absolute_X, Absolute_Y};
use crate::cpu::Cpu;
use crate::labels::Labels;
use crate::memory::{DefaultMemory, Memory};

impl Display for AddressingType {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{:?}", self)
        // or, alternatively:
        // fmt::Debug::fmt(self, f)
    }
}

impl AddressingType {
    pub fn to_string(&self, pc: u16, byte: u8, word: u16, labels: &Labels) -> String {
        use AddressingType::*;
        match self {
            Immediate => format!("#{}", h(byte, labels)),
            Zp => format!("{}", h(byte, labels)),
            Zp_X => format!("{},X", h(byte, labels)),
            Zp_Y => format!("{},Y", h(byte, labels)),
            Zpi => format!("({})", h(byte, labels)),
            Absolute => format!("{}", hh(word, labels)), Absolute_X =>
                format!("{},X", hh(word, labels)),
            Absolute_Y => format!("{},Y", hh(word, labels)),
            Indirect_X => format!("({},X)", h(byte, labels)),
            Indirect_Y => format!("({}),Y", h(byte, labels)),
            Indirect => format!("({})", hh(word, labels)),
            Indirect_Abs_X => format!("({},X)", hh(word, labels)), Zp_Relative => {
                let byte = (word >> 8) & 0xff;
                let mut new_pc = pc.wrapping_add(byte) + 3;
                if byte >= 0x80 {
                    new_pc = (new_pc as i64 - 0x100) as u16;
                }
                format!("{}", hh(new_pc, labels))
            }
            Relative => {
                let signed: i64 = 2_i64 + pc as i64 + byte as i64;
                let subtract: i64 = if byte >= 0x7f {0x100} else {0};
                let value = (signed - subtract) as u16;
                format!("{}", hh(value, labels))
            },
            _ => "".to_string()
        }
    }

    // Only used by JMP
    //     fn deref16(&self, mut memory: Memory, pc: usize) -> u16 {
    //         let w = memory.word(pc.wrapping_add(1)) as usize;
    //         memory.word(w)
    //     }

    fn word(&self, mem: &Vec<u8>, address: usize) -> u16 {
        mem[address] as u16 | (mem[address + 1] as u16) << 8
    }

    pub(crate) fn address<T: Memory>(&self, pc: u16, cpu: &mut Cpu<T>) -> u16 {
        use AddressingType::*;

        // if pc == 49162 {
        //     println!("GETTING WRONG ADDRESS HERE");
        // }
        let memory = &mut cpu.memory;
        fn zp(a: u8, b: u8) -> u8 {
            (a as u16 + b as u16) as u8
        }
        fn word(mut memory: &mut impl Memory, a1 : u16, a2: u16) -> u16 {
            let result = (memory.get(a1) as u16) | (memory.get(a2) as u16) << 8;
            // println!("v1: {:02X}, v2: {:02X} value:{result:02X}", v1, v2);
            result
        }

        let next = pc.wrapping_add(1);
        match self {
            Zp => memory.get(next) as u16,
            Zp_X => zp(memory.get(next), cpu.x) as u16,
            Zp_Y => zp(memory.get(next), cpu.y) as u16,
            Absolute => word(memory, next, pc.wrapping_add(2)),
            Absolute_X => {
                let a1 = word(memory, next, pc.wrapping_add(2));
                a1.wrapping_add(cpu.x as u16)
            }
            Absolute_Y => {
                let a1 = word(memory, next, pc.wrapping_add(2));
                a1.wrapping_add(cpu.y as u16)
            }
            Indirect => {
                let address = word(memory, next, pc.wrapping_add(2));
                let next_address =
                    if cpu.is_65c02 {
                        // For the 65C02, always jump to the next byte even if at end of page
                        address.wrapping_add(1)
                    } else {
                        // For 6502 only:
                        // Fix test "6c ff 70"
                        // AN INDIRECT JUMP MUST NEVER USE A VECTOR BEGINNING ON THE LAST BYTE
                        // OF A PAGE
                        // For example if address $3000 contains $40, $30FF contains $80, and $3100
                        // contains $50, the result of JMP ($30FF) will be a transfer of control to
                        // $4080 rather than $5080 as you intended i.e. the 6502 took the low byte of
                        // the address from $30FF and the high byte from $3000.
                        if address & 0xff == 0xff {
                            address & 0xff00
                        } else {
                           address.wrapping_add(1)
                        }
                    };
                (memory.get(next_address) as u16) << 8 | memory.get(address) as u16
            }
            Indirect_X => {
                // The address can wrap around page zero (e.g. $ff then $00) so need
                // to get the individual bytes one by one
                let v0 = memory.get(next);
                let byte0 = memory.get(zp(v0, cpu.x) as u16);
                let byte1 = memory.get(zp(v0, cpu.x.wrapping_add(1)) as u16);
                ((byte1 as u16) << 8) | (byte0 as u16)
            },
            Indirect_Y => {
                let zp = memory.get(next);
                let w = DefaultMemory::word_ind_y_mem(memory, zp as u16, true);
                w.wrapping_add(cpu.y as u16)
            },
            Indirect_Abs_X => {
                let w = word(memory,next, pc.wrapping_add(2));
                let content = w.wrapping_add(cpu.x as u16);
                word(memory, content, content + 1)
            }
            Zpi => {
                let zp = memory.get(next) as u16;
                let byte0 = memory.get(zp);
                let byte1 = memory.get(if zp == 0xff { 0 } else { (zp).wrapping_add(1) });
                (byte1 as u16) << 8 | (byte0 as u16)
            }
            Zp_Relative => {
                // mem[next as usize] as u16
                memory.get(next) as u16
            }
            Immediate | Relative | Register_A | Unknown => 0

        }
    }
}

