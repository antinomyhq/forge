use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Lit, parse_macro_input};

/// Generates a function that returns a mapping of aliases to full tool names
/// by automatically extracting serde aliases from the Tools enum
pub fn generate_alias_map(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let enum_data = match input.data {
        Data::Enum(data) => data,
        _ => panic!("AliasMap can only be used with enums"),
    };

    let mut alias_entries = Vec::new();

    for variant in enum_data.variants {
        let variant_name = variant.ident.to_string();
        let full_tool_name = convert_to_snake_case(&variant_name);

        // Extract serde aliases from the variant attributes
        for attr in &variant.attrs {
            if attr.path().is_ident("serde")
                && attr
                    .parse_nested_meta(|meta| {
                        if meta.path.is_ident("alias") {
                            let value: Lit = meta.value()?.parse()?;
                            if let Lit::Str(alias_str) = value {
                                let alias = alias_str.value();
                                alias_entries.push(quote! {
                                    (#alias, #full_tool_name)
                                });
                            }
                        }
                        Ok(())
                    })
                    .is_ok()
            {
                // Successfully parsed
            }
        }
    }

    let expanded = quote! {
        pub fn get_tool_aliases() -> &'static [(&'static str, &'static str)] {
            &[
                #(#alias_entries),*
            ]
        }
    };

    TokenStream::from(expanded)
}

/// Convert PascalCase to snake_case
fn convert_to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_lowercase().next().unwrap());
    }

    result
}
