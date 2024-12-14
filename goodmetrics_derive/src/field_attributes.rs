use quote::format_ident;
use quote::quote;
use quote::ToTokens;
use syn::braced;
use syn::parenthesized;
use syn::parse::ParseBuffer;
use syn::spanned::Spanned;
use syn::token::Comma;
use syn::Ident;

use crate::symbol::*;

#[derive(Debug)]
pub struct FieldAttributes {
    pub ident: syn::Ident,
    pub kind: StatKind,
}

#[derive(Debug)]
pub enum StatKind {
    /// resets per report
    Sum,
    /// inspects the value at a point in time
    Number,
    /// monotonic value
    Monotonic,
    /// histogram value
    Histogram,
    /// a group of stats within a complete stat
    Component {
        dimension_root_kind: ComponentRootKind,
        dimensions: Vec<proc_macro2::TokenStream>,
    },
}

#[derive(Debug)]
pub enum ComponentRootKind {
    Root,
    Subcomponent,
}

impl syn::parse::Parse for ComponentRootKind {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.parse()?;
        match ident.to_string().as_str() {
            "root" => Ok(Self::Root),
            "subcomponent" => Ok(Self::Subcomponent),
            _ => Err(syn::Error::new(
                ident.span(),
                "Expected root or subcomponent",
            )),
        }
    }
}

#[derive(Debug)]
pub enum ComponentCustomizationKind {
    Dimensions,
}

impl syn::parse::Parse for ComponentCustomizationKind {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.parse()?;
        match ident.to_string().as_str() {
            "dimensions" => Ok(Self::Dimensions),
            _ => Err(syn::Error::new(ident.span(), "Expected dimensions")),
        }
    }
}

impl FieldAttributes {
    // Borrowed strategies in here from serde_derive
    pub fn from_ast(ident: syn::Ident, field: &syn::Field) -> syn::Result<Self> {
        let mut kind = None;
        if field.attrs.is_empty() {
            return Err(syn::Error::new(field.span(), "No stat attribute found. Expected one of sum, number, monotonic, histogram, component"));
        }
        for attr in &field.attrs {
            if attr.path() != STAT {
                // not a stat attribute
                continue;
            }
            if kind.is_some() {
                return Err(syn::Error::new(
                    attr.span(),
                    "Only one stat attribute is allowed per field",
                ));
            }

            //eprintln!("attr: {:#?}", attr);
            attr.parse_nested_meta(|meta| {
                if meta.path == SUM {
                    // #[stat(sum)]
                    kind = Some(StatKind::Sum);
                } else if meta.path == NUMBER {
                    // #[stat(sum)]
                    kind = Some(StatKind::Number);
                } else if meta.path == MONOTONIC {
                    // #[stat(sum)]
                    kind = Some(StatKind::Monotonic);
                } else if meta.path == HISTOGRAM {
                    // #[stat(histogram)]
                    kind = Some(StatKind::Histogram);
                } else if meta.path == COMPONENT {
                    // #[stat(
                    //     component(
                    //         root,
                    //         dimensions = {
                    //             name="a_subcomponent"
                    //         }
                    //     )
                    // )]
                    let content;
                    parenthesized!(content in meta.input);
                    let dimension_root_kind: ComponentRootKind = content.parse()?;
                    if !content.is_empty() {
                        let _eat_the_comma: Comma = content.parse()?;
                    }
                    let mut dimensions = Vec::new();
                    while !content.is_empty() {
                        let customization: ComponentCustomizationKind = content.parse()?;
                        let _eat_the_equals: syn::token::Eq = content.parse()?;
                        match customization {
                            ComponentCustomizationKind::Dimensions => {
                                let dimension_content;
                                braced!(dimension_content in content);
                                while !dimension_content.is_empty() {
                                    let name: Ident = dimension_content.parse()?;
                                    dimensions.push(get_literal_dimension_tuple_tokens(name, &dimension_content)?);
                                    if !dimension_content.is_empty() {
                                        let _eat_the_comma: Comma = dimension_content.parse()?;
                                    }
                                }
                            }
                        }
                        eprintln!("component: customization: {customization:#?}");
                    }

                    kind = Some(StatKind::Component {
                        dimension_root_kind,
                        dimensions,
                    });
                    return Ok(());
                } else {
                    let path = meta.path.to_token_stream().to_string().replace(' ', "");
                    return Err(
                        meta.error(format_args!("unknown stat container attribute `{path}`. Expected one of sum, number, monotonic, histogram"))
                    );
                }
                Ok(())
            })?;
        }
        Ok(Self {
            ident,
            kind: kind.expect("must set stat(<kind>) attribute, where <kind> is one of sum, number, monotonic, histogram. E.g., stat(sum)"),
        })
    }

