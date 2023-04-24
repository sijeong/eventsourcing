//! # EventSourcing Derive
//!
//! Macro implementations for custom derivations for the *eventsourcing* crate
#![recursion_limit = "128"]

// extern crate proc_macro;
// #[macro_use]
// extern crate quote;
// #[macro_use]
// extern crate syn;

use proc_macro::TokenStream;
// use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::Parse;
use syn::punctuated::Punctuated;
// use syn::synom::Synom;
use syn::token::Comma;
use syn::{parse_quote, Data, DataEnum, DeriveInput, Fields, Ident, LitStr, Path, Variant};

/// Derives the boilerplate code for a Dispatcher
#[proc_macro_derive(Dispatcher, attributes(aggregate))]
pub fn component(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    let gen = impl_component(&ast);
    gen
}

/// Derives the boilerplate code for an Event
#[proc_macro_derive(Event, attributes(event_type_version, event_source))]
pub fn component_event(input: TokenStream) -> TokenStream {
    let ast: DeriveInput = syn::parse(input).unwrap();
    let gen = match ast.data {
        Data::Enum(ref data_enum) => impl_component_event(&ast, data_enum),
        Data::Struct(_) => quote! {
            panic!("#[derive(Event)] is only defined for enums, not structs")
        }
        .into(),
        Data::Union(_) => quote! {
            panic!("#[derive(Event)] is only defined for enums, not unions")
        }
        .into(),
    };

    gen
}

struct EventSourceAttribute {
    event_source: LitStr,
}

impl Parse for EventSourceAttribute {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(EventSourceAttribute {
            event_source: input.parse()?,
        })
    }
}
struct EventTypeVersionAttribute {
    event_type_version: Ident,
}
impl Parse for EventTypeVersionAttribute {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(EventTypeVersionAttribute {
            event_type_version: input.parse()?,
        })
    }
}
struct AggregateAttribute {
    aggregate: Path,
}
impl Parse for AggregateAttribute {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(AggregateAttribute {
            aggregate: input.parse()?,
        })
    }
}

// impl Synom for EventSourceAttribute {
//     named!(parse -> Self, map!(
//         parens!(syn!(LitStr)),
//         |(_, event_source)| EventSourceAttribute { event_source }
//     ));
// }

// impl Synom for AggregateAttribute {
//     named!(parse -> Self, map!(
//         parens!(syn!(Path)),
//         |(_, aggregate)| AggregateAttribute { aggregate }
//     ));
// }

// impl Synom for EventTypeVersionAttribute {
//     named!(parse -> Self, map!(
//         parens!(syn!(Ident)),
//         |(_, event_type_version) | EventTypeVersionAttribute { event_type_version }
//     ));
// }

fn impl_component_event(ast: &DeriveInput, data_enum: &DataEnum) -> TokenStream {
    let name = &ast.ident;
    let variants = &data_enum.variants;
    let (impl_generics, _ty_generics, where_clause) = ast.generics.split_for_impl();
    let event_type_version = ast
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("event_type_version"))
        .map(|attr| {
            // syn::parse2::<EventTypeVersionAttribute>(attr.tts.clone())
            attr.parse_args::<EventTypeVersionAttribute>()
                .unwrap()
                .event_type_version
        })
        .unwrap_or_else(|| parse_quote!(NoSchemaVersion));

    let event_source = ast
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("event_source"))
        .map(|attr| {
            // syn::parse2::<EventSourceAttribute>(attr.tts.clone())
            attr.parse_args::<EventSourceAttribute>()
                .unwrap()
                .event_source
        })
        .unwrap_or_else(|| parse_quote!(NoEventSource));

    let event_matches = generate_event_matches(&name, &variants);

    quote! {
        impl #impl_generics ::eventsourcing::Event for #name #where_clause {
            fn event_type_version(&self) -> &str {
                #event_type_version
            }

            fn event_source(&self) -> &str {
                #event_source
            }

            fn event_type(&self) -> &str {
                match self {
                    #(#event_matches)*
                }
            }
        }
        #[cfg(feature = "orgeventstore")]
        impl From<::eventsourcing::cloudevents::CloudEvent> for #name {
            fn from(__source: ::eventsourcing::cloudevents::CloudEvent) -> Self {
                ::serde_json::from_str(&::serde_json::to_string(&__source.data).unwrap()).unwrap()
            }
        }
    }
    .into()
}

fn generate_event_matches(name: &Ident, variants: &Punctuated<Variant, Comma>) -> Vec<proc_macro2::TokenStream> {
    let mut result: Vec<proc_macro2::TokenStream> = Vec::new();
    for (_idx, variant) in variants.iter().enumerate() {
        let id = &variant.ident;
        let et_name = event_type_name(name, id);
        let new = match variant.fields {
            Fields::Unit => quote! {
                #name::#id => #et_name,
            },
            Fields::Unnamed(ref fields) => {
                let idents: Vec<_> = fields.unnamed.pairs().map(|p| &p.value().ident).collect();
                quote! {
                    #name::#id( #(_ #idents,)* ) => #et_name,
                }
            }
            Fields::Named(ref fields) => {
                let idents: Vec<_> = fields.named.pairs().map(|p| &p.value().ident).collect();
                quote! {
                    #name::#id { #(#idents: _,)* } => #et_name,
                }
            }
        };
        result.push(new);
    }
    result
}

fn event_type_name(name: &Ident, variant_id: &Ident) -> String {
    let name_s = name.to_string().to_lowercase();
    let variant_s = variant_id.to_string().to_lowercase();
    format!("{}.{}", name_s, variant_s)
}

fn impl_component(ast: &DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let (impl_generics, _ty_generics, where_clause) = ast.generics.split_for_impl();

    let aggregate = ast
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("aggregate"))
        .map(|attr| {
            // syn::parse2::<AggregateAttribute>(attr.tts.clone())
            //     .unwrap()
            //     .aggregate
            attr.parse_args::<AggregateAttribute>().unwrap().aggregate
        })
        .unwrap_or_else(|| parse_quote!(NoAggregate));

    quote! {
        impl #impl_generics ::eventsourcing::Dispatcher for #name #where_clause {
            type Aggregate = #aggregate;
            type Event = <#aggregate as Aggregate>::Event;
            type Command = <#aggregate as Aggregate>::Command;
            type State = <#aggregate as Aggregate>::State;

            fn dispatch(
                state: &Self::State,
                cmd: &Self::Command,
                store: &impl ::eventsourcing::eventstore::EventStore,
                stream: &str,
            ) -> Vec<Result<::eventsourcing::cloudevents::CloudEvent>> {
                match Self::Aggregate::handle_command(state, cmd) {
                    Ok(evts) => evts.into_iter().map(|evt| store.append(evt, stream)).collect(),
                    Err(e) => vec![Err(e)],
                }
            }
        }
    }
    .into()
}
