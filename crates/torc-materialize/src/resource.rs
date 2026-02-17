//! Resource fitting: check whether estimated resource usage fits the target.

use std::fmt;

use torc_targets::Platform;

use crate::error::MaterializationError;
use crate::layout::MemoryLayout;

/// Usage of a single resource.
#[derive(Debug, Clone)]
pub struct ResourceUsage {
    /// Resource name (e.g., "flash", "ram", "stack").
    pub name: String,
    /// Bytes used.
    pub used: u64,
    /// Bytes available.
    pub available: u64,
    /// Usage as a percentage.
    pub percent: f64,
}

impl fmt::Display for ResourceUsage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {}/{} bytes ({:.1}%)",
            self.name, self.used, self.available, self.percent
        )
    }
}

/// Complete resource fitting report.
#[derive(Debug, Clone)]
pub struct ResourceReport {
    /// Flash/ROM usage.
    pub flash: ResourceUsage,
    /// RAM usage.
    pub ram: ResourceUsage,
    /// Stack usage (if stack size is constrained).
    pub stack: Option<ResourceUsage>,
    /// Whether all resources fit within constraints.
    pub all_fit: bool,
    /// Human-readable descriptions of any violations.
    pub violations: Vec<String>,
}

impl fmt::Display for ResourceReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== Resource Report ===")?;
        writeln!(f, "  {}", self.flash)?;
        writeln!(f, "  {}", self.ram)?;
        if let Some(ref stack) = self.stack {
            writeln!(f, "  {}", stack)?;
        }
        if self.all_fit {
            writeln!(f, "  Status: ALL FIT")?;
        } else {
            writeln!(f, "  Status: VIOLATIONS")?;
            for v in &self.violations {
                writeln!(f, "    - {v}")?;
            }
        }
        Ok(())
    }
}

/// Check whether the estimated memory layout fits the target platform's resources.
pub fn check_resource_fit(layout: &MemoryLayout, platform: &Platform) -> ResourceReport {
    let constraints = platform.resource_constraints();
    let mut violations = Vec::new();

    // Flash: code + static data
    let flash_used = layout.estimated_code_bytes + layout.static_data_bytes;
    let flash_available = constraints.flash_bytes;
    let flash_percent = if flash_available > 0 {
        (flash_used as f64 / flash_available as f64) * 100.0
    } else {
        0.0
    };
    if flash_used > flash_available {
        violations.push(format!(
            "flash overflow: need {} bytes, have {} bytes",
            flash_used, flash_available
        ));
    }

    // RAM: peak stack + dynamic data
    let ram_used = layout.peak_stack_bytes;
    let ram_available = constraints.ram_bytes;
    let ram_percent = if ram_available > 0 {
        (ram_used as f64 / ram_available as f64) * 100.0
    } else {
        0.0
    };
    if ram_used > ram_available {
        violations.push(format!(
            "RAM overflow: need {} bytes, have {} bytes",
            ram_used, ram_available
        ));
    }

    // Stack
    let stack = constraints.max_stack_bytes.map(|max_stack| {
        let stack_used = layout.peak_stack_bytes;
        let stack_percent = if max_stack > 0 {
            (stack_used as f64 / max_stack as f64) * 100.0
        } else {
            0.0
        };
        if stack_used > max_stack {
            violations.push(format!(
                "stack overflow: need {} bytes, limit {} bytes",
                stack_used, max_stack
            ));
        }
        ResourceUsage {
            name: "stack".into(),
            used: stack_used,
            available: max_stack,
            percent: stack_percent,
        }
    });

    let all_fit = violations.is_empty();

    ResourceReport {
        flash: ResourceUsage {
            name: "flash".into(),
            used: flash_used,
            available: flash_available,
            percent: flash_percent,
        },
        ram: ResourceUsage {
            name: "ram".into(),
            used: ram_used,
            available: ram_available,
            percent: ram_percent,
        },
        stack,
        all_fit,
        violations,
    }
}

/// Return an error if the resource report has any violations.
pub fn require_fit(report: &ResourceReport) -> Result<(), MaterializationError> {
    if report.all_fit {
        Ok(())
    } else {
        Err(MaterializationError::ResourceFittingFailed {
            message: report.violations.join("; "),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::estimate_layout;
    use torc_core::graph::node::{Node, NodeKind};
    use torc_core::graph::Graph;
    use torc_core::types::{Type, TypeSignature};

    #[test]
    fn small_graph_fits_linux() {
        let mut g = Graph::new();
        let n1 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        g.add_node(n1).unwrap();

        let platform = Platform::generic_linux_x86_64();
        let layout = estimate_layout(&g, &platform).unwrap();
        let report = check_resource_fit(&layout, &platform);

        assert!(report.all_fit);
        assert!(report.violations.is_empty());
        assert!(report.flash.percent < 1.0);
    }

    #[test]
    fn overflow_detection() {
        // Create a layout that exceeds a tiny platform
        let layout = MemoryLayout {
            frames: vec![],
            peak_stack_bytes: 1_000_000, // 1 MB stack
            static_data_bytes: 500_000,
            estimated_code_bytes: 2_000_000, // 2 MB code
        };

        // STM32 has 1 MB flash and ~256 KB RAM
        let platform = Platform::stm32f407_discovery();
        let report = check_resource_fit(&layout, &platform);

        assert!(!report.all_fit);
        assert!(!report.violations.is_empty());
    }

    #[test]
    fn require_fit_error_on_violation() {
        let layout = MemoryLayout {
            frames: vec![],
            peak_stack_bytes: 1_000_000,
            static_data_bytes: 500_000,
            estimated_code_bytes: 2_000_000,
        };

        let platform = Platform::stm32f407_discovery();
        let report = check_resource_fit(&layout, &platform);
        let result = require_fit(&report);
        assert!(result.is_err());
    }
}
