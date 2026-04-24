//! Implementation of the `projection!` declarative query language macro.
//!
//! The macro parses a custom DSL (Proposal H from CONTEXT.md) and emits:
//! 1. Companion structs for nested object fields (e.g. `{Name}{FieldPascal}`)
//! 2. The main `#[derive(Debug, serde::Deserialize)] pub struct {Name} { ... }`
//! 3. `impl event_sourcing::ReadModel for {Name} { fn definition() -> ... }`
//!
//! The macro uses a custom grammar parser via `syn::parse::Parse` — NOT Rust syntax.
//!
//! Note: `ParseStream` in syn is `type ParseStream<'a> = &'a ParseBuffer<'a>`, so
//! helper functions accept `ParseStream` (already a reference) — never `&ParseStream`.

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    braced, parenthesized,
    parse::{Parse, ParseBuffer, ParseStream},
    token, Error, Ident, LitStr, Result, Token,
};

// ── Custom keywords ──────────────────────────────────────────────────────────

syn::custom_keyword!(projection);
syn::custom_keyword!(query);
syn::custom_keyword!(tag);
syn::custom_keyword!(starts_with);
syn::custom_keyword!(ends_with);
syn::custom_keyword!(equals);
syn::custom_keyword!(cleared_by);

// ── AST types ────────────────────────────────────────────────────────────────

/// The full input to `projection! { ... }`.
struct ProjectionInput {
    name: Ident,
    query: QuerySpec,
    fields: Vec<FieldDecl>,
}

/// The tag filter in the `query` line.
enum QuerySpec {
    StartsWith(String),
    EndsWith(String),
    Equals(String),
}

/// A single field declaration (scalar or nested object).
enum FieldDecl {
    Scalar {
        name: Ident,
        optional: bool,
        handlers: Vec<HandlerDecl>,
    },
    Object {
        name: Ident,
        optional: bool,
        sub_fields: Vec<ScalarFieldDecl>,
        cleared_by_event: Option<String>,
    },
}

/// A handler inside a scalar field: `alias.EventType.path` or `alias.EventType = value`.
struct HandlerDecl {
    event_type: String,
    operation: OperationDecl,
}

/// A scalar field inside an object block.
struct ScalarFieldDecl {
    name: Ident,
    handlers: Vec<HandlerDecl>,
}

/// The operation in a handler.
enum OperationDecl {
    /// `alias.EventType.field_path` → `HandlerSpec::from_path("$.field_path")`
    FromPath(String),
    /// `alias.EventType = null`
    ValueNull,
    /// `alias.EventType = "literal"`
    ValueString(String),
    /// `|+ alias.EventType.path` → `HandlerSpec::increment_by("$.path")`
    IncrementBy(String),
    /// `|- alias.EventType.path` → `HandlerSpec::decrement_by("$.path")`
    DecrementBy(String),
}

// ── Parsing ──────────────────────────────────────────────────────────────────

impl Parse for ProjectionInput {
    fn parse(input: ParseStream) -> Result<Self> {
        // `projection Name { ... }`
        input.parse::<projection>()?;
        let name: Ident = input.parse()?;

        let body;
        braced!(body in input);

        // `query tag.starts_with("prefix:") as alias`
        body.parse::<query>()?;
        body.parse::<tag>()?;
        body.parse::<Token![.]>()?;

        let query = if body.peek(starts_with) {
            body.parse::<starts_with>()?;
            let args;
            parenthesized!(args in body);
            let lit: LitStr = args.parse()?;
            QuerySpec::StartsWith(lit.value())
        } else if body.peek(ends_with) {
            body.parse::<ends_with>()?;
            let args;
            parenthesized!(args in body);
            let lit: LitStr = args.parse()?;
            QuerySpec::EndsWith(lit.value())
        } else if body.peek(equals) {
            body.parse::<equals>()?;
            let args;
            parenthesized!(args in body);
            let lit: LitStr = args.parse()?;
            QuerySpec::Equals(lit.value())
        } else {
            return Err(body.error(
                "expected `starts_with(...)`, `ends_with(...)`, or `equals(...)` after `tag.`",
            ));
        };

        // `as alias`
        body.parse::<Token![as]>()?;
        let alias: Ident = body.parse()?;

        // Parse field declarations
        let mut fields = Vec::new();
        while !body.is_empty() {
            fields.push(parse_field_decl(&body, &alias)?);
        }

        Ok(ProjectionInput {
            name,
            query,
            fields,
        })
    }
}

