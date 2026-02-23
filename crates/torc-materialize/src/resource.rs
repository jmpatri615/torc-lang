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

/// Format a number with comma-separated thousands.
pub(crate) fn format_number(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            result.push(b',');
        }
        result.push(b);
    }
    String::from_utf8(result).unwrap()
}

impl ResourceReport {
    /// Format a compact spec-style resource report with right-aligned columns.
    pub fn format_spec_style(&self) -> String {
        // Pre-format all lines: (label, used_str, avail_str, percent)
        let mut lines: Vec<(String, String, String, f64)> = vec![
            (
                "Flash".into(),
                format_number(self.flash.used),
                format_number(self.flash.available),
                self.flash.percent,
            ),
            (
                "RAM".into(),
                format_number(self.ram.used),
                format_number(self.ram.available),
                self.ram.percent,
            ),
        ];
        if let Some(ref stack) = self.stack {
            lines.push((
                "Stack".into(),
                format_number(stack.used),
                format_number(stack.available),
                stack.percent,
            ));
        }

        // Compute column widths for right-alignment
        let max_name_len = lines.iter().map(|(n, _, _, _)| n.len()).max().unwrap_or(0);
        let max_used_len = lines.iter().map(|(_, u, _, _)| u.len()).max().unwrap_or(0);
        let max_avail_len = lines.iter().map(|(_, _, a, _)| a.len()).max().unwrap_or(0);

        let mut out = String::from("Resources:\n");
        for (name, used_s, avail_s, percent) in &lines {
            out.push_str(&format!(
                "  {:<width_name$}  {:>width_used$} / {:>width_avail$} bytes  ({:.1}%)\n",
                format!("{name}:"),
                used_s,
                avail_s,
                percent,
                width_name = max_name_len + 1,
                width_used = max_used_len,
                width_avail = max_avail_len,
            ));
        }
        out.truncate(out.trim_end_matches('\n').len());
        out
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
    fn format_number_basic() {
        assert_eq!(super::format_number(0), "0");
        assert_eq!(super::format_number(999), "999");
        assert_eq!(super::format_number(1_000), "1,000");
        assert_eq!(super::format_number(31_244), "31,244");
        assert_eq!(super::format_number(524_288), "524,288");
        assert_eq!(super::format_number(1_000_000), "1,000,000");
    }

    #[test]
    fn format_spec_style_all_resources() {
        let report = ResourceReport {
            flash: ResourceUsage {
                name: "flash".into(),
                used: 31_244,
                available: 524_288,
                percent: 6.0,
            },
            ram: ResourceUsage {
                name: "ram".into(),
                used: 2_108,
                available: 131_072,
                percent: 1.6,
            },
            stack: Some(ResourceUsage {
                name: "stack".into(),
                used: 892,
                available: 4_096,
                percent: 21.8,
            }),
            all_fit: true,
            violations: vec![],
        };

        let output = report.format_spec_style();
        assert!(output.starts_with("Resources:"));
        assert!(output.contains("Flash:"));
        assert!(output.contains("RAM:"));
        assert!(output.contains("Stack:"));
        assert!(output.contains("31,244"));
        assert!(output.contains("524,288"));
        assert!(output.contains("6.0%"));
        assert!(output.contains("21.8%"));
    }

    #[test]
    fn format_spec_style_no_stack() {
        let report = ResourceReport {
            flash: ResourceUsage {
                name: "flash".into(),
                used: 1_000,
                available: 1_000_000,
                percent: 0.1,
            },
            ram: ResourceUsage {
                name: "ram".into(),
                used: 500,
                available: 500_000,
                percent: 0.1,
            },
            stack: None,
            all_fit: true,
            violations: vec![],
        };

        let output = report.format_spec_style();
        assert!(output.contains("Flash:"));
        assert!(output.contains("RAM:"));
        assert!(!output.contains("Stack:"));
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
