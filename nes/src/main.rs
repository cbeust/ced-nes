mod rom;
mod iced;
mod color;
pub mod nes_memory;
mod ppu;
mod emulator;
mod joypad;
mod app;
mod minifb;
mod constants;
mod listview;
mod rom_list;
mod bits;
mod logging;
pub mod internal_registers;
mod ppu_mask;
mod ppu_ctrl;
mod config_file;
mod apu;
// mod v2;

#[cfg(test)]
mod test;
mod mappers;
mod mesen_logger;
// mod test_rom;

use crate::config_file::EmulatorConfig;
use crate::constants::{RomInfo, ALL_MAPPERS, CPU_TYPE_NEW, LOG_TO_FILE, ROM_NAMES, SELECTED_ROM, TRACE_FILE_NAME, USE_ICED};
use crate::iced::main_iced;
use crate::logging::init_logging;
use crate::minifb::main_minifb;
use crate::rom_list::find_roms_with_mappers;
use clap::Parser;
use tracing::debug;

#[derive(Default, Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Path of the rom
    #[arg(long)]
    rom_name: Option<String>,

    /// Directory containing ROM files.
    #[arg(long)]
    rom_dir: Option<String>,

    /// ROM file names to load
    #[arg(long, num_args = 1.., value_parser = clap::value_parser!(u8))]
    rom_names: Vec<String>,

    /// Number of the rom to launch (dev mode only)
    #[arg(long)]
    rom: Option<usize>,

    #[arg(long)]
    demo: bool,

    #[arg(long)]
    dev: bool,
}

impl Clone for Args {
    fn clone(&self) -> Self {
        Self {
            rom_name: None,
            rom_dir: self.rom_dir.clone(),
            rom_names: self.rom_names.clone(),
            rom: None,
            demo: false,
            dev: false,
        }
    }
}

// #[tokio::main]
pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    // apu.rs::play();
    // exit(0);

    let filename = if CPU_TYPE_NEW { "trace-new-cpu.txt" } else { TRACE_FILE_NAME };
    let _guard = init_logging(
        if LOG_TO_FILE { Some(filename.into() ) } else { None },
        cpu::cpu::LOG_ASYNC
    );

    debug!("Trying to open config file: {}", EmulatorConfig::config_file_name());

    let mut config = EmulatorConfig::read_or_create().unwrap();

    // Parse command-line arguments
    let mut args = Args::parse();

    if config.rom_dir.is_none() {
        if let Some(rom_dir) = &args.rom_dir {
            config.rom_dir = Some(rom_dir.clone());
            config.save()?;
        } else {
            return Err("Specify the rom directory with --rom-dir".into());
        }
    }

    // config.rom_dir = Some("C:\\users\\cedric\\t".into());
    // config.save().unwrap();

    // test_nametable_mirroring();
    // create_test_rom();
    // test_mapper3();
    // exit(0);
    // test_internal_registers();
    // test_horizontal_scrolling();
    // convert();
    // test_all();

    // joypad::test::test_strobe_mode_on_off();

    let roms: &Vec<RomInfo> = if args.dev {
        &ROM_NAMES.iter().cloned().collect()
    } else {
        &find_roms_with_mappers(&config.rom_dir.unwrap(), ALL_MAPPERS.into())
    };

    // Log the parsed ROM IDs
    let rom_info = {
        let index2 =
            if let Some(index) = &args.rom {
                index
            } else {
                &SELECTED_ROM
            };
        let index = roms.iter().enumerate().find(|(_index, rom)| rom.id == *index2)
            .map_or(0, |(index, _)| index);
        // .cloned().unwrap_or(ROM_NAMES[0].clone());
        if args.rom_names.is_empty() {
            args.rom_names.push(roms[index].file_name.clone());
        }
        if let Some(name) = &args.rom_name {
            RomInfo::n(0, name)
        } else {
            roms[index].clone()
        }
    };

    if USE_ICED {
        main_iced(args, roms.clone(), rom_info);
    } else {
        main_minifb(args);
    }

    Ok(())
}
