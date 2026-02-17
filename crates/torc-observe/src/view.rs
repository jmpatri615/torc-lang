//! View trait and core abstractions for the observability layer.

use serde_json::Value;
use torc_core::graph::Graph;
use torc_materialize::resource::ResourceReport;
use torc_materialize::schedule::ExecutionSchedule;
use torc_targets::Platform;

use crate::error::ObserveError;

/// The kind of view to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    PseudoCode,
    Contract,
    ResourceBudget,
    Dataflow,
    Provenance,
}

impl ViewKind {
    /// Parse a view kind from a string.
    pub fn parse(s: &str) -> Result<Self, ObserveError> {
        match s {
            "pseudo-code" | "pseudocode" => Ok(ViewKind::PseudoCode),
            "contracts" | "contract" => Ok(ViewKind::Contract),
            "resources" | "resource-budget" => Ok(ViewKind::ResourceBudget),
            "dataflow" => Ok(ViewKind::Dataflow),
            "provenance" => Ok(ViewKind::Provenance),
            _ => Err(ObserveError::UnknownView {
                name: s.to_string(),
            }),
        }
    }

    /// Display name for this view kind.
    pub fn name(&self) -> &'static str {
        match self {
            ViewKind::PseudoCode => "pseudo-code",
            ViewKind::Contract => "contracts",
            ViewKind::ResourceBudget => "resources",
            ViewKind::Dataflow => "dataflow",
            ViewKind::Provenance => "provenance",
        }
    }
}

/// The output format for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewFormat {
    Text,
    Json,
}

impl ViewFormat {
    /// Parse a view format from a string.
    pub fn parse(s: &str) -> Self {
        match s {
            "json" => ViewFormat::Json,
            _ => ViewFormat::Text,
        }
    }
}

/// The output of a view render.
#[derive(Debug)]
pub struct ViewOutput {
    /// Terminal-friendly text rendering.
    pub text: String,
    /// Machine-readable JSON (always populated).
    pub data: Value,
}

impl ViewOutput {
    /// Render in the requested format.
    pub fn render(&self, format: ViewFormat) -> String {
        match format {
            ViewFormat::Text => self.text.clone(),
            ViewFormat::Json => serde_json::to_string_pretty(&self.data)
                .unwrap_or_else(|_| "{}".to_string()),
        }
    }
}

/// Context passed to views for rendering.
pub struct RenderContext<'a> {
    /// Optional target platform (needed for resource budget).
    pub platform: Option<&'a Platform>,
    /// Optional resource report (pre-computed).
    pub resource_report: Option<&'a ResourceReport>,
    /// Optional execution schedule (pre-computed).
    pub schedule: Option<&'a ExecutionSchedule>,
}

impl<'a> RenderContext<'a> {
    /// Create an empty render context.
    pub fn empty() -> Self {
        Self {
            platform: None,
            resource_report: None,
            schedule: None,
        }
    }
}

/// Trait for all observability views.
pub trait View {
    /// Render this view for the given graph and context.
    fn render(&self, graph: &Graph, ctx: &RenderContext<'_>) -> Result<ViewOutput, ObserveError>;

    /// The kind of view this is.
    fn kind(&self) -> ViewKind;
}

/// List all available view kinds.
pub fn available_views() -> &'static [ViewKind] {
    &[
        ViewKind::PseudoCode,
        ViewKind::Contract,
        ViewKind::ResourceBudget,
        ViewKind::Dataflow,
        ViewKind::Provenance,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_view_kinds() {
        assert_eq!(ViewKind::parse("pseudo-code").unwrap(), ViewKind::PseudoCode);
        assert_eq!(ViewKind::parse("pseudocode").unwrap(), ViewKind::PseudoCode);
        assert_eq!(ViewKind::parse("contracts").unwrap(), ViewKind::Contract);
        assert_eq!(ViewKind::parse("contract").unwrap(), ViewKind::Contract);
        assert_eq!(ViewKind::parse("resources").unwrap(), ViewKind::ResourceBudget);
        assert_eq!(ViewKind::parse("resource-budget").unwrap(), ViewKind::ResourceBudget);
        assert_eq!(ViewKind::parse("dataflow").unwrap(), ViewKind::Dataflow);
        assert_eq!(ViewKind::parse("provenance").unwrap(), ViewKind::Provenance);
    }

    #[test]
    fn parse_unknown_view() {
        assert!(ViewKind::parse("nonexistent").is_err());
    }

    #[test]
    fn view_format_parsing() {
        assert_eq!(ViewFormat::parse("json"), ViewFormat::Json);
        assert_eq!(ViewFormat::parse("text"), ViewFormat::Text);
        assert_eq!(ViewFormat::parse("anything"), ViewFormat::Text);
    }

    #[test]
    fn view_output_render_formats() {
        let output = ViewOutput {
            text: "hello world".to_string(),
            data: serde_json::json!({"greeting": "hello"}),
        };
        assert_eq!(output.render(ViewFormat::Text), "hello world");
        let json = output.render(ViewFormat::Json);
        assert!(json.contains("greeting"));
        assert!(json.contains("hello"));
    }
}
