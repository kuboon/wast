#[allow(warnings)]
mod bindings;

struct Component;

impl bindings::exports::wast::core::syntax_plugin::Guest for Component {
    fn to_text(
        _component: bindings::wast::core::types::WastComponent,
    ) -> String {
        todo!("implement to_text")
    }

    fn from_text(
        _text: String,
        _existing: bindings::wast::core::types::WastComponent,
    ) -> Result<bindings::wast::core::types::WastComponent, Vec<bindings::wast::core::types::WastError>> {
        todo!("implement from_text")
    }
}

bindings::export!(Component with_types_in bindings);
