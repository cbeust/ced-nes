use std::env::home_dir;
use std::fs::File;
use std::io::{BufRead, BufReader};
use tracing::info;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = [home_dir().unwrap().to_str().unwrap(), "t".into()].join("/");
    let file1 = &format!("{dir}/nestest.log.txt");
    let file2 = &format!("{dir}/trace-new-cpu.txt");
    compare_log(file1, file2)
}

#[derive(Debug)]
struct LogLine {
    line: String,
    pc: String,
    cycles: u32,
    asm: String,
    _mem_value: Option<String>,
    registers: String,
    v: u16,
    h: u16,
}


#[derive(Debug)]
enum LineStatus {
    EOF,
    SKIP,
}

fn parse_cpu_trace_line(reader: &mut BufReader<File>, line_number: u64)
    -> Result<LogLine, LineStatus>
{
    let mut line_read = String::new();
    reader.read_line(&mut line_read).unwrap();
    let line = line_read.clone();
    if line.is_empty() {
        return Err(LineStatus::EOF);
    }

    // INFO:asm - 033,000 - 00000008 00 00 00 01FD ..RB.I.. C000: 4C F5 C5   JMP $C5F5     |
    let dashes = line_read.split("-").collect::<Vec<&str>>();

    // H,V
    let mut hv = dashes[1].split(",");;
    let h = u16::from_str_radix(hv.next().unwrap().trim(), 10).unwrap();
    let v = u16::from_str_radix(hv.next().unwrap().trim(), 10).unwrap();

    // Asm
    let d = dashes[2].split(" ").collect::<Vec<&str>>();
    // println!("line:{line}");
    let cycles = u32::from_str_radix(d[1], 10).unwrap();
    let a = u8::from_str_radix(d[2], 16).unwrap();
    let x = u8::from_str_radix(d[3], 16).unwrap();
    let y = u8::from_str_radix(d[4], 16).unwrap();
    let flags = d[6];
    let flags = parse_flags(flags);
    let sp = u16::from_str_radix(d[5], 16).unwrap() & 0xff;
    let registers = format!("A:{a:02X} X:{x:02X} Y:{y:02X} P:{flags:02X} SP:{sp:02X}");
    let pc = &d[7][.. d[7].len() - 1];
    let mut asm = [d[11], d[12], d[13], d[14], d[15], d[16], d[17], d[18]].join(" ").trim().to_string();
    if let Some(index) = asm.find("|") {
        if let Some((a, _)) = asm.split_once("|") {
            asm = a.into();
        }
    }
    asm = (asm.to_string().trim()).to_string();

    Ok(LogLine {
        line,
        pc: pc.into(),
        cycles: cycles.into(),
        asm,
        _mem_value: None,
        registers,
        v,
        h,
    })

}

fn parse_flags(f: &str) -> u8 {
    let mut result = 0;
    let mut value = 128;
    for c in f.chars() {
        if c != '.' && c != 'B' { result += value; }
        value >>= 1;
    }
    result
}

