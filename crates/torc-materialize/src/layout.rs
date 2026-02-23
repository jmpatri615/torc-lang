//! Memory layout estimation: type sizes, stack frames, code size heuristics.

use torc_core::graph::node::NodeId;
use torc_core::graph::Graph;
use torc_core::types::{FloatPrecision, Type};
use torc_targets::Platform;

use crate::error::MaterializationError;

/// Estimated size and alignment of a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeSize {
    /// Size in bytes.
    pub size_bytes: u64,
    /// Required alignment in bytes.
    pub alignment_bytes: u64,
}

/// Estimated stack frame for a single node.
#[derive(Debug, Clone)]
pub struct FrameEstimate {
    /// The node this frame belongs to.
    pub node_id: NodeId,
    /// Total input size in bytes.
    pub input_bytes: u64,
    /// Total output size in bytes.
    pub output_bytes: u64,
    /// Total frame size (inputs + outputs + overhead).
    pub total_bytes: u64,
}

/// Complete memory layout estimate for a graph.
#[derive(Debug, Clone)]
pub struct MemoryLayout {
    /// Per-node stack frame estimates.
    pub frames: Vec<FrameEstimate>,
    /// Peak stack usage in bytes (sum of all live frames on the critical path).
    pub peak_stack_bytes: u64,
    /// Static data size estimate in bytes.
    pub static_data_bytes: u64,
    /// Estimated code size in bytes.
    pub estimated_code_bytes: u64,
}

/// Estimate the size and alignment of a type for a given platform.
///
/// Returns `None` for dynamically-sized types (Vec, Distribution, etc.).
/// Follows C-like ABI layout rules: natural alignment, pointer = word size.
pub fn estimate_type_size(ty: &Type, platform: &Platform) -> Option<TypeSize> {
    let word = platform.word_size_bytes() as u64;

    match ty {
        Type::Void => Some(TypeSize {
            size_bytes: 0,
            alignment_bytes: 1,
        }),
        Type::Unit => Some(TypeSize {
            size_bytes: 0,
            alignment_bytes: 1,
        }),
        Type::Bool => Some(TypeSize {
            size_bytes: 1,
            alignment_bytes: 1,
        }),
        Type::Int { width, .. } => {
            let bytes = (*width as u64).div_ceil(8);
            let align = bytes.min(word); // natural alignment, capped at word
            Some(TypeSize {
                size_bytes: bytes,
                alignment_bytes: align,
            })
        }
        Type::Float { precision } => {
            let bytes = match precision {
                FloatPrecision::F16 => 2,
                FloatPrecision::F32 => 4,
                FloatPrecision::F64 => 8,
                FloatPrecision::F128 => 16,
            };
            let align = bytes.min(word);
            Some(TypeSize {
                size_bytes: bytes,
                alignment_bytes: align,
            })
        }
        Type::Fixed {
            total_bits,
            frac_bits: _,
        } => {
            let bytes = (*total_bits as u64).div_ceil(8);
            let align = bytes.min(word);
            Some(TypeSize {
                size_bytes: bytes,
                alignment_bytes: align,
            })
        }
        Type::Tuple(fields) => {
            let mut size: u64 = 0;
            let mut max_align: u64 = 1;
            for field in fields {
                let fs = estimate_type_size(field, platform)?;
                // Align to field alignment
                size = align_up(size, fs.alignment_bytes);
                size += fs.size_bytes;
                max_align = max_align.max(fs.alignment_bytes);
            }
            // Pad to struct alignment
            size = align_up(size, max_align);
            Some(TypeSize {
                size_bytes: size,
                alignment_bytes: max_align,
            })
        }
        Type::Record(fields) => {
            let mut size: u64 = 0;
            let mut max_align: u64 = 1;
            for field_ty in fields.values() {
                let fs = estimate_type_size(field_ty, platform)?;
                size = align_up(size, fs.alignment_bytes);
                size += fs.size_bytes;
                max_align = max_align.max(fs.alignment_bytes);
            }
            size = align_up(size, max_align);
            Some(TypeSize {
                size_bytes: size,
                alignment_bytes: max_align,
            })
        }
        Type::Variant(cases) => {
            // Tag + max(case sizes), padded to overall alignment
            let tag_size = if cases.len() <= 256 { 1u64 } else { 4 };
            let mut max_size: u64 = 0;
            let mut max_align: u64 = tag_size.min(word);
            for case_ty in cases.values() {
                if let Some(cs) = estimate_type_size(case_ty, platform) {
                    max_size = max_size.max(cs.size_bytes);
                    max_align = max_align.max(cs.alignment_bytes);
                } else {
                    return None;
                }
            }
            let total = align_up(tag_size + max_size, max_align);
            Some(TypeSize {
                size_bytes: total,
                alignment_bytes: max_align,
            })
        }
        Type::Array { element, length } => {
            let es = estimate_type_size(element, platform)?;
            let elem_stride = align_up(es.size_bytes, es.alignment_bytes);
            Some(TypeSize {
                size_bytes: elem_stride * (*length as u64),
                alignment_bytes: es.alignment_bytes,
            })
        }
        // Dynamically-sized types
        Type::Vec { .. } | Type::Distribution(_) => None,

        // Wrapper types: delegate to inner
        Type::Refined { base, .. }
        | Type::Linear { inner: base, .. }
        | Type::Timed { inner: base, .. }
        | Type::Sized { inner: base, .. }
        | Type::Powered { inner: base, .. }
        | Type::Bandwidth { inner: base, .. }
        | Type::Posterior { inner: base, .. }
        | Type::Interval { inner: base, .. }
        | Type::Approximate { inner: base, .. } => estimate_type_size(base, platform),

        Type::Option(inner) => {
            // 1-byte discriminant + inner, padded to overall alignment
            let is = estimate_type_size(inner, platform)?;
            let align = is.alignment_bytes.max(1);
            let size = align_up(1 + is.size_bytes, align);
            Some(TypeSize {
                size_bytes: size,
                alignment_bytes: align,
            })
        }

        // Named types and parameterized types can't be sized without resolution
        Type::Named(_) | Type::Parameterized { .. } => None,
    }
}

