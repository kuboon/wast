#[allow(warnings)]
mod bindings;

struct Component;

impl bindings::exports::wast::core::file_manager::Guest for Component {
    fn read(
        _path: String,
        _targets: Option<Vec<bindings::wast::core::types::ExtractTarget>>,
    ) -> Result<bindings::wast::core::types::WastComponent, bindings::wast::core::types::WastError> {
        todo!("implement read")
    }

    fn write(
        _path: String,
        _component: bindings::wast::core::types::WastComponent,
    ) -> Result<(), bindings::wast::core::types::WastError> {
        todo!("implement write")
    }

    fn merge(
        _path: String,
        _partial: bindings::wast::core::types::WastComponent,
    ) -> Result<(), bindings::wast::core::types::WastError> {
        todo!("implement merge")
    }
}

bindings::export!(Component with_types_in bindings);
