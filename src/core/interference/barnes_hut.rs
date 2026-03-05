use crate::core::CoreWave;
use std::sync::Arc;

/// Represents a bounding box in 3D space
#[derive(Clone, Debug, Copy)]
pub struct BoundingBox {
    pub center: [f32; 3],
    pub half_size: [f32; 3], // Half-width, half-height, half-depth
}

impl BoundingBox {
    pub fn new(center: [f32; 3], half_size: [f32; 3]) -> Self {
        Self { center, half_size }
    }

    pub fn contains(&self, point: [f32; 3]) -> bool {
        (point[0] >= self.center[0] - self.half_size[0]) &&
        (point[0] <= self.center[0] + self.half_size[0]) &&
        (point[1] >= self.center[1] - self.half_size[1]) &&
        (point[1] <= self.center[1] + self.half_size[1]) &&
        (point[2] >= self.center[2] - self.half_size[2]) &&
        (point[2] <= self.center[2] + self.half_size[2])
    }

    pub fn intersects(&self, other: &BoundingBox) -> bool {
        (self.center[0] - self.half_size[0] <= other.center[0] + other.half_size[0]) &&
        (self.center[0] + self.half_size[0] >= other.center[0] - other.half_size[0]) &&
        (self.center[1] - self.half_size[1] <= other.center[1] + other.half_size[1]) &&
        (self.center[1] + self.half_size[1] >= other.center[1] - other.half_size[1]) &&
        (self.center[2] - self.half_size[2] <= other.center[2] + other.half_size[2]) &&
        (self.center[2] + self.half_size[2] >= other.center[2] - other.half_size[2])
    }
}

/// A node in the Octree
#[derive(Debug)]
pub enum OctreeNode {
    Leaf(Vec<(usize, [f32; 3])>), // Stores (index, position)
    Internal(Box<[Option<OctreeNode>; 8]>),
}

/// The Octree structure
#[derive(Debug)]
pub struct Octree {
    root: OctreeNode,
    bounds: BoundingBox,
    capacity: usize, // Max items per leaf before splitting
    max_depth: usize,
}

impl Octree {
    pub fn new(bounds: BoundingBox, capacity: usize, max_depth: usize) -> Self {
        Self {
            root: OctreeNode::Leaf(Vec::with_capacity(capacity)),
            bounds,
            capacity,
            max_depth,
        }
    }

    pub fn insert(&mut self, wave_idx: usize, position: [f32; 3]) -> bool {
        if !self.bounds.contains(position) {
            return false;
        }
        Self::insert_recursive(&mut self.root, wave_idx, position, &self.bounds, self.capacity, 0, self.max_depth)
    }

    fn insert_recursive(
        node: &mut OctreeNode,
        wave_idx: usize,
        position: [f32; 3],
        bounds: &BoundingBox,
        capacity: usize,
        current_depth: usize,
        max_depth: usize,
    ) -> bool {
        // We need to temporarily take ownership of the node to mutate it (specifically for splitting)
        // But since we are using a mutable reference, we can swap.
        
        let needs_split = match node {
            OctreeNode::Leaf(items) => {
                if items.len() < capacity || current_depth >= max_depth {
                    items.push((wave_idx, position));
                    return true;
                }
                true // Needs split
            }
            OctreeNode::Internal(children) => {
                let octant = Self::get_octant(position, bounds);
                if children[octant].is_none() {
                    let _child_bounds = Self::get_child_bounds(bounds, octant);
                    children[octant] = Some(OctreeNode::Leaf(Vec::with_capacity(capacity)));
                }
                if let Some(child) = &mut children[octant] {
                    let child_bounds = Self::get_child_bounds(bounds, octant);
                    return Self::insert_recursive(child, wave_idx, position, &child_bounds, capacity, current_depth + 1, max_depth);
                } else {
                    unreachable!()
                }
            }
        };

        if needs_split {
            // Perform split
            let items = if let OctreeNode::Leaf(items) = std::mem::replace(node, OctreeNode::Internal(Box::new([None, None, None, None, None, None, None, None]))) {
                items
            } else {
                unreachable!()
            };

            // Re-insert existing items
            if let OctreeNode::Internal(children) = node {
                for (idx, pos) in items {
                    let octant = Self::get_octant(pos, bounds);
                    if children[octant].is_none() {
                         children[octant] = Some(OctreeNode::Leaf(Vec::with_capacity(capacity)));
                    }
                    if let Some(child) = &mut children[octant] {
                        let child_bounds = Self::get_child_bounds(bounds, octant);
                        Self::insert_recursive(child, idx, pos, &child_bounds, capacity, current_depth + 1, max_depth);
                    }
                }
                
                // Insert the new item
                let octant = Self::get_octant(position, bounds);
                if children[octant].is_none() {
                     children[octant] = Some(OctreeNode::Leaf(Vec::with_capacity(capacity)));
                }
                if let Some(child) = &mut children[octant] {
                    let child_bounds = Self::get_child_bounds(bounds, octant);
                    Self::insert_recursive(child, wave_idx, position, &child_bounds, capacity, current_depth + 1, max_depth);
                }
            }
        }
        true
    }

    pub fn query_range(&self, range: &BoundingBox) -> Vec<usize> {
        let mut results = Vec::new();
        Self::query_recursive(&self.root, &self.bounds, range, &mut results);
        results
    }

    fn query_recursive(node: &OctreeNode, node_bounds: &BoundingBox, range: &BoundingBox, results: &mut Vec<usize>) {
        if !node_bounds.intersects(range) {
            return;
        }

        match node {
            OctreeNode::Leaf(items) => {
                for (idx, pos) in items {
                    if range.contains(*pos) {
                        results.push(*idx);
                    }
                }
            }
            OctreeNode::Internal(children) => {
                for (i, child) in children.iter().enumerate() {
                    if let Some(child_node) = child {
                        let child_bounds = Self::get_child_bounds(node_bounds, i);
                        Self::query_recursive(child_node, &child_bounds, range, results);
                    }
                }
            }
        }
    }

    fn get_octant(point: [f32; 3], bounds: &BoundingBox) -> usize {
        let mut octant = 0;
        if point[0] >= bounds.center[0] { octant |= 1; }
        if point[1] >= bounds.center[1] { octant |= 2; }
        if point[2] >= bounds.center[2] { octant |= 4; }
        octant
    }

    fn get_child_bounds(parent_bounds: &BoundingBox, octant: usize) -> BoundingBox {
        let half = [
            parent_bounds.half_size[0] * 0.5,
            parent_bounds.half_size[1] * 0.5,
            parent_bounds.half_size[2] * 0.5,
        ];
        let mut center = parent_bounds.center;
        
        if (octant & 1) != 0 { center[0] += half[0]; } else { center[0] -= half[0]; }
        if (octant & 2) != 0 { center[1] += half[1]; } else { center[1] -= half[1]; }
        if (octant & 4) != 0 { center[2] += half[2]; } else { center[2] -= half[2]; }

        BoundingBox { center, half_size: half }
    }
}