/// Estimate the memory layout for all nodes in a graph.
pub fn estimate_layout(
    graph: &Graph,
    platform: &Platform,
) -> Result<MemoryLayout, MaterializationError> {
    let mut frames = Vec::new();
    let mut frame_map = std::collections::HashMap::new();
    let mut static_data_bytes: u64 = 0;

    for node in graph.nodes() {
        let (input_bytes, output_bytes) = if let Some(ref sig) = node.type_signature {
            let inputs: u64 = sig
                .inputs
                .iter()
                .filter_map(|t| estimate_type_size(t, platform))
                .map(|ts| ts.size_bytes)
                .sum();
            let outputs: u64 = sig
                .outputs
                .iter()
                .filter_map(|t| estimate_type_size(t, platform))
                .map(|ts| ts.size_bytes)
                .sum();
            (inputs, outputs)
        } else {
            (0, 0)
        };

        // Overhead for frame bookkeeping (return address, saved registers)
        let overhead = platform.word_size_bytes() as u64 * 2;
        let total = input_bytes + output_bytes + overhead;

        frame_map.insert(node.id, total);
        frames.push(FrameEstimate {
            node_id: node.id,
            input_bytes,
            output_bytes,
            total_bytes: total,
        });

        // Literal nodes contribute to static data
        if node.kind == torc_core::graph::node::NodeKind::Literal {
            static_data_bytes += output_bytes;
        }
    }

    // Peak stack estimate: compute the maximum cumulative frame size along
    // any path from a root to a leaf (longest-path in terms of bytes).
    // This is the worst-case stack depth assuming sequential execution.
    let peak_stack_bytes = compute_peak_stack(graph, &frame_map);

    // Code size heuristic: per-node instruction estimate * platform multiplier
    let code_multiplier = if platform.isa.word_size == 32 {
        4u64
    } else {
        8
    };
    let instructions_per_node = 10u64; // heuristic
    let estimated_code_bytes = graph.node_count() as u64 * instructions_per_node * code_multiplier;

    Ok(MemoryLayout {
        frames,
        peak_stack_bytes,
        static_data_bytes,
        estimated_code_bytes,
    })
}