/// Parse a single field declaration from the projection body.
///
/// Handles:
/// - `field_name:  alias.Event.path | ...`         → required scalar
/// - `field_name?: alias.Event.path | ...`          → optional scalar
/// - `field_name? { ... }`                           → optional object (no cleared_by)
/// - `field_name? { ... } cleared_by alias.Event`   → optional object with cleared_by
fn parse_field_decl(input: &ParseBuffer, alias: &Ident) -> Result<FieldDecl> {
    let name: Ident = input.parse()?;

    // Check for `?` (optional marker)
    let optional = if input.peek(Token![?]) {
        input.parse::<Token![?]>()?;
        true
    } else {
        false
    };

    // Check if this is an object block `{ ... }` or a scalar `:`
    if input.peek(token::Brace) {
        // Object field: `name? { sub_fields... } [cleared_by alias.Event]`
        let sub_body;
        braced!(sub_body in input);

        let mut sub_fields = Vec::new();
        while !sub_body.is_empty() {
            sub_fields.push(parse_scalar_field_decl(&sub_body, alias)?);
        }

        // Optional `cleared_by alias.EventType`
        let cleared_by_event = if input.peek(cleared_by) {
            input.parse::<cleared_by>()?;
            let cb_alias: Ident = input.parse()?;
            // Verify alias matches
            if cb_alias != *alias {
                return Err(Error::new_spanned(
                    &cb_alias,
                    format!(
                        "cleared_by alias `{}` does not match query alias `{}`",
                        cb_alias, alias
                    ),
                ));
            }
            input.parse::<Token![.]>()?;
            let event_type: Ident = input.parse()?;
            Some(event_type.to_string())
        } else {
            None
        };

        Ok(FieldDecl::Object {
            name,
            optional,
            sub_fields,
            cleared_by_event,
        })
    } else {
        // Scalar field: `name: handler [| handler]*`
        input.parse::<Token![:]>()?;

        let handlers = parse_handlers(input, alias)?;

        Ok(FieldDecl::Scalar {
            name,
            optional,
            handlers,
        })
    }
}

/// Parse `alias.EventType.path | alias.Event2.path2 | ...`
/// Also handles `|+` and `|-` prefix operators for increment/decrement.
fn parse_handlers(input: &ParseBuffer, alias: &Ident) -> Result<Vec<HandlerDecl>> {
    let mut handlers = Vec::new();

    // First handler (no leading `|`)
    // Check if the first handler starts with `|+` or `|-`
    let first = if input.peek(Token![|]) {
        input.parse::<Token![|]>()?;
        if input.peek(Token![+]) {
            input.parse::<Token![+]>()?;
            parse_handler_rhs_path(input, alias, true)?
        } else if input.peek(Token![-]) {
            input.parse::<Token![-]>()?;
            parse_handler_rhs_path(input, alias, false)?
        } else {
            parse_handler_rhs(input, alias)?
        }
    } else {
        parse_handler_rhs(input, alias)?
    };
    handlers.push(first);

    // Continuation handlers: `| handler`
    while input.peek(Token![|]) {
        input.parse::<Token![|]>()?;
        let decl = if input.peek(Token![+]) {
            input.parse::<Token![+]>()?;
            parse_handler_rhs_path(input, alias, true)?
        } else if input.peek(Token![-]) {
            input.parse::<Token![-]>()?;
            parse_handler_rhs_path(input, alias, false)?
        } else {
            parse_handler_rhs(input, alias)?
        };
        handlers.push(decl);
    }

    Ok(handlers)
}

/// Parse `alias.EventType.path` or `alias.EventType = null/value` as a handler RHS.
fn parse_handler_rhs(input: &ParseBuffer, alias: &Ident) -> Result<HandlerDecl> {
    // `alias.EventType`
    let handler_alias: Ident = input.parse()?;
    if handler_alias != *alias {
        return Err(Error::new_spanned(
            &handler_alias,
            format!(
                "handler alias `{}` does not match query alias `{}`",
                handler_alias, alias
            ),
        ));
    }
    input.parse::<Token![.]>()?;
    let event_type: Ident = input.parse()?;

    // Now either `.field_path` or `= value`
    let operation = if input.peek(Token![=]) {
        input.parse::<Token![=]>()?;
        if input.peek(LitStr) {
            let lit: LitStr = input.parse()?;
            OperationDecl::ValueString(lit.value())
        } else {
            // `= null` — parse the `null` keyword as an ident
            let kw: Ident = input.parse()?;
            if kw != "null" {
                return Err(Error::new_spanned(
                    &kw,
                    format!(
                        "expected `null` or a string literal after `=`, found `{}`",
                        kw
                    ),
                ));
            }
            OperationDecl::ValueNull
        }
    } else {
        input.parse::<Token![.]>()?;
        // Parse dot-separated field path (e.g. `name` or `address.city`)
        let path = parse_dot_path(input)?;
        OperationDecl::FromPath(path)
    };

    Ok(HandlerDecl {
        event_type: event_type.to_string(),
        operation,
    })
}