    pub fn as_collect_token_stream(&self) -> proc_macro2::TokenStream {
        let ident = &self.ident;
        let name = ident.to_string();
        match &self.kind {
            StatKind::Sum => {
                quote! {
                    collector.collect_sum(goodmetrics::Name::Str(#name), &self.#ident);
                }
            }
            StatKind::Number => {
                quote! {
                    collector.observe_number(goodmetrics::Name::Str(#name), &self.#ident);
                }
            }
            StatKind::Monotonic => {
                quote! {
                    collector.observe_monotonic(goodmetrics::Name::Str(#name), &self.#ident);
                }
            }
            StatKind::Histogram => {
                quote! {
                    collector.collect_histogram(goodmetrics::Name::Str(#name), &self.#ident);
                }
            }
            StatKind::Component {
                dimension_root_kind,
                dimensions,
            } => match dimension_root_kind {
                ComponentRootKind::Root => quote! {
                    goodmetrics::stats::Collector::collect_root_component(collector, &self.#ident, [
                        #(#dimensions),*
                    ]);
                },
                ComponentRootKind::Subcomponent => quote! {
                    goodmetrics::stats::Collector::collect_subcomponent(collector, &self.#ident, [
                        #(#dimensions),*
                    ]);
                },
            },
        }
    }

    pub fn as_impl_token_stream(&self) -> proc_macro2::TokenStream {
        let ident = &self.ident;
        match &self.kind {
            StatKind::Sum => {
                let increment_sum_ident = format_ident!("increase_sum_{ident}");
                let increment_sum_msg = format!(
                    "Add count to the {ident} sum\n\n\
                    # Arguments\n\
                    - `count` - The amount to add to {ident}\n\n\
                    # Note\n\
                    Sums reset to 0 after a report. Each report encapsulates a ",
                );

                quote! {
                    #[doc = #increment_sum_msg]
                    pub fn #increment_sum_ident(&self, count: u64) {
                        self.#ident.fetch_add(count, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
            StatKind::Number => {
                let set_number_ident = format_ident!("set_number_{}", ident);
                let set_number_msg = format!(
                    "Set the value of the number {ident}\n\n\
                    # Arguments\n\
                    - `value` - The value to set {ident} to\n\n\
                    # Note\n\
                    Numbers don't reset to 0 after a report. Set the number to the value you want to report
                    as often as it changes, and whatever the value happens to be at the time of the next
                    report is what is reported.
                    ",);
                quote! {
                    #[doc = #set_number_msg]
                    pub fn #set_number_ident(&self, value: u64) {
                        self.#ident.store(value, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
            StatKind::Monotonic => {
                let increase_monotonic_ident = format_ident!("increase_monotonic_{ident}");
                let increase_monotonic_msg = format!(
                    "Add count to the monotonic {ident} counter\n\n\
                    # Arguments\n\
                    - `count` - The amount to add to {ident}\n\n\
                    # Note\n\
                    Monotonic counters don't reset to 0 after a report. They only keep increasing.",
                );

                let set_monotonic_ident = format_ident!("set_monotonic_{}", ident);
                let set_monotonic_msg = format!(
                    "Set the value of the monotonic {ident} counter\n\n\
                    # Arguments\n\
                    - `value` - The value to set {ident} to\n\n\
                    # Note\n\
                    Monotonic counters don't reset to 0 after a report. They only keep increasing.
                    If you reset the value to a lower number with this function, your downstream collector
                    may be greatly confused.
                    ",);
                quote! {
                    #[doc = #increase_monotonic_msg]
                    pub fn #increase_monotonic_ident(&self, count: u64) {
                        self.#ident.fetch_add(count, std::sync::atomic::Ordering::Relaxed);
                    }

                    #[doc = #set_monotonic_msg]
                    pub fn #set_monotonic_ident(&self, value: u64) {
                        self.#ident.store(value, std::sync::atomic::Ordering::Relaxed);
                        // std::dbg_assert!(self.#ident.load(std::sync::atomic::Ordering::Relaxed) <= value, "Monothonic counter was set to {value}, which is less than the previous value. You might want a number instead of a monotonic.");
                    }
                }
            }
            StatKind::Histogram => {
                let accumulate_ident = format_ident!("accumulate_histogram_{}", ident);
                let accumulate_msg = format!(
                    "Observe `value` and increment its bucket in the {ident} histogram\n\n\
                    # Arguments\n\
                    - `value` - The value to increment in {ident}'s buckets to\n\n\
                    # Note\n\
                    Histograms SHOULD reset after a report, but it is an implementation detail of the Collector.
                    ",);
                quote! {
                    #[doc = #accumulate_msg]
                    pub fn #accumulate_ident(&self, value: impl Into<f64>) {
                        self.#ident.accumulate(value.into());
                        // std::dbg_assert!(self.#ident.load(std::sync::atomic::Ordering::Relaxed) <= value, "Monothonic counter was set to {value}, which is less than the previous value. You might want a number instead of a monotonic.");
                    }
                }
            }
            StatKind::Component {
                dimension_root_kind: _,
                dimensions: _,
            } => {
                quote! {}
            }
        }
    }
}

// Thanks david tolnay for serde_derive
fn get_literal_dimension_tuple_tokens(
    meta_item_name: Ident,
    meta: &ParseBuffer,
) -> syn::Result<proc_macro2::TokenStream> {
    let _eat_the_equals = meta.parse::<syn::token::Eq>()?;
    let expr: syn::Expr = meta.parse()?;
    let mut value = &expr;
    while let syn::Expr::Group(e) = value {
        value = &e.expr;
    }
    let name = meta_item_name.to_string();
    // string literal dimension names are as optimal a dimension name as you can get
    let dimension_name = quote! {
        goodmetrics::Name::Str(#name)
    };
    Ok(match value {
        syn::Expr::Lit(syn::ExprLit { lit, .. }) => {
            match lit {
                // literal dimension values are as optimal a dimension as you can get. Strings, numbers, and booleans
                // are treated as though they were explicitly hardcoded by the derive macro.
                syn::Lit::Str(lit_str) => {
                    quote! {
                        (#dimension_name, goodmetrics::Dimension::Str(#lit_str))
                    }
                }
                syn::Lit::Int(lit_int) => {
                    quote! {
                        (#dimension_name, goodmetrics::Dimension::Number(#lit_int))
                    }
                }
                syn::Lit::Bool(lit_bool) => {
                    quote! {
                        (#dimension_name, goodmetrics::Dimension::Boolean(#lit_bool))
                    }
                }
                _other_literal => {
                    return Err(syn::Error::new_spanned(
                        expr,
                        format!(
                            "expected attribute to be a dimension value: `{} = <\"...\", 123, true>`",
                            meta_item_name
                        ),
                    ));
                }
            }
        }
        _ => {
            return Err(syn::Error::new_spanned(
                expr,
                format!(
                    "expected attribute to be a dimension value: `{} = <\"...\", 123, true>`",
                    meta_item_name
                ),
            ));
        }
    })
}