fn parse_nestest_line(reader: &mut BufReader<File>, line_number: u64)
    -> Result<LogLine, LineStatus>
{
    let mut line_read = String::new();
    reader.read_line(&mut line_read).unwrap();
    let line = line_read.clone();
    if line.is_empty() {
        return Err(LineStatus::EOF);
    }

    // Format:
    // C000  4C F5 C5  JMP $C5F5                       A:00 X:00 Y:00 P:24 SP:FD PPU:  0, 21 CYC:7
    // 012345678901234567890123456789012345678901234567890123456789012345678901234567890123
    // PC    BYTES     ASM                             REGISTERS               PPU          CYC

    let pc = if line.len() >= 4 {
        line[0..4].to_string()
    } else {
        "".to_string()
    };

    // Registers start at index 48
    // A:00 X:00 Y:00 P:24 SP:FD
    let registers = if line.len() >= 73 {
        line[48..73].trim().to_string()
    } else {
        "".to_string()
    };

    // PPU:  0, 21
    let v = if let Some(ppu_pos) = line.find("PPU:") {
        let comma_pos = line[ppu_pos..].find(",").unwrap_or(0);
        let v = line[ppu_pos + 4..ppu_pos + comma_pos].trim().to_string();
        u16::from_str_radix(&v, 10).unwrap()
    } else {
        0
    };

    let h = if let Some(ppu_pos) = line.find("PPU:") {
        let comma_pos = line[ppu_pos..].find(",").unwrap_or(0);
        let cyc_pos = line[ppu_pos..].find("CYC:").unwrap_or(line.len() - ppu_pos);
        let h = line[ppu_pos + comma_pos + 1..ppu_pos + cyc_pos].trim().to_string();
        u16::from_str_radix(&h, 10).unwrap()
    } else {
        0
    };

    // Cycles (CYC) starts after "CYC:"
    let cycles = if let Some(cyc_pos) = line.find("CYC:") {
        line[cyc_pos + 4..].trim().to_string()
    } else {
        "".to_string()
    };
    let cycles = u32::from_str_radix(&cycles, 10).unwrap();

    // ASM is between index 16 and 48
    let mut full_asm = if line.len() >= 48 {
        line[16..48].trim().to_string()
    } else {
        "".to_string()
    };

    // if cycles == 9550 {
    //     println!("EQUAL {line}");
    // }

    full_asm = maybe_remove(&full_asm, '@');
    full_asm = maybe_remove(&full_asm, '=');
    full_asm = maybe_remove(&full_asm, '|');

    // Memory value sometimes appears in ASM part like "STX $00 = 00"
    let mut mem_value = None;
    let mut asm = full_asm.clone();
    if let Some(eq_pos) = full_asm.find(" = ") {
        mem_value = Some(full_asm[eq_pos + 3..].to_string());
        asm = full_asm[0..eq_pos].trim().to_string();
    }

    Ok(LogLine {
        line,
        pc,
        cycles,
        asm,
        _mem_value: mem_value,
        registers,
        v, h,
    })
}

fn maybe_remove(full_asm: &str, c: char) -> String {
    // if c == '='{
    //     println!("EQUAL");
    // }
    let mut result = full_asm;
    match full_asm.find(c) {
        None => {}
        Some(index) => {
            result = full_asm[0..index].trim().into();
        }
    }
    result.into()
}

fn parse_mesen_line(reader: &mut BufReader<File>, line_number: u64) -> Result<LogLine, LineStatus> {
    let mut line_read = String::new();
    reader.read_line(&mut line_read).unwrap();
    let line = line_read.clone();
    if line.len() == 0 {
        return Err(LineStatus::EOF);
    }
    if line.starts_with('#') {
        return Err(LineStatus::SKIP);
    }
    // println!("Parsing line:{line}");
    let mut i = 0;
    let s: Vec<char> = line.chars().collect();
    // "A01E  98050 JMP ($00D0) [$A021] = $60  A:A0 X:00 Y:01 S:F7 P:Nv--dIzc V:76  H:211"
    //     "E12C  97950 STA $51 = $00              A:07 X:00 Y:00 S:F9 P:nv--dIzc V:75  H:252 "
    //     // "EBA2  57187 STA $0000,Y [$0000] = $00  A:00 X:00 Y:00 S:F9 P:nv--dIZc V:241 H:41 "
    //     .chars().collect();

    while s[i] != ' ' {
        i += 1;
        if i >= s.len() {
            info!("About to crash on line#{line_number}:{line}");
            println!();
        }
    }
    let pc = s[0..i].iter().collect::<String>();
    // println!("pc: -{pc}-");
    while s[i] == ' ' { i += 1; }

    let j = i;
    while s[i] != ' ' { i += 1; }
    let cycles = s[j..i].iter().collect::<String>();
    let cycles = u32::from_str_radix(&cycles, 10).unwrap();
    // println!("cycles: -{cycles}-");

    let j = i + 1;
    while s[i] != ':' { i += 1; }
    let registers_start = i - 1;
    let full_asm = &s[j..registers_start].iter().collect::<String>();
    let mut asm = full_asm.clone();
    let mut read_value = None;
    if full_asm.contains(",") || full_asm.contains("JMP") {
        // Index case, e.g. STA $0000,Y [$0000] = $00
        if let Some(equals) = full_asm.find("=") {
            let mut index = equals - 1;
            while s[index] != ' ' { index -= 1 }
            index -= 1;
            while s[index] != ' ' { index -= 1 }
            asm = s[j..j + index].iter().collect::<String>();
            read_value = Some(full_asm[index + 1..].chars().collect::<String>());
        };
    } else if let Some(equals) = full_asm.find("=") {
        // Simple case, e.g. STA $51 = $00
        let index = equals - 1;
        asm = s[j..j + index].iter().collect::<String>();
        read_value = Some(full_asm[index + 1..].chars().collect::<String>());
    }

    // println!("asm:-{asm}-");
    // if let Some(read_value) = &read_value {
    //     println!("read_value:-{read_value}-");
    // }

    while i < s.len() && s[i] != 'V' { i += 1; }
    let registers = s[registers_start..i - 1].iter().collect::<String>();
    // println!("registers: -{registers}-");

    let j = i;
    while i < s.len() && s[i] != ' ' { i += 1; }
    let v = s[j..i].iter().collect::<String>();
    let v = u16::from_str_radix(&v, 16).unwrap();
    // println!("v: -{v}-");

    while i < s.len() && s[i] == ' ' { i += 1; }
    let j = i;
    while i < s.len() && s[i] != ' ' { i += 1; }
    let h = s[j..i].iter().collect::<String>();
    let h = u16::from_str_radix(&h, 16).unwrap();
    // println!("h: -{h}-");
    // let re = Regex::new(
    //     r"([0-9A-F]+)\s+(\d+)\s+([^=]+?)(?:\s*=\s*\$([0-9A-F]+))?\s+A:([0-9A-F]+)\s+X:([0-9A-F]+)\s+Y:([0-9A-F]+)\s+S:([0-9A-F]+)\s+P:(\S+)\s+V:(-?\d+)\s+H:(\d+)"
    // ).unwrap();

    Ok(LogLine {
        line, pc, cycles, asm: asm.trim().into(), _mem_value: read_value, registers, v, h
    })
}