/// Parse `alias.EventType.path` as an increment/decrement handler.
fn parse_handler_rhs_path(
    input: &ParseBuffer,
    alias: &Ident,
    increment: bool,
) -> Result<HandlerDecl> {
    let handler_alias: Ident = input.parse()?;
    if handler_alias != *alias {
        return Err(Error::new_spanned(
            &handler_alias,
            format!(
                "handler alias `{}` does not match query alias `{}`",
                handler_alias, alias
            ),
        ));
    }
    input.parse::<Token![.]>()?;
    let event_type: Ident = input.parse()?;
    input.parse::<Token![.]>()?;
    let path = parse_dot_path(input)?;

    let operation = if increment {
        OperationDecl::IncrementBy(path)
    } else {
        OperationDecl::DecrementBy(path)
    };

    Ok(HandlerDecl {
        event_type: event_type.to_string(),
        operation,
    })
}

/// Parse a scalar field inside an object block: `name: handler [| handler]*`
fn parse_scalar_field_decl(input: &ParseBuffer, alias: &Ident) -> Result<ScalarFieldDecl> {
    let name: Ident = input.parse()?;
    input.parse::<Token![:]>()?;
    let handlers = parse_handlers(input, alias)?;
    Ok(ScalarFieldDecl { name, handlers })
}

/// Parse a dot-separated identifier path like `name` or `address.city`.
/// Returns the JSONPath string `$.name` or `$.address.city`.
fn parse_dot_path(input: &ParseBuffer) -> Result<String> {
    let first: Ident = input.parse()?;
    let mut path = first.to_string();

    while input.peek(Token![.]) {
        // Peek ahead to check the token after `.` is an identifier
        let ahead = input.fork();
        let _ = ahead.parse::<Token![.]>();
        if ahead.peek(Ident) {
            input.parse::<Token![.]>()?;
            let next: Ident = input.parse()?;
            path.push('.');
            path.push_str(&next.to_string());
        } else {
            break;
        }
    }

    Ok(format!("$.{}", path))
}

// ── Code generation ──────────────────────────────────────────────────────────

/// Convert a snake_case ident to PascalCase string (e.g. `address` → `Address`).
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}

