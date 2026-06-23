use crate::constants::RomInfo;
use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;
use std::fs::File;
use std::io::{BufReader, Read};
use tracing::{debug};
use walkdir::WalkDir;

/// Return all the .nes files using the provided mappers
pub fn find_roms_with_mappers(path: &str, mappers: Vec<u8>) -> Vec<RomInfo> {
    let entries: Vec<_> = WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .collect();

    let mut result: Vec<RomInfo> = entries
        .into_par_iter()
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension() == Some(std::ffi::OsStr::new("nes")))
        .filter(|e| mappers.contains(&mapper_number(e.path().to_str().unwrap())))
        .map(|e| RomInfo::n(0, e.path().to_str().unwrap())).collect();

    let mut id = 100;
    for ri in &mut result {
        ri.id = id;
        id += 1;
    }

    debug!("Found {} ROMs for mappers [{}]", result.len(),
        mappers.iter().map(|m| format!("{}", m)).collect::<Vec<String>>().join(", "));
    result
}

/// Extract the mapper number from the .nes file
pub fn mapper_number(path: &str) -> u8 {
    match check_first_bytes(path, 8) {
        Ok(bytes) => {
            if bytes.len() >= 8 {
                (bytes[6] & 0xf0) >> 4 | bytes[7] & 0xf0
            } else {
                // Default to mapper 0 if file is too small
                0
            }
        }
        Err(_) => {
            // Default to mapper 0 if file cannot be read
            0
        }
    }
}

/// Return the furst `num_bytes` of the file
fn check_first_bytes(path: &str, num_bytes: usize) -> std::io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut buffer = vec![0u8; num_bytes];
    let bytes_read = reader.read(&mut buffer)?;
    buffer.truncate(bytes_read);
    Ok(buffer)
}

