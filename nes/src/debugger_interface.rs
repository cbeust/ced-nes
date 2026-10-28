use serde::{Deserialize, Serialize};

/// /set_memories
#[derive(Deserialize, Serialize)]
pub(crate) struct SetMemoriesJson {
    pub(crate) memories: Vec<SetMemoryJson>,
}

#[derive(Deserialize, Serialize)]
#[derive(Clone)]
pub(crate) struct SetMemoryJson {
    pub(crate) name: String,
    pub(crate) content: Vec<u8>,
}

#[derive(Clone, Deserialize, Debug, PartialEq, Serialize)]
pub(crate) struct SetRegisterJson {
    pub(crate) name: String,
    pub(crate) value: String,
}

#[derive(Clone, Default, Deserialize, Debug, PartialEq, Serialize)]
pub(crate) struct SetRegistersJson {
    pub(crate) registers: Vec<SetRegisterJson>,
}