/// Generate code for a single `HandlerDecl`.
fn codegen_handler(handler: &HandlerDecl) -> TokenStream {
    let event_type = &handler.event_type;
    match &handler.operation {
        OperationDecl::FromPath(path) => quote! {
            .on(
                #event_type,
                event_sourcing::HandlerSpec::from_path(#path)
            )
        },
        OperationDecl::ValueNull => quote! {
            .on(
                #event_type,
                event_sourcing::HandlerSpec::value(serde_json::Value::Null)
            )
        },
        OperationDecl::ValueString(s) => quote! {
            .on(
                #event_type,
                event_sourcing::HandlerSpec::value(serde_json::Value::String(#s.to_string()))
            )
        },
        OperationDecl::IncrementBy(path) => quote! {
            .on(
                #event_type,
                event_sourcing::HandlerSpec::increment_by(#path)
            )
        },
        OperationDecl::DecrementBy(path) => quote! {
            .on(
                #event_type,
                event_sourcing::HandlerSpec::decrement_by(#path)
            )
        },
    }
}

/// Determine if a field is numeric (uses increment/decrement handlers).
fn is_numeric_field(handlers: &[HandlerDecl]) -> bool {
    handlers.iter().any(|h| {
        matches!(
            &h.operation,
            OperationDecl::IncrementBy(_) | OperationDecl::DecrementBy(_)
        )
    })
}

/// Main code generator for `ProjectionInput`.
fn generate_code(input: &ProjectionInput) -> TokenStream {
    let name = &input.name;
    let name_str = name.to_string();

    // Generate the query TagFilter
    let query_tokens = match &input.query {
        QuerySpec::StartsWith(s) => quote! {
            event_sourcing::query::TagFilter::StartsWith(#s.to_string())
        },
        QuerySpec::EndsWith(s) => quote! {
            event_sourcing::query::TagFilter::EndsWith(#s.to_string())
        },
        QuerySpec::Equals(s) => quote! {
            event_sourcing::query::TagFilter::Equals(
                event_sourcing::Tag::new(#s).expect("invalid tag in projection! macro")
            )
        },
    };

    // Collect companion structs for object fields
    let mut companion_structs: Vec<TokenStream> = Vec::new();
    // Collect struct field definitions for the main struct
    let mut struct_fields: Vec<TokenStream> = Vec::new();
    // Collect builder calls for ProjectionDefinition
    let mut builder_calls: Vec<TokenStream> = Vec::new();

    for field in &input.fields {
        match field {
            FieldDecl::Scalar {
                name: field_name,
                optional,
                handlers,
            } => {
                let field_name_str = field_name.to_string();
                let numeric = is_numeric_field(handlers);

                // Struct field type
                let field_type = if numeric {
                    if *optional {
                        quote! { Option<f64> }
                    } else {
                        quote! { f64 }
                    }
                } else if *optional {
                    quote! { Option<String> }
                } else {
                    quote! { String }
                };
                struct_fields.push(quote! {
                    pub #field_name: #field_type
                });

                // Builder call
                let handler_calls: Vec<TokenStream> =
                    handlers.iter().map(codegen_handler).collect();
                let required_call = if *optional {
                    quote! {}
                } else {
                    quote! { .required() }
                };
                builder_calls.push(quote! {
                    .scalar(
                        #field_name_str,
                        event_sourcing::ScalarFieldSpecBuilder::new()
                            #required_call
                            #(#handler_calls)*
                            .build()
                    )
                });
            }

            FieldDecl::Object {
                name: field_name,
                optional,
                sub_fields,
                cleared_by_event,
            } => {
                let field_name_str = field_name.to_string();
                let pascal = to_pascal_case(&field_name_str);
                let companion_name =
                    Ident::new(&format!("{}{}", name_str, pascal), Span::call_site());

                // Generate companion struct fields
                let mut companion_fields: Vec<TokenStream> = Vec::new();
                let mut object_field_calls: Vec<TokenStream> = Vec::new();

                for sub in sub_fields {
                    let sub_name = &sub.name;
                    let sub_name_str = sub.name.to_string();
                    let sub_numeric = is_numeric_field(&sub.handlers);
                    let sub_type = if sub_numeric {
                        quote! { Option<f64> }
                    } else {
                        quote! { Option<String> }
                    };
                    companion_fields.push(quote! {
                        pub #sub_name: #sub_type
                    });

                    let sub_handler_calls: Vec<TokenStream> =
                        sub.handlers.iter().map(codegen_handler).collect();
                    object_field_calls.push(quote! {
                        .field(
                            #sub_name_str,
                            event_sourcing::ScalarFieldSpecBuilder::new()
                                #(#sub_handler_calls)*
                                .build()
                        )
                    });
                }

                // Companion struct definition
                companion_structs.push(quote! {
                    #[derive(Debug, serde::Deserialize)]
                    pub struct #companion_name {
                        #(#companion_fields),*
                    }
                });

                // Main struct field
                let field_type = if *optional {
                    quote! { Option<#companion_name> }
                } else {
                    quote! { #companion_name }
                };
                struct_fields.push(quote! {
                    pub #field_name: #field_type
                });

                // Builder call for object field
                let cleared_by_call = if let Some(evt) = cleared_by_event {
                    quote! { .cleared_by(#evt) }
                } else {
                    quote! {}
                };
                builder_calls.push(quote! {
                    .object(
                        #field_name_str,
                        event_sourcing::ObjectFieldSpecBuilder::new()
                            #cleared_by_call
                            #(#object_field_calls)*
                            .build()
                    )
                });
            }
        }
    }

    quote! {
        #(#companion_structs)*

        #[derive(Debug, serde::Deserialize)]
        pub struct #name {
            #(#struct_fields),*
        }

        impl event_sourcing::ReadModel for #name {
            fn definition() -> event_sourcing::ProjectionDefinition {
                event_sourcing::ProjectionDefinition::builder(
                    #name_str,
                    #query_tokens,
                )
                #(#builder_calls)*
                .build()
            }
        }
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Entry point called from `lib.rs`.
pub fn impl_projection_macro(input: TokenStream) -> Result<TokenStream> {
    let parsed: ProjectionInput = syn::parse2(input)?;
    Ok(generate_code(&parsed))
}
