pub const CAP_FPS: Option<u16> = Some(60);

pub const DONKEY_KONG: &str = "Donkey_Kong.nes";
pub const PACMAN: &str = "PacMan2.nes";
pub const TRACE_FILE_NAME: &str = "trace.txt";

const DEBUG: bool = false;
pub const DEBUG_ASM: bool = DEBUG;
pub const CPU_TYPE_NEW: bool = true;
pub const _COMPARE_LOGS: bool = true;
pub const DEBUG_MESEN: bool = true;
pub const LOG_TO_FILE: bool = DEBUG;
pub const USE_ICED: bool = true;
pub const SELECTED_ROM: usize = 18;

// Logging
pub const IR: bool = false;
pub const VRAM: bool = false;
pub const ROM: bool = false;
pub const MAPPER: bool = false;
pub const VBL: bool = false;

// Screen
pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 240;
pub const SCALE_X: f32 = 2.0;
pub const SCALE_Y: f32 = 2.0;

// pub const ALL_MAPPERS: [u8; 1] = [9];
pub const ALL_MAPPERS: [u8; 9] = [0, 1, 2, 3, 4, 7, 9, 19, 66];
pub const DEMO_DELAY_SECONDS: u64 = 5;

use once_cell::sync::Lazy;
use std::convert::Into;

pub const WINDOW_TITLE: &str = "CedNES";

#[derive(Clone, Debug)]
pub struct RomInfo {
    pub file_name: String,
    pub _name: Option<String>,
    pub id: usize,
}

impl Default for RomInfo {
    fn default() -> Self {
        RomInfo::n2(0, DONKEY_KONG, "Donkey Kong")
    }
}

impl RomInfo {
    pub fn n(id: usize, file_name: &str) -> Self {
        Self::n3(id, file_name.into(), None)
    }

    fn n2(id: usize, file_name: &str, name: &str) -> Self {
        Self::n3(id, file_name.into(), Some(name.into()))
    }

    fn n3(id: usize, file_name: String, name: Option<String>) -> Self {
        Self {
            file_name,
            _name: name, id,
        }
    }

    pub fn name(&self) -> String {
        // Extract filename (last part after \ or /)
        let filename = self.file_name.split(['\\', '/'])
            .last()
            .unwrap_or(&self.file_name);

        // Remove .nes extension
        let name_without_ext = filename
            .strip_suffix(".nes")
            .unwrap_or(filename);

        // Replace non-alphanumeric with spaces, then clean up multiple spaces
        let cleaned = name_without_ext
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { ' ' })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<&str>>()
            .join(" ");

        cleaned
    }

    pub fn file_name(&self) -> String { self.file_name.clone() }

    pub fn mapper_number(&self) -> u8 {
        crate::rom_list::mapper_number(&self.file_name)
    }
}

