use reqwest::blocking::Response;
use tracing::{info, warn};
use crate::debugger_interface::{SetMemoriesJson, SetMemoryJson, SetRegisterJson, SetRegistersJson};
use crate::emulator::Emulator;

/// Snapshot of CPU registers for the debugger UI (names must match UDI).
#[derive(Clone, Debug)]
pub(crate) struct RegisterSnapshot {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub s: u8,
    pub p: u8,
    pub pc: u16,
}

impl RegisterSnapshot {
    pub(crate) fn from_emulator(emulator: &Emulator) -> Self {
        Self {
            a: emulator.cpu.a,
            x: emulator.cpu.x,
            y: emulator.cpu.y,
            s: emulator.cpu.s,
            p: emulator.cpu.p.value(),
            pc: emulator.cpu.pc(),
        }
    }
}

/// POST /v1/control?command=run|pause
pub(crate) fn send_control(command: &str) -> reqwest::Result<Response> {
    let client = reqwest::blocking::Client::new();
    let url = format!("http://localhost:9000/v1/control?command={command}");
    info!("Sending control: {command}");
    client.post(&url).send()
}

pub(crate) fn send_memories(cpu: &[u8], ppu: &[u8], oam: &[u8]) -> reqwest::Result<Response> {
    let json = SetMemoriesJson {
        memories: vec![
            SetMemoryJson {
                name: "CPU".into(),
                content: cpu.into(),
            },
            SetMemoryJson {
                name: "PPU".into(),
                content: ppu.into(),
            },
            SetMemoryJson {
                name: "OAM".into(),
                content: oam.into(),
            },
        ]
    };
    info!("Sending memories (CPU {} bytes, PPU {} bytes, OAM {} bytes)",
        cpu.len(), ppu.len(), oam.len());

    let client = reqwest::blocking::Client::new();
    client.post("http://localhost:9000/v1/set_memories")
        .json(&json)
        .send()
}

pub(crate) fn send_registers(regs: &RegisterSnapshot) -> reqwest::Result<Response> {
    // UDI panel names: A X Y S P PC  (not "SP")
    let json = SetRegistersJson {
        registers: vec![
            SetRegisterJson {
                name: "A".into(),
                value: format!("{:02X}", regs.a),
            },
            SetRegisterJson {
                name: "X".into(),
                value: format!("{:02X}", regs.x),
            },
            SetRegisterJson {
                name: "Y".into(),
                value: format!("{:02X}", regs.y),
            },
            SetRegisterJson {
                name: "S".into(),
                value: format!("{:02X}", regs.s),
            },
            SetRegisterJson {
                name: "P".into(),
                value: format!("{:02X}", regs.p),
            },
            SetRegisterJson {
                name: "PC".into(),
                value: format!("{:04X}", regs.pc),
            },
        ]
    };

    info!(
        "Sending registers A={:02X} X={:02X} Y={:02X} S={:02X} P={:02X} PC={:04X}",
        regs.a, regs.x, regs.y, regs.s, regs.p, regs.pc
    );

    let client = reqwest::blocking::Client::new();
    client.post("http://localhost:9000/v1/set_registers")
        .json(&json)
        .send()
}

/// Best-effort push to UDI: control + registers first (fast), then memories.
/// Register send is not skipped if the large memory dump fails.
pub(crate) fn push_debug_state(
    regs: &RegisterSnapshot,
    cpu: &[u8],
    ppu: &[u8],
    oam: &[u8],
) -> reqwest::Result<Response> {
    if let Err(e) = send_control("pause") {
        warn!("UDI control pause failed: {e:?}");
    }

    // Registers first so a slow/failed memory POST cannot leave the UI stuck.
    let reg_response = send_registers(regs)?;

    match send_memories(cpu, ppu, oam) {
        Ok(r) => info!("UDI set_memories status: {:?}", r.status()),
        Err(e) => warn!("UDI set_memories failed (registers already sent): {e:?}"),
    }

    Ok(reg_response)
}