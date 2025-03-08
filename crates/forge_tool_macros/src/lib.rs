use proc_macro::TokenStream;
use proc_macro2::TokenTree;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(ToolDescription)]
pub fn derive_description(input: TokenStream) -> TokenStream {
    // Parse the input struct or enum
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;

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
                    // Remove surrounding quotes from the doc string
                    let clean_str = str.trim_matches('"').to_string();
                    if !clean_str.is_empty() {
                        doc_lines.push(clean_str);
                    }
                }
            }
        }
    }

    // Join all lines with a space (or newline, if you prefer)
    if doc_lines.is_empty() {
        panic!("No doc comment found for {}", name);
    }
    let doc_string = doc_lines.join("\n").trim().to_string();

    // Generate an implementation of `ToolDescription` that returns the doc string
    let expanded = if generics.params.is_empty() {
        quote! {
            impl ToolDescription for #name {
                fn description(&self) -> String {
                    #doc_string.into()
                }
            }
        }
    } else {
        quote! {
            impl #generics ToolDescription for #name #generics {
                fn description(&self) -> String {
                    #doc_string.into()
                }
            }
        }
    };

    expanded.into()
}
