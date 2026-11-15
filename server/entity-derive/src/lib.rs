use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, parse_macro_input};

#[proc_macro_derive(Entity)]
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

    let keys: Vec<_> = fields
        .named
        .iter()
        .filter(|field| field.attrs.iter().any(|attr| attr.path().is_ident("key")))
        .collect();
    if keys.is_empty() {
        return syn::Error::new_spanned(
            &input.ident,
            "Entity requires at least one #[key] field",
        )
        .to_compile_error()
        .into();
    }

    let flags_name = format_ident!("{}Fields", input.ident);
    let change_name = format_ident!("{}Change", input.ident);
    let model_name = &input.ident;
    let key_name = keys[0].ident.as_ref().expect("named key field");
    let other_keys = keys.iter().skip(1).map(|field| {
        let name = field.ident.as_ref().expect("named key field");
        quote! {
            let filter = filter.and(#model_name::fields().#name().eq(entity.#name.clone()));
        }
    });
    let visibility = &input.vis;
    let create_field_names: Vec<_> = fields
        .named
        .iter()
        .map(|field| field.ident.as_ref().expect("named field"))
        .collect();
    let create_setters = create_field_names.iter().map(|name| quote! { .#name(#name) });
    let constants = fields.named.iter().enumerate().map(|(index, field)| {
        let field_name = field.ident.as_ref().expect("named field");
        let name = format_ident!("{}", field_name.to_string().to_ascii_uppercase());
        let value = 1_u64 << index;
        quote! { const #name = #value; }
    });
    let setters = fields.named.iter().filter(|field| {
        !field.attrs.iter().any(|attr| attr.path().is_ident("key"))
    }).map(|field| {
        let field_name = field.ident.as_ref().expect("named field");
        let flag_name = format_ident!("{}", field_name.to_string().to_ascii_uppercase());
        let setter_name = format_ident!("set_{}", field_name);
        quote! {
            if fields.contains(#flags_name::#flag_name) {
                update.#setter_name(entity.#field_name.clone());
            }
        }
    });

    quote! {
        impl crate::domain::EntityCreate for #model_name {
            type Create = <Self as ::toasty::codegen_support::Model>::Create;

            fn create(self) -> Self::Create {
                let Self { #(#create_field_names),* } = self;
                #model_name::create() #(#create_setters)*
            }
        }

        bitflags::bitflags! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            #visibility struct #flags_name: u64 {
                #(#constants)*
            }
        }

        #[must_use]
        #visibility struct #change_name<'a> {
            pub entity: &'a #model_name,
            pub fields: #flags_name,
        }

        impl #change_name<'_> {
            pub async fn exec(
                self,
                executor: &mut dyn ::toasty::codegen_support::Executor,
            ) -> ::toasty::Result<()> {
                let Self { entity, fields } = self;
                if fields.is_empty() {
                    return Ok(());
                }
                let filter = #model_name::fields().#key_name().eq(entity.#key_name.clone());
                #(#other_keys)*
                let mut update = #model_name::filter(filter).update();
                #(#setters)*
                update.exec(executor).await
            }
        }
    }
    .into()
}
