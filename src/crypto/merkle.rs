use blake3::Hash;
use rayon::prelude::*;


/// A parallelized Merkle Tree implementation using Blake3.
#[derive(Debug, Clone)]
pub struct MerkleTree {
    pub leaves: Vec<Hash>,
    pub layers: Vec<Vec<Hash>>,
    pub root: Hash,
}

impl MerkleTree {
    /// Constructs a Merkle Tree from a slice of data items.
    /// Uses Rayon for parallel hashing of leaves and tree construction.
    pub fn new(data: &[&[u8]]) -> Self {
        if data.is_empty() {
            let empty_hash = blake3::hash(&[]);
            return Self {
                leaves: vec![],
                layers: vec![],
                root: empty_hash,
            };
        }

        // 1. Hash leaves in parallel
        let leaves: Vec<Hash> = data
            .par_iter()
            .map(|item| blake3::hash(item))
            .collect();

        Self::from_leaves(leaves)
    }

    /// Constructs a Merkle Tree from pre-hashed leaves.
    pub fn from_leaves(leaves: Vec<Hash>) -> Self {
        if leaves.is_empty() {
             let empty_hash = blake3::hash(&[]);
            return Self {
                leaves: vec![],
                layers: vec![],
                root: empty_hash,
            };
        }

        let mut layers = Vec::new();
        let mut current_layer = leaves.clone();

        while current_layer.len() > 1 {
            // 2. Build next layer in parallel
            let next_layer: Vec<Hash> = current_layer
                .par_chunks(2)
                .map(|chunk| {
                    if chunk.len() == 2 {
                        // Standard node: Hash(Left || Right)
                        let mut hasher = blake3::Hasher::new();
                        hasher.update(chunk[0].as_bytes());
                        hasher.update(chunk[1].as_bytes());
                        hasher.finalize()
                    } else {
                        // Odd node: Duplicate last item (or just lift it up)
                        // Bitcoin style: duplicate. Ethereum style: lift.
                        // We'll duplicate for simplicity and standard compliance in some systems.
                        let mut hasher = blake3::Hasher::new();
                        hasher.update(chunk[0].as_bytes());
                        hasher.update(chunk[0].as_bytes());
                        hasher.finalize()
                    }
                })
                .collect();

            layers.push(current_layer);
            current_layer = next_layer;
        }

        let root = current_layer[0];
        layers.push(current_layer); // Push root layer

        Self {
            leaves,
            layers,
            root,
        }
    }

    pub fn root(&self) -> Hash {
        self.root
    }

    /// Generates a Merkle Proof for a given leaf index.
    pub fn generate_proof(&self, index: usize) -> Option<Vec<Hash>> {
        if index >= self.leaves.len() {
            return None;
        }

        let mut proof = Vec::new();
        let mut current_index = index;

        // Iterate through layers (excluding the root layer)
        for layer in self.layers.iter().take(self.layers.len() - 1) {
            let is_right_node = current_index % 2 == 1;
            let sibling_index = if is_right_node {
                current_index - 1
            } else {
                current_index + 1
            };

            if sibling_index < layer.len() {
                proof.push(layer[sibling_index]);
            } else {
                // If sibling doesn't exist (odd number of nodes), we duplicated the node in construction.
                // So the sibling is effectively itself.
                proof.push(layer[current_index]);
            }

            current_index /= 2;
        }

        Some(proof)
    }

    /// Verifies a Merkle Proof.
    pub fn verify(root: Hash, leaf: &[u8], proof: &[Hash], index: usize) -> bool {
        let mut current_hash = blake3::hash(leaf);
        let mut current_index = index;

        for sibling in proof {
            let mut hasher = blake3::Hasher::new();
            if current_index % 2 == 0 {
                // Current is left, sibling is right
                hasher.update(current_hash.as_bytes());
                hasher.update(sibling.as_bytes());
            } else {
                // Current is right, sibling is left
                hasher.update(sibling.as_bytes());
                hasher.update(current_hash.as_bytes());
            }
            current_hash = hasher.finalize();
            current_index /= 2;
        }

        current_hash == root
    }
}


