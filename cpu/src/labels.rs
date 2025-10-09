use std::collections::HashMap;
use std::fs;
use std::io;
use tracing::info;

/// Labels struct that contains a HashMap mapping u16 addresses to String labels
#[derive(Clone, Debug, Default)]
pub struct Labels {
    map: HashMap<u16, String>,
}

impl Labels {
    /// Read the AccuracyCoins.fns file and create a Labels instance
    /// Format expected: "LabelName=0xAddress" on each line
    pub fn from_file(filename: &str) -> io::Result<Self> {
        let content = fs::read_to_string(filename)?;
        let mut map = HashMap::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue; // Skip empty lines and comments
            }

            if let Some(pos) = line.find('=') {
                let label = line[..pos].trim().to_string();
                let address_str = line[pos + 1..].trim();
                
                // Parse hex address (with or without 0x prefix)
                let address = if address_str.starts_with("0x") || address_str.starts_with("0X") {
                    u16::from_str_radix(&address_str[2..], 16)
                } else if address_str.starts_with("$") {
                    u16::from_str_radix(&address_str[1..], 16)
                } else {
                    u16::from_str_radix(address_str, 16)
                };

                match address {
                    Ok(addr) => {
                        map.insert(addr, label);
                    }
                    Err(_) => {
                        eprintln!("Warning: Could not parse address '{}' for label '{}'", address_str, label);
                    }
                }
            }
        }

        Ok(Labels { map })
    }

    /// Get a label by address
    pub fn get(&self, address: &u16) -> Option<&String> {
        self.map.get(address)
    }

    /// Insert a new label
    pub fn insert(&mut self, address: u16, label: String) -> Option<String> {
        self.map.insert(address, label)
    }

    /// Get the underlying HashMap
    pub fn as_hashmap(&self) -> &HashMap<u16, String> {
        &self.map
    }

    /// Convert into the underlying HashMap
    pub fn into_hashmap(self) -> HashMap<u16, String> {
        self.map
    }
}

// Implement Iterator traits to make it compatible with existing code
impl IntoIterator for Labels {
    type Item = (u16, String);
    type IntoIter = std::collections::hash_map::IntoIter<u16, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.map.into_iter()
    }
}

impl<'a> IntoIterator for &'a Labels {
    type Item = (&'a u16, &'a String);
    type IntoIter = std::collections::hash_map::Iter<'a, u16, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.map.iter()
    }
}

// Implement FromIterator to allow collecting into Labels
impl FromIterator<(u16, String)> for Labels {
    fn from_iter<T: IntoIterator<Item = (u16, String)>>(iter: T) -> Self {
        Labels {
            map: HashMap::from_iter(iter),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_labels_from_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "StartLabel=0x8000").unwrap();
        writeln!(temp_file, "MainLoop=0x8010").unwrap();
        writeln!(temp_file, "# This is a comment").unwrap();
        writeln!(temp_file, "").unwrap(); // empty line
        writeln!(temp_file, "EndLabel=FFFF").unwrap(); // without 0x prefix
        temp_file.flush().unwrap();

        let labels = Labels::from_file(temp_file.path().to_str().unwrap()).unwrap();
        
        assert_eq!(labels.get(&0x8000), Some(&"StartLabel".to_string()));
        assert_eq!(labels.get(&0x8010), Some(&"MainLoop".to_string()));
        assert_eq!(labels.get(&0xFFFF), Some(&"EndLabel".to_string()));
        assert_eq!(labels.get(&0x1234), None);
    }

    #[test]
    fn test_labels_compatibility() {
        // Test that Labels can be used like the existing HashMap<u16, String>
        let labels: Labels = [(0x3f0, "SomeLabel".into()), (0x8045, "SomeBranch".into())]
            .into_iter().collect();
        
        assert_eq!(labels.get(&0x3f0), Some(&"SomeLabel".to_string()));
        assert_eq!(labels.get(&0x8045), Some(&"SomeBranch".to_string()));
    }
}