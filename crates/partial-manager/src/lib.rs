#[allow(warnings)]
mod bindings;

struct Component;

impl bindings::exports::wast::core::partial_manager::Guest for Component {
    fn extract(
        _full: bindings::wast::core::types::WastComponent,
        _targets: Vec<bindings::wast::core::types::ExtractTarget>,
    ) -> bindings::wast::core::types::WastComponent {
        todo!("implement extract")
    }

    fn merge(
        _partial: bindings::wast::core::types::WastComponent,
        _full: bindings::wast::core::types::WastComponent,
    ) -> Result<bindings::wast::core::types::WastComponent, Vec<bindings::wast::core::types::WastError>> {
        todo!("implement merge")
    }
}

bindings::export!(Component with_types_in bindings);
