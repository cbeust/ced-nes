use std::fmt::{Display, Formatter};
use tracing::debug;

/// Internal registers
/// v and t have the following format:
/// yyy NN YYYYY XXXXX
/// ||| || ||||| +++++-- coarse X scroll
/// ||| || +++++-------- coarse Y scroll
/// ||| ++-------------- nametable select
/// +++----------------- fine Y scroll
#[derive(Copy, Clone, Default)]
pub struct IR {
    pub v: u16,  // 15 bits
    pub t: u16,  // 15 bits
    pub x: u8,   // fine_x, 3 bits
    pub w: bool, // w, 1 bit
}

impl IR {
    pub fn v(&self) -> u16 { self.v }
    pub fn set_v(&mut self, v: u16) { self.v = v; }
    pub fn set_t(&mut self, v: u16) { self.t = v; }

    pub fn set_coarse_x(&mut self, x: u16) {
        debug!(target: "ir", "set_coarse_x({x}) IR:{}",self);
        self.v = (self.v & !0x1F) | x;
    }

    pub fn set_coarse_y(&mut self, y: u16) {
        debug!(target: "ir", "set_coarse_y({y}) IR:{}",self);
        let mask = 0b11111_00000;
        self.v = (self.v & !mask) | (y << 5);
    }

    pub fn increment_coarse_x(&mut self) {
        debug!(target: "ir", "increment_coarse_x IR:{}",self);
        self.set_coarse_x((self.coarse_x() + 1) & 0b11111);
    }

    pub fn increment_coarse_y(&mut self) {
        debug!(target: "ir", "increment_coarse_y IR:{}",self);
        self.set_coarse_y((self.coarse_y() + 1) & 0b11111);
    }

    pub fn increment_fine_y(&mut self) {
        debug!(target: "ir", "increment_fine_y IR:{}",self);
        self.set_fine_y((self.fine_y() + 1) & 0b111);
    }

    pub fn set_fine_y(&mut self, y: u16) {
        debug!(target: "ir", "set_fine_y({y:02X}) IR:{}",self);
        let mask = 0x7000;
        self.v = (self.v & !mask) | (y << 12);
    }

    pub fn horizontal_nametable(&self) -> u16 {
        (self.v & 0x400) >> 10
    }

    pub fn switch_horizontal_nametable(&mut self) {
        debug!(target: "ir", "witch_horizontal_nametable IR:{}",self);
        self.v ^= 0x0400;                 // switch vertical nametable
    }

    pub fn vertical_nametable(&self) -> u16 {
        (self.v & 0x800) >> 11
    }

    pub fn switch_vertical_nametable(&mut self) {
        debug!(target: "ir", "switch_vertical_nametable IR:{}",self);
        self.v ^= 0x0800;                 // switch vertical nametable
    }

    pub fn hori_v_equals_hori_t(&mut self) {
        debug!(target: "ir", "hori(v)=hori(t) IR:{}",self);
        let mask = 0b000_01_00000_11111;
        self.v = (self.v & !mask) | (self.t & mask);
    }

    pub fn vert_v_equals_vert_t(&mut self) {
        debug!(target: "ir", "vert(v)=vert(t) IR:{}",self);
        let mask = 0b111_10_11111_00000;
        self.v = (self.v & !mask) | (self.t & mask);
    }

    pub fn increment_v(&mut self, inc: u16) {
        debug!(target: "ir", "increment_v({inc:02X}) IR:{}",self);
        self.v = self.v.wrapping_add(inc) & 0x3fff;
    }

    pub fn set_v_to_t(&mut self) {
        debug!(target: "ir", "set_v_to_t IR:{}",self);
        self.v = self.t;
    }

    fn format_binary_with_underscores(n: u16) -> String {
        let binary = format!("{:b}", n); // format to binary
        binary
            .chars()
            .rev()
            .collect::<Vec<_>>()
            .chunks(4)
            .map(|chunk| chunk.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join("_")
            .chars()
            .rev()
            .collect()
    }

    pub fn coarse_y(&self) -> u16 {
        (self.v >> 5) & 0b1_1111
    }

    pub fn fine_y(&self) -> u16 {
        (self.v & 0b111_00_00000_00000) >> 12
    }

    pub fn coarse_x(&self) -> u16 {
        self.v & 0b1_1111
    }

    pub fn fine_x(&self) -> u16 {
        self.x as u16
    }

    pub fn nametable(&self) -> u16 {
        (self.v & 0b11_00000_00000) >> 10
    }
}

impl Display for IR {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!("IR: v:{:04X}/{} t:{:04X}/{} w:{} NT:{} \
         coarse_x:{} fine_x:{} coarse_y:{} fine_y:{}",
            self.v, Self::format_binary_with_underscores(self.v),
            self.t, Self::format_binary_with_underscores(self.t),
            self.w, self.nametable(),
            self.coarse_x(), self.fine_x(),
            self.coarse_y(), self.fine_y()
        ))
    }
}