fn lines_match(line1: &LogLine, line2: &LogLine) -> Option<Vec<String>> {
    let mut result: Vec<String> = Vec::new();
    if line1.pc != line2.pc {
        result.push(format!("Different PC: {} vs {}", line1.pc, line2.pc));
    }
    if line1.cycles != line2.cycles {
        result.push(format!("Different cycle: {:?} vs {:?}", line1.cycles, line2.cycles));
    }
    if line1.asm.to_ascii_lowercase() != line2.asm.to_ascii_lowercase() {
        result.push(format!("Different asm: {:?} vs {:?}", line1.asm, line2.asm));
    }
    // if line1.mem_value != line2.mem_value {
    //     result.push(format!("Different mem_value: {:?} vs {:?}", line1.mem_value, line2.mem_value;
    // }
    if line1.registers != line2.registers {
        result.push(format!("Different registers:\n{}\n{}", line1.registers, line2.registers));
    }
    if line1.v != line2.v {
        result.push(format!("Different V: {:?} vs {:?}", line1.v, line2.v));
    }
    if line1.h != line2.h {
        result.push(format!("Different H: {:?} vs {:?}", line1.h, line2.h));
    }

    if result.is_empty() { None } else { Some(result) }
}

pub fn compare_log(file_name_1: &str, file_name_2: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("Comparing files {} and {}", file_name_1, file_name_2);
    let file1 = File::open(file_name_1).expect(&format!("File {} should exist", file_name_1));
    let file2 = File::open(file_name_2).expect(&format!("File {} should exist", file_name_2));
    let mut reader1 = BufReader::new(file1);
    let mut reader2 = BufReader::new(file2);

    let mut line: u64 = 1;
    let mut stop = false;
    while ! stop {
        let line1 = parse_nestest_line(&mut reader1, line);
        let line2 = parse_cpu_trace_line(&mut reader2, line);
        match(line1, line2) {
            (Ok(line1), Ok(line2)) => {
                if let Some(error_message) = lines_match(&line1, &line2) {
                    println!("Line {line:#?} doesn't match");
                    print!("{}", line1.line);
                    print!("{}", line2.line);
                    for e in error_message {
                        println!("{e}");
                    }
                    stop = true;
                }
            }
            (Err(e1), _) => {
                println!("{line}: Error from file 1: {e1:#?}");
                stop = true;
            }
            (_, Err(e2)) => {
                println!("{line}: Error from file 2: {e2:#?}");
                stop = true;
            }
        }

        line += 1;
    }

    Ok(())
}