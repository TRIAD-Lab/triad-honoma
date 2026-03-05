use blake3::Hasher;

/// Position of the sibling node inside a proof step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofPosition {
    Left,
    Right,
}

/// Node inside a Merkle/MMR proof describing sibling hash and its position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProofNode {
    pub sibling: [u8; 32],
    pub position: ProofPosition,
}

impl ProofNode {
    #[inline]
    pub fn new(sibling: [u8; 32], position: ProofPosition) -> Self {
        Self { sibling, position }
    }
}

#[inline(always)]
pub fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(b"triad.merkle.node");
    hasher.update(left);
    hasher.update(right);
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_bytes());
    out
}

/// Fold proof nodes over a leaf hash and return the resulting root.
pub fn fold_proof(leaf: &[u8; 32], proof: &[ProofNode]) -> [u8; 32] {
    let mut acc = *leaf;
    for node in proof {
        match node.position {
            ProofPosition::Left => {
                acc = hash_pair(&node.sibling, &acc);
            }
            ProofPosition::Right => {
                acc = hash_pair(&acc, &node.sibling);
            }
        }
    }
    acc
}