pub static ROM_NAMES: Lazy<Vec<RomInfo>> = Lazy::new(|| {
    vec![
        RomInfo::default(),
        RomInfo::n2(1, PACMAN, "PacMan"),
        RomInfo::n2(2, "Ice Climber.nes", "Ice Climber"),
        RomInfo::n2(3, "Super Mario Bros. (W) (V1.0) [!].nes", "Super Mario Bros"),
        RomInfo::n2(4, "Balloon Fight (E).nes", "Balloon Fight"),
        RomInfo::n2(5, "Spelunker (U) [!].nes", "Spelunker"),
        RomInfo::n2(6, "Wrecking Crew (W) [!].nes", "Wrecking Crew"),
        RomInfo::n2(7, "Donkey Kong Jr. (U) (V1.1) (e-Reader) [!].nes", "Donkey Kong Jr"),
        RomInfo::n2(8, "Urban Champion (U) (e-Reader) [!].nes", "Urban Champion"),
        RomInfo::n2(9, "Clu Clu Land (U) (e-Reader) [!].nes", "Clu Clu Land"),
        RomInfo::n2(11, "1942 (JU) [!].nes", "1942"),
        RomInfo::n2(12, "Chou Fuyuu Yousai - Exed Exes (J) [!].nes", "Exed Exes"),
        RomInfo::n2(13, "Gyrodine (J) [!].nes", "Gyrodine"),
        RomInfo::n2(14, "Ms. Pac-Man (U) (Namco) [!].nes", "Ms PacMan"),
        RomInfo::n2(15, "Bomberman Collection [p1].nes", "Bomberman"),
        RomInfo::n2(16, "AccuracyCoin.nes", "Accuracy Coin"),
        RomInfo::n2(17, "test-rom/test-rom.nes", "Test ROM"),
        RomInfo::n2(18, "test-rom/apu_test.nes", "Test ROM"),

        // Mqpper 1
        RomInfo::n2(100, "Legend of Zelda, The (U) (V1.0) [!].nes", "Zelda"),
        RomInfo::n2(101, "Lemmings (E) [!].nes", "Lemmings"),
        RomInfo::n2(102, "Chessmaster, The (U) (V1.0) [!].nes", "Chessmaster"),
        RomInfo::n2(103, "Wizardry - Proving Grounds of the Mad Overlord (U) [!].nes", "Wizardry"),
        RomInfo::n2(104, "Metroid (U) (VC) [!].nes", "Metroid"),
        RomInfo::n2(105, "International Cricket (A) (Beta-1992.08.24) [!].nes", "International Cricket"),
        RomInfo::n2(106, "Sesame Street - 123 (U) [!].nes", "Sesame Street"),
        RomInfo::n2(107, "Cosmic Wars (J) [!].nes", "Cosmic Wars"),
        RomInfo::n2(108, "Hyokkori Hyoutan-jima - Nazo no Kaizokusen (J) [!].nes", "Hyokkori"),
        // Crash
        RomInfo::n2(109, "Genghis Khan (U) [!].nes", "Genghis Khan"),
        RomInfo::n2(110, "Defender of the Crown (U) [!].nes", "Defender of the Crown"),
        RomInfo::n2(111, "Tetris.nes", "Tetris"),
        RomInfo::n2(112, "Snow Brothers (USA).nes", "Snow Brothers"),

        // Mapper 2
        RomInfo::n2(200, "Castlevania (U) (V1.0) [!].nes", "Castlevania"),
        RomInfo::n2(201, "1943 - The Battle of Midway (U) [!].nes", "Battle of Midway"),
        RomInfo::n2(202, "3-D WorldRunner (U) [!].nes", "3D WorldrRunner"),
        RomInfo::n2(203, "Caesars Palace (U) [!].nes", "Caesar's Palace"),
        RomInfo::n2(204, "Prince of Persia (U) [!].nes", "Prince of Persia"),

        // CNROM (mapper 3)
        RomInfo::n2(300, "Arkanoid (U) [!].nes", "Arkanoid"),
        RomInfo::n2(301, "Legend of Kage, The (USA).nes", "Legend of Kage"),
        RomInfo::n2(302, "Adventure Island Classic (E) [!].nes", "Adventure Island"),

        // MMC3 (mapper 4)
        RomInfo::n2(400, "Super Mario Bros. 3 (USA) (Rev 1).nes", "Super Mario Bros 3"),
        RomInfo::n2(401, "War on Wheels (U) (Proto) [!].nes", "War on Wheels"),
        RomInfo::n2(402, "Hoshi no Kirby - Yume no Izumi no Monogatari (J) [!].nes", "Kirby"),
        RomInfo::n2(403, "F-15 Strike Eagle (U) [!].nes", "F-15 Strike Eagle"),
        RomInfo::n2(404, "Kirby's Adventure (E).nes", "Kirby's Adventure"),
        RomInfo::n2(405, "AD&D Dragon Strike (U).nes", "Dragon Strike"),

        // Mapper 7
        RomInfo::n2(700, "Battletoads (U) [!].nes", "Battletoads"),

        // Mapper 9
        RomInfo::n2(900, "Mike Tyson's Punch-Out!! (E) (V1.0) [!].nes", "Mike Tyson's Punch-Out"),

        // Mapper 19
        RomInfo::n2(1900, "championship.nes", "PacMan Championship Edition"),
        RomInfo::n2(1901, "Battle Fleet (J) [!].nes", "Battle Fleet"),
        RomInfo::n2(1902, "Digital Devil Story - Megami Tensei II (J) (V1.0) [!].nes", "Digital Devil Story"),

        // Mapper 69
        RomInfo::n2(6900, "Honoo no Toukyuuji - Dodge Danpei 2 (Japan).nes", "Honoo"),

        // Tests
        RomInfo::n(551, "01.basics.nes"),
        RomInfo::n(552, "02.alignment.nes"),
        RomInfo::n(553, "03.corners.nes"),
        RomInfo::n(554, "04.flip.nes"),
        RomInfo::n(555, "05.left_clip.nes"),
        RomInfo::n(556, "06.right_edge.nes"),
        RomInfo::n(557, "07.screen_bottom.nes"), // fail
        RomInfo::n(558, "08.double_height.nes"),
        RomInfo::n(559, "09.timing_basics.nes"), // fail
        RomInfo::n(561, "palette_ram.nes"),
        RomInfo::n(562, "sprite_ram.nes"),
        RomInfo::n(582, "1.Branch_Basics.nes"),
        RomInfo::n(583, "2.Backward_Branch.nes"),
        RomInfo::n(584, "3.Forward_Branch.nes"),
        RomInfo::n(585, "color_test.nes"),
        RomInfo::n(586, "nestest.nes"),
    ]
});
