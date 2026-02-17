//! LLVM code generation context wrapping inkwell Context/Module/Builder.

use std::collections::HashMap;

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::BasicValueEnum;

use torc_core::graph::node::NodeId;

/// Code generation context holding LLVM state and the node→value mapping.
///
/// The `values` map tracks each node's output ports as LLVM values, keyed
/// by `(NodeId, port_index)`. When lowering a node, its input values are
/// looked up from the edges' source nodes, and its output values are stored
/// for downstream consumers.
pub struct CodegenContext<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    /// Maps (NodeId, output_port_index) → LLVM value.
    values: HashMap<(NodeId, usize), BasicValueEnum<'ctx>>,
}

impl<'ctx> CodegenContext<'ctx> {
    /// Create a new codegen context with an empty module.
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();
        Self {
            context,
            module,
            builder,
            values: HashMap::new(),
        }
    }

    /// Get the LLVM value for a node's output port.
    pub fn get_value(&self, node_id: &NodeId, port: usize) -> Option<BasicValueEnum<'ctx>> {
        self.values.get(&(*node_id, port)).copied()
    }

    /// Register an LLVM value for a node's output port.
    pub fn set_value(&mut self, node_id: NodeId, port: usize, value: BasicValueEnum<'ctx>) {
        self.values.insert((node_id, port), value);
    }

    /// Access the LLVM module.
    pub fn module(&self) -> &Module<'ctx> {
        &self.module
    }

    /// Access the IR builder.
    pub fn builder(&self) -> &Builder<'ctx> {
        &self.builder
    }

    /// Access the LLVM context.
    pub fn llvm_context(&self) -> &'ctx Context {
        self.context
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn context_creation() {
        let ctx = Context::create();
        let cg = CodegenContext::new(&ctx, "test_module");
        assert_eq!(cg.module().get_name().to_str().unwrap(), "test_module");
    }

    #[test]
    fn value_store_and_retrieve() {
        let ctx = Context::create();
        let mut cg = CodegenContext::new(&ctx, "test");
        let node_id = Uuid::new_v4();
        let val = ctx.i32_type().const_int(42, false).into();
        cg.set_value(node_id, 0, val);
        assert!(cg.get_value(&node_id, 0).is_some());
        assert!(cg.get_value(&node_id, 1).is_none());
    }
}
