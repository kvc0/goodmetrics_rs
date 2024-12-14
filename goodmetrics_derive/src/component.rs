use quote::quote;
use syn::{spanned::Spanned, Field};

use crate::FieldAttributes;

#[derive(Debug)]
pub struct Component {
    ident: syn::Ident,
    fields: Vec<FieldAttributes>,
}

impl Component {
    pub fn from_ast(ast: &syn::DeriveInput) -> syn::Result<Self> {
        let ident = &ast.ident;
        let data_struct = match &ast.data {
            syn::Data::Struct(data_struct) => data_struct,
            _other => {
                return Err(syn::Error::new(
                    ast.span(),
                    "Only structs are supported for deriving Stat",
                ));
            }
        };

        let fields = match &data_struct.fields {
            syn::Fields::Named(fields_named) => fields_named,
            _ => {
                return Err(syn::Error::new(
                    ast.span(),
                    "Only named fields are supported for deriving Stat",
                ));
            }
        };

        let fields: syn::Result<Vec<FieldAttributes>> = fields
            .named
            .iter()
            .map(|field| {
                let Field {
                    ident: Some(ident), ..
                } = field
                else {
                    return Err(syn::Error::new(
                        ast.span(),
                        "Only named fields are supported for deriving Stat",
                    ));
                };
                FieldAttributes::from_ast(ident.clone(), field)
            })
            .collect();

        Ok(Self {
            ident: ident.clone(),
            fields: fields?,
        })
    }

    pub fn as_token_stream(&self) -> proc_macro2::TokenStream {
        let collect_implementation_token_stream: proc_macro2::TokenStream = self
            .fields
            .iter()
            .map(|field| field.as_collect_token_stream())
            .collect();

        let implementation_token_stream: proc_macro2::TokenStream = self
            .fields
            .iter()
            .map(|field| field.as_impl_token_stream())
            .collect();

        let ident = &self.ident;
        quote! {
            #[automatically_derived]
            impl goodmetrics::stats::Stat for #ident {
                fn record(&self, collector: &mut impl goodmetrics::stats::Collector) {
                    #collect_implementation_token_stream
                }
            }

            #[automatically_derived]
            impl #ident {
                #implementation_token_stream
            }
        }
    }
}
