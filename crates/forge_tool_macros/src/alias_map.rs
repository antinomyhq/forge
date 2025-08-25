use convert_case::{Case, Casing};

/// Extract the rename_all case from serde attributes
fn extract_rename_all_case(attrs: &[syn::Attribute]) -> Case<'static> {
    attrs
        .iter()
        .find_map(|attr| {
            if !attr.path().is_ident("serde") {
                return None;
            }

            let mut case = None;
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename_all")
                    && let Ok(Lit::Str(rename_str)) = meta.value()?.parse::<Lit>()
                {
                    case = Some(parse_serde_case(&rename_str.value()));
                }
                Ok(())
            });
            case
        })
        .unwrap_or(Case::Snake)
}

/// Parse serde rename_all values to convert_case::Case
fn parse_serde_case(rename_value: &str) -> Case<'static> {
    match rename_value {
        "snake_case" => Case::Snake,
        "camelCase" => Case::Camel,
        "PascalCase" => Case::Pascal,
        "kebab-case" => Case::Kebab,
        "SCREAMING_SNAKE_CASE" => Case::UpperSnake,
        _ => Case::Snake, // Default fallback
    }
}

/// Apply case conversion based on the rename_all attribute
fn apply_case_conversion(name: &str, case: Case) -> String {
    name.to_case(case)
}
use quote::quote;
use syn::{Data, DeriveInput, Lit, parse_macro_input};

/// Attribute macro implementation that generates both the original enum and the
/// alias mapping
pub fn generate_tool_alias_map_attribute(
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let enum_data = match &input.data {
        Data::Enum(data) => data,
        _ => panic!("tool_alias_map can only be used with enums"),
    };

    // Extract rename_all attribute from enum serde attributes
    let rename_all_case = extract_rename_all_case(&input.attrs);

    let mut alias_entries = Vec::new();

    for variant in &enum_data.variants {
        let variant_name = variant.ident.to_string();
        let full_tool_name = apply_case_conversion(&variant_name, rename_all_case);

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

    let enum_name = &input.ident;
    let vis = &input.vis;
    let attrs = &input.attrs;
    let generics = &input.generics;
    let variants = &enum_data.variants;

    let expanded = quote! {
        #(#attrs)*
        #vis enum #enum_name #generics {
            #variants
        }

        pub fn get_tool_aliases() -> &'static [(&'static str, &'static str)] {
            &[
                #(#alias_entries),*
            ]
        }
    };

    proc_macro::TokenStream::from(expanded)
}
