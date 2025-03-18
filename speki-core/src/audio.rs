
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, Hash)]
pub struct Audio {
    pub id: Uuid,
    pub data: Vec<u8>,
}

impl Audio {
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            id: AudioId::new_v4(),
            data,
        }
    }
}

pub type AudioId = Uuid;
