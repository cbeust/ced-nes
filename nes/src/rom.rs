use crate::is_set;
use std::cmp::min;
use std::fs::File;
use std::io::Read;
use tracing::debug;

#[derive(Clone, Debug, Default)]
pub struct Header {
    /// 16 KB units
    pub(crate) prg_rom_count: usize,
    /// 8 KB units (0 means the board uses CHR RAM)
    pub chr_rom_count: usize,
    // Memory backed ram at $6000-$7FFF
    _battery_backed_ram: bool,
    // Byte trainer at $7000-$71FF
    // byte_trainer: bool,
    _four_screen_mirroring: bool,
    _mapper_number: u16,
    _submapper_number: u16,
    pub(crate) mirroring: Mirroring,
    _chr_ram_size: usize,
    _chr_nvram_size: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Mirroring {
    #[default]
    Vertical,
    Horizontal,
    FourScreen,
    SingleScreen,
    ScreenA,
    ScreenB,
}

pub const CHR_ROM_SIZE: usize = 0x2000; // 8K
pub(crate) const PRG_ROM_SIZE: usize = 0x4000; // 16K

#[derive(Clone)]
pub struct Rom {
    pub header: Header,
    pub prg_rom: Vec<u8>, // Size PRG_ROM_SIZE
    pub chr_rom: Vec<u8>, // Size CHR_ROM_SIZE
    // prg_ram: Vec<u8>,
    pub mapper: u16,
    pub submapper: u16,
}

impl Default for Rom {
    fn default() -> Self {
        Self {
            header: Header::default(),
            prg_rom: vec![0; 0x8000], // Size PRG_ROM_SIZE
            chr_rom: vec![0; 0x2000], // Size CHR_ROM_SIZE
            // prg_ram: Vec::new(),
            mapper: 0,
            submapper: 0,
        }
    }
}
impl Rom {
    fn find_rom(file_name: & str) -> Result<File,() > {
        if let Ok(f) = File::open(file_name) {
            Ok(f)
        } else if let Ok(f) = File::open( & format ! ("nes/{file_name}")) {
            Ok(f)
        } else {
            Err(())
        }
    }

    pub fn read_nes_file(file_name: &str) -> Result<Rom, ()> {
        let mut file = Self::find_rom(file_name)
            .expect(&format!("File {file_name} should exist"));
        let mut buffer: Vec<u8> = Vec::new();
        file.read_to_end(&mut buffer).unwrap();

        //
        // Read header (0..0x10)
        //
        assert!(buffer[0] == b'N' && buffer[1] == b'E' && buffer[2] == b'S' && buffer[3] == 0x1a);
        let byte6 = buffer[6] as u16;
        let byte7 = buffer[7] as u16;
        let byte8 = buffer[8] as u16;
        let _mapper_number = (byte6 & 0xf0) >> 4 | (byte7 & 0xf0) | (byte8 & 0xf) << 8;
        let _submapper_number = (byte8 & 0xf0) >> 4;
        let vertical_mirroring = is_set!(byte6, 0);
        let four_screen = is_set!(byte6, 3);
        let mirroring = match (four_screen, vertical_mirroring) {
            (true, _) => Mirroring::FourScreen,
            (false, true) => Mirroring::Vertical,
            (false, false) => Mirroring::Horizontal,
        };

        let header = Header {
            prg_rom_count: buffer[4] as usize | ((buffer[9] as usize & 0xf) << 4),
            chr_rom_count: buffer[5] as usize | (buffer[9] as usize & 0xf0),
            _battery_backed_ram: is_set!(byte6, 1),
            // byte_trainer: is_set!(byte6, 2),
            _four_screen_mirroring: is_set!(byte6, 3),
            _chr_ram_size: buffer[11] as usize & 0xf,
            _chr_nvram_size: (buffer[11] as usize & 0xf0) >> 4,
            _mapper_number,
            _submapper_number,
            mirroring,
        };

        let prg_size = header.prg_rom_count * PRG_ROM_SIZE;
        let chr_size = header.chr_rom_count * CHR_ROM_SIZE;
        debug!(target: "rom", "Read {}, size:${:X} prg_size:${:X} ({}K) chr_size:${:X} ({}K) mapper:{}",
            file_name, buffer.len(),
            prg_size, prg_size / 1024,
            chr_size, chr_size / 1024,
            _mapper_number);
        debug!(target: "rom", "Header: {header:#?}");

        //
        // Extract PRG ROM data
        //
        let mut prg_rom: Vec<u8> = Vec::new();
        let size = min(prg_size, buffer.len() - 0x10);
        for i in 0..size {
            prg_rom.push(buffer[i + 0x10]);
        }

        let mut chr_rom: Vec<u8> = Vec::new();

        //
        // Read CHR
        //
        let chr_rom_offset = 0x10 + prg_size;
        if chr_size > 0 {
            for i in 0..chr_size {
                chr_rom.push(buffer[chr_rom_offset + i]);
            }
        } else {
            for _ in 0..CHR_ROM_SIZE {
                chr_rom.push(0);
            }
        }

        // debug!(target: "rom", "File offsets");
        // for i in 0..header.prg_rom_count {
        //     let i = i as usize;
        //     debug!(target: "rom", "PRG Bank {i}: {:05X}-{:05X}",
        //         0x10 + i * PRG_ROM_SIZE, 0x10 + (i + 1) * PRG_ROM_SIZE - 1);
        // }
        // for i in 0..header.chr_rom_count * 2 {
        //     let i = i as usize;
        //     debug!(target: "rom", "CHR Bank {i}: {:05X}-{:05X}",
        //         chr_rom_offset + i * 0x1000, chr_rom_offset + (i + 1) * 0x1000 - 1);
        // }

        let rom = Rom {
            header,
            chr_rom,
            prg_rom,
            // prg_ram: Vec::new(),
            mapper: _mapper_number,
            submapper: _submapper_number,
        };

        Ok(rom)
    }
}

fn _display_character(table: &[u8]) {
    for i in 0..8 {
        for j in 0..8 {
            match table[i * 8 + j] {
                0 => { print!("."); }
                _ => { print!("*"); }
            }
        }
        println!("");
    }
}

