use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields, Type, parse2};

pub fn impl_derive_event(input: TokenStream) -> Result<TokenStream, Error> {
    let ast: DeriveInput = parse2(input)?;
    let name = &ast.ident;
    let name_str = name.to_string();

    let fields = match &ast.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => &f.named,
            _ => {
                return Err(Error::new_spanned(
                    &ast,
                    "#[derive(Event)] only supports structs with named fields",
                ))
            }
        },
        _ => {
            return Err(Error::new_spanned(
                &ast,
                "#[derive(Event)] only supports structs",
            ))
        }
    };

    let field_defs: Vec<TokenStream> = fields
        .iter()
        .map(|f| {
            let field_name = f.ident.as_ref().unwrap().to_string();
            let (field_type_tokens, is_optional) = map_rust_type_to_field_type(&f.ty, f)?;
            Ok(quote! {
                event_sourcing::FieldDef {
                    name: #field_name.to_string(),
                    field_type: #field_type_tokens,
                    optional: #is_optional,
                }
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;

    Ok(quote! {
        impl event_sourcing::Event for #name {
            fn event_type() -> &'static str {
                #name_str
            }
            fn schema() -> event_sourcing::EventSchemaDef {
                event_sourcing::EventSchemaDef {
                    event_type: event_sourcing::EventType::from(#name_str),
                    fields: vec![#(#field_defs),*],
                }
            }
        }
    })
}

/// Returns `(FieldType tokens, is_optional)`.
/// `is_optional` is true when the Rust type is `Option<T>` — sets `FieldDef.optional`.
fn map_rust_type_to_field_type(
    ty: &Type,
    span_target: &impl quote::ToTokens,
) -> Result<(TokenStream, bool), Error> {
    let type_str = quote!(#ty).to_string().replace(' ', "");
    let (inner_str, is_optional) = if type_str.starts_with("Option<") && type_str.ends_with('>') {
        (&type_str[7..type_str.len() - 1], true)
    } else {
        (type_str.as_str(), false)
    };
    let field_type = match inner_str {
        "String" | "std::string::String" | "&str" | "str" => {
            quote! { event_sourcing::FieldType::String }
        }
        "i8" | "i16" | "i32" | "i64" | "i128" | "u8" | "u16" | "u32" | "u64" | "u128"
        | "usize" | "isize" => {
            quote! { event_sourcing::FieldType::Integer }
        }
        "f32" | "f64" => quote! { event_sourcing::FieldType::Decimal },
        "bool" => quote! { event_sourcing::FieldType::String },
        "chrono::NaiveDate" => quote! { event_sourcing::FieldType::Date },
        "chrono::DateTime" | "chrono::DateTime<chrono::Utc>" | "DateTime<Utc>" => {
            quote! { event_sourcing::FieldType::DateTime }
        }
        _ => {
            return Err(Error::new_spanned(
                span_target,
                format!(
                    "unsupported field type for #[derive(Event)]: `{}`. Use String, integer primitive, f32/f64, chrono::NaiveDate, or chrono::DateTime",
                    inner_str
                ),
            ))
        }
    };
    Ok((field_type, is_optional))
}
