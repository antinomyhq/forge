use proc_macro::TokenStream;
use proc_macro2::TokenTree;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(Description)]
pub fn derive_description(input: TokenStream) -> TokenStream {
    // Parse the input struct or enum
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // Collect doc lines from all `#[doc = "..."]` attributes
    let mut doc_lines = Vec::new();
    for attr in &input.attrs {
        // Check if the attribute is `#[doc(...)]`
        if attr.path().is_ident("doc") {
            // `parse_nested_meta` calls the provided closure for each nested token
            // of the attribute (e.g., = "some doc string").

            for t in attr
                .to_token_stream()
                .into_iter()
                .filter_map(|t| match t {
                    TokenTree::Group(lit) => Some(lit.stream()),
                    _ => None,
                })
                .flatten()
            {
                if let TokenTree::Literal(lit) = t {
                    let str = lit.to_string();
                    if !str.is_empty() {
                        doc_lines.push(lit.to_string());
                    }
                }
            }
        }
    }

    // Join all lines with a space (or newline, if you prefer)
    if doc_lines.is_empty() {
        panic!("No doc comment found for {}", name);
    }
    let doc_string = doc_lines.join(" ");

    // Generate an implementation of `Description` that returns the doc string
    let expanded = quote! {
        impl Description for #name {
            fn description() -> &'static str {
                #doc_string
            }
        }
    };

    expanded.into()
}
