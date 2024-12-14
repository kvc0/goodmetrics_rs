use component::Component;
use field_attributes::FieldAttributes;
use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

mod component;
mod field_attributes;
mod symbol;

// #[proc_macro_derive(Stat, attributes(stat))]
// pub fn stat_derive(input: TokenStream) -> TokenStream {
//     let ast = syn::parse(input).expect("stat code must be valid rust");

//     // Build the trait implementation
//     impl_stat(&ast)
// }

// fn impl_stat(ast: &syn::DeriveInput) -> TokenStream {
//     let root = Component::from_ast(ast);
//     root.as_token_stream().into()
// }

#[proc_macro_derive(Stat, attributes(stat))]
pub fn stat_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    // Build the trait implementation
    TokenStream::from(match Component::from_ast(&ast) {
        Ok(root) => root.as_token_stream(),
        Err(e) => e.into_compile_error(),
    })
}
