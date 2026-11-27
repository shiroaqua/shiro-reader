use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, parse_macro_input};

/// ORM-agnostic domain entity macro.
///
/// Generates:
/// - `<Entity>Fields`: `bitflags` bitmask covering each named field (up to 64 fields).
/// - `<Entity>Change<'a>`: `#[must_use]` dirty-field patch carrying a reference to the entity
///   and the modified `<Entity>Fields` bitmask.
/// - `<Entity>Inner`: owned plain data representation for zero-copy dismantling into DTOs and persistence records.
/// - `<Entity>::into_inner`: zero-copy conversion into `<Entity>Inner`.
#[proc_macro_derive(Entity, attributes(key))]
pub fn derive_entity(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let Data::Struct(data) = &input.data else {
        return syn::Error::new_spanned(&input.ident, "Entity requires a struct")
            .to_compile_error()
            .into();
    };
    let Fields::Named(fields) = &data.fields else {
        return syn::Error::new_spanned(&input.ident, "Entity requires named fields")
            .to_compile_error()
            .into();
    };
    if fields.named.len() > 64 {
        return syn::Error::new_spanned(&input.ident, "Entity supports at most 64 fields")
            .to_compile_error()
            .into();
    }

    let has_key = fields
        .named
        .iter()
        .any(|field| field.attrs.iter().any(|attr| attr.path().is_ident("key")));
    if !has_key {
        return syn::Error::new_spanned(&input.ident, "Entity requires at least one #[key] field")
            .to_compile_error()
            .into();
    }

    let model_name = &input.ident;
    let flags_name = format_ident!("{}Fields", model_name);
    let change_name = format_ident!("{}Change", model_name);
    let inner_name = format_ident!("{}Inner", model_name);
    let visibility = &input.vis;

    let field_names: Vec<_> = fields
        .named
        .iter()
        .map(|field| field.ident.as_ref().expect("named field"))
        .collect();

    let inner_fields = fields.named.iter().map(|field| {
        let name = field.ident.as_ref().expect("named field");
        let ty = &field.ty;
        quote! { pub #name: #ty }
    });

    let constants = fields.named.iter().enumerate().map(|(index, field)| {
        let field_name = field.ident.as_ref().expect("named field");
        let const_name = format_ident!("{}", field_name.to_string().to_ascii_uppercase());
        let value = 1_u64 << index;
        quote! { const #const_name = #value; }
    });

    quote! {
        ::bitflags::bitflags! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            #visibility struct #flags_name: u64 {
                #(#constants)*
            }
        }

        #[must_use]
        #[derive(Debug)]
        #visibility struct #change_name<'a> {
            entity: &'a #model_name,
            fields: #flags_name,
        }

        impl<'a> #change_name<'a> {
            pub const fn new(entity: &'a #model_name, fields: #flags_name) -> Self {
                Self { entity, fields }
            }

            pub const fn entity(&self) -> &'a #model_name {
                self.entity
            }

            pub const fn fields(&self) -> #flags_name {
                self.fields
            }

            pub const fn is_empty(&self) -> bool {
                self.fields.is_empty()
            }
        }

        #[derive(Debug, Clone)]
        #visibility struct #inner_name {
            #(#inner_fields,)*
        }

        impl #model_name {
            pub fn into_inner(self) -> #inner_name {
                let Self { #(#field_names),* } = self;
                #inner_name { #(#field_names),* }
            }
        }
    }
    .into()
}
