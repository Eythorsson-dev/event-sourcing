use proc_macro::TokenStream;

mod derive_event;
mod projection_macro;

#[proc_macro_derive(Event)]
pub fn derive_event(input: TokenStream) -> TokenStream {
    derive_event::impl_derive_event(input.into())
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}

/// Declarative DSL macro for defining read model projections.
///
/// Parses the Proposal H DSL syntax and emits:
/// - A `#[derive(Debug, serde::Deserialize)] pub struct {Name}` for each projection
/// - Companion structs for nested object fields (`{Name}{FieldPascal}`)
/// - `impl event_sourcing::ReadModel for {Name}` with `fn definition()`
///
/// # Example
///
/// ```ignore
/// use event_sourcing::projection;
///
/// projection! CustomerView {
///     query tag.starts_with("customer:") as c
///
///     name:  c.CustomerRegistered.name
///          | c.CustomerRenamed.name,
/// }
/// ```
#[proc_macro]
pub fn projection(input: TokenStream) -> TokenStream {
    projection_macro::impl_projection_macro(input.into())
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}
