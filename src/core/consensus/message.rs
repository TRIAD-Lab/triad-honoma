use serde::{Serialize, Deserialize};
use bytes::Bytes;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusMessage {
    pub sender_id: String,
    pub state_id: String,
    pub raw_data: Bytes,
    pub signature: Vec<u8>,  // PHASE 2: 64-byte Schnorr signature (Vec for serde compat)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<Vec<u8>>, // PublicKey as bytes (32 bytes for Ristretto255)
}

impl ConsensusMessage {
    /// Zero-Copy serialization using postcard
    /// Returns the number of bytes written to the buffer
    pub fn to_bytes_zero_copy(&self, buffer: &mut [u8]) -> Result<usize, postcard::Error> {
        let slice = postcard::to_slice(self, buffer)?;
        Ok(slice.len())
    }

    /// Zero-Copy deserialization using postcard
    pub fn from_bytes_zero_copy(buffer: &[u8]) -> Result<Self, postcard::Error> {
        postcard::from_bytes(buffer)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConsensusState {
    pub current_round: u64,
    pub votes: Vec<ConsensusVote>,
    pub threshold: f64,
    pub status: ConsensusStatus,
}

#[derive(Debug, Clone)]
pub struct ConsensusVote {
    pub node_id: String,
    pub vote: bool,
    pub timestamp: std::time::SystemTime,
}

#[derive(Debug, Clone)]
pub enum ConsensusStatus {
    Pending,
    Committed,
    Aborted,
}

impl Default for ConsensusStatus {
    fn default() -> Self {
        Self::Pending
    }
}