/// Compute peak stack usage as the heaviest path (in frame bytes) through the graph.
fn compute_peak_stack(graph: &Graph, frame_map: &std::collections::HashMap<NodeId, u64>) -> u64 {
    // Use topological order and dynamic programming to find heaviest path
    let sorted = match graph.topological_sort() {
        Ok(s) => s,
        Err(_) => return frame_map.values().sum(), // fallback for cycles
    };

    if sorted.is_empty() {
        return 0;
    }

    // longest_path[node] = max cumulative frame bytes from any root to this node
    let mut longest_path: std::collections::HashMap<NodeId, u64> = std::collections::HashMap::new();

    for &node_id in &sorted {
        let own_frame = frame_map.get(&node_id).copied().unwrap_or(0);
        let max_predecessor = graph
            .incoming_edges(&node_id)
            .iter()
            .filter_map(|eid| {
                let edge = graph.get_edge(eid)?;
                longest_path.get(&edge.source.0).copied()
            })
            .max()
            .unwrap_or(0);
        longest_path.insert(node_id, max_predecessor + own_frame);
    }

    longest_path.values().copied().max().unwrap_or(0)
}

/// Round `value` up to the next multiple of `align`.
fn align_up(value: u64, align: u64) -> u64 {
    if align == 0 {
        return value;
    }
    value.div_ceil(align) * align
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::graph::edge::Edge;
    use torc_core::graph::node::{ArithmeticOp, Node, NodeKind};
    use torc_core::types::TypeSignature;

    #[test]
    fn primitive_type_sizes() {
        let platform = Platform::generic_linux_x86_64();
        assert_eq!(
            estimate_type_size(&Type::Bool, &platform),
            Some(TypeSize {
                size_bytes: 1,
                alignment_bytes: 1
            })
        );
        assert_eq!(
            estimate_type_size(&Type::i32(), &platform),
            Some(TypeSize {
                size_bytes: 4,
                alignment_bytes: 4
            })
        );
        assert_eq!(
            estimate_type_size(
                &Type::Float {
                    precision: FloatPrecision::F64
                },
                &platform,
            ),
            Some(TypeSize {
                size_bytes: 8,
                alignment_bytes: 8
            })
        );
    }

    #[test]
    fn tuple_type_size() {
        let platform = Platform::generic_linux_x86_64();
        let ty = Type::Tuple(vec![Type::Bool, Type::i32()]);
        let size = estimate_type_size(&ty, &platform).unwrap();
        // bool(1) + padding(3) + i32(4) = 8
        assert_eq!(size.size_bytes, 8);
        assert_eq!(size.alignment_bytes, 4);
    }

    #[test]
    fn array_type_size() {
        let platform = Platform::generic_linux_x86_64();
        let ty = Type::Array {
            element: Box::new(Type::i32()),
            length: 10,
        };
        let size = estimate_type_size(&ty, &platform).unwrap();
        assert_eq!(size.size_bytes, 40); // 4 * 10
    }

    #[test]
    fn dynamic_types_return_none() {
        let platform = Platform::generic_linux_x86_64();
        assert!(estimate_type_size(
            &Type::Vec {
                element: Box::new(Type::i32())
            },
            &platform
        )
        .is_none());
        assert!(
            estimate_type_size(&Type::Distribution(Box::new(Type::f32())), &platform).is_none()
        );
    }

    #[test]
    fn layout_estimation() {
        let mut g = Graph::new();
        let n1 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        let n2 = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        let id1 = g.add_node(n1).unwrap();
        let id2 = g.add_node(n2).unwrap();
        g.add_edge(Edge::typed((id1, 0), (id2, 0), Type::i32()))
            .unwrap();

        let platform = Platform::generic_linux_x86_64();
        let layout = estimate_layout(&g, &platform).unwrap();
        assert_eq!(layout.frames.len(), 2);
        assert!(layout.peak_stack_bytes > 0);
        assert!(layout.estimated_code_bytes > 0);
        assert!(layout.static_data_bytes > 0); // Literal node
    }

    #[test]
    fn arm_platform_sizes() {
        let platform = Platform::stm32f407_discovery();
        let size = estimate_type_size(&Type::i32(), &platform).unwrap();
        assert_eq!(size.size_bytes, 4);
        assert_eq!(size.alignment_bytes, 4);

        // Code size uses 4-byte multiplier for 32-bit
        let mut g = Graph::new();
        g.add_node(Node::new(NodeKind::Literal)).unwrap();
        let layout = estimate_layout(&g, &platform).unwrap();
        assert_eq!(layout.estimated_code_bytes, 10 * 4); // 1 node * 10 insns * 4 bytes
    }
}
