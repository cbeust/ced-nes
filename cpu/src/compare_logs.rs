use std::env::home_dir;
use std::fs::File;
use std::io::{BufRead, BufReader};
use tracing::info;

fn _main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = [home_dir().unwrap().to_str().unwrap(), "t".into()].join("/");
    let file1 = &format!("{dir}/trace-mesen.txt");
    let file2 = &format!("{dir}/trace-new-cpu.txt");
    compare_log(file1, file2)
}

#[derive(Debug)]
struct MesenLogLine {
    line: String,
    pc: String,
    cycles: String,
    asm: String,
    _mem_value: Option<String>,
    registers: String,
    v: String,
    h: String,
}

#[derive(Debug)]
enum LineStatus {
    EOF,
    SKIP,
}

fn parse_mesen_line(reader: &mut BufReader<File>, line_number: u64) -> Result<MesenLogLine, LineStatus> {
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

    while s[i] != 'V' { i += 1; }
    let registers = s[registers_start..i - 1].iter().collect::<String>();
    // println!("registers: -{registers}-");

    let j = i;
    while s[i] != ' ' { i += 1; }
    let v = s[j..i].iter().collect::<String>();
    // println!("v: -{v}-");

    while s[i] == ' ' { i += 1; }
    let j = i;
    while i < s.len() && s[i] != ' ' { i += 1; }
    let h = s[j..i].iter().collect::<String>();
    // println!("h: -{h}-");
    // let re = Regex::new(
    //     r"([0-9A-F]+)\s+(\d+)\s+([^=]+?)(?:\s*=\s*\$([0-9A-F]+))?\s+A:([0-9A-F]+)\s+X:([0-9A-F]+)\s+Y:([0-9A-F]+)\s+S:([0-9A-F]+)\s+P:(\S+)\s+V:(-?\d+)\s+H:(\d+)"
    // ).unwrap();

    Ok(MesenLogLine {
        line, pc, cycles, asm: asm.trim().into(), _mem_value: read_value, registers, v, h
    })
}


fn lines_match(line1: &MesenLogLine, line2: &MesenLogLine) -> Option<Vec<String>> {
    let mut result: Vec<String> = Vec::new();
    if line1.pc != line2.pc {
        result.push(format!("Different PC: {} vs {}", line1.pc, line2.pc));
    }
    if line1.cycles != line2.cycles {
        result.push(format!("Different cycle: {:?} vs {:?}", line1.cycles, line2.cycles));
    }
    if line1.asm != line2.asm {
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
    let file1 = File::open(file_name_1).expect(&format!("Open file {} failed", file_name_1));
    let file2 = File::open(file_name_2).expect(&format!("Open file {} failed", file_name_2));
    let mut reader1 = BufReader::new(file1);
    let mut reader2 = BufReader::new(file2);

    let mut line: u64 = 1;
    let mut stop = false;
    while ! stop {
        let line1 = parse_mesen_line(&mut reader1, line);
        let line2 = parse_mesen_line(&mut reader2, line);
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