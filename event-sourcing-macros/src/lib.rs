use proc_macro::TokenStream;

mod derive_event;

#[proc_macro_derive(Event)]
pub fn derive_event(input: TokenStream) -> TokenStream {
    derive_event::impl_derive_event(input.into())
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}
