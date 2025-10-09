use std::fmt::{Display, Formatter};

/// PPUCTRL: $2000
/// 7  bit  0
/// ---- ----
/// VPHB SINN
/// |||| ||||
/// |||| ||++- Base nametable address
/// |||| ||    (0 = $2000; 1 = $2400; 2 = $2800; 3 = $2C00)
/// |||| |+--- VRAM address increment per CPU read/write of PPUDATA
/// |||| |     (0: add 1, going across; 1: add 32, going down)
/// |||| +---- Sprite pattern table address for 8x8 sprites
/// ||||       (0: $0000; 1: $1000; ignored in 8x16 mode)
/// |||+------ Background pattern table address (0: $0000; 1: $1000)
/// ||+------- Sprite size (0: 8x8 pixels; 1: 8x16 pixels – see PPU OAM#Byte 1)
/// |+-------- PPU master/slave select
/// |          (0: read backdrop from EXT pins; 1: output color on EXT pins)
/// +--------- Vblank NMI enable (0: off, 1: on)
#[derive(Copy, Clone, Debug, Default)]
pub struct PpuCtrl {
    pub vbl: bool,
    ppu_master: bool,
    // 8 or 16
    pub sprite_height: u8,
    pub background_table: u16,   // 0 or 0x1000
    pub(crate) sprite_table: u16, // 0 or 0x1000
    pub vram_increment: u16, // 1 or 0x20
    base_nametable: u16,  // $2000, $2400, $2800, $2c00
}


impl PpuCtrl {
    pub fn new(value: u8) -> Self {
        let vbl = 0 != (value & (1 << 7));
        let ppu_master = 0 == (value & (1 << 6));
        let sprite_height = if 0 == (value & (1 << 5)) { 8 } else { 16 };
        // println!("SPRITE HEIGHT: {sprite_height:?}");
        let background_table = if 0 == (value & (1 << 4)) { 0 } else { 0x1000 };
        let sprite_table = if 0 == (value & (1 << 3)) { 0 } else { 0x1000 };
        let vram_increment = if 0 == (value & (1 << 2))  { 1 } else { 0x20 };
        let base_nametable = match value & 0b0000_0011 {
            0 => 0x2000,
            1 => 0x2400,
            2 => 0x2800,
            3 => 0x2C00,
            _ => { panic!("Should not happen") }
        };

        Self {
            vbl, ppu_master, sprite_height, background_table, sprite_table, vram_increment,
            base_nametable
        }
    }
}

impl Display for PpuCtrl {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let _ = write!(f, "PPUCTRL: {:02X} vbl:{} sprite_height:{} background_table:{:04X} \
             sprite_table:{:04X} vram_increment:{} nametable:{:04X}",
            self.ppu_master as u8,
            self.vbl, self.sprite_height, self.background_table, self.sprite_table,
            self.vram_increment, self.base_nametable);
        Ok(())
    }
}
