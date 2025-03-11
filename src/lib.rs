extern crate proc_macro;
use proc_macro::TokenStream;
use quote::quote;
use syn::{AttributeArgs, ItemEnum, Lit, Meta, NestedMeta, parse_macro_input};

#[proc_macro_attribute]
pub fn derive_fromstr(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse attribute arguments as a list, e.g. [trim, lowercase]
    let args = parse_macro_input!(attr as AttributeArgs);
    let has_trim = args.iter().any(|arg| {
        if let NestedMeta::Meta(Meta::Path(path)) = arg {
            path.is_ident("trim")
        } else {
            false
        }
    });
    let has_lowercase = args.iter().any(|arg| {
        if let NestedMeta::Meta(Meta::Path(path)) = arg {
            path.is_ident("lowercase")
        } else {
            false
        }
    });
    // Parse truncate argument
    let truncate_value: Option<usize> = args.iter().find_map(|arg| {
        if let NestedMeta::Meta(Meta::List(meta_list)) = arg {
            if meta_list.path.is_ident("truncate") && meta_list.nested.len() == 1 {
                let first = meta_list.nested.first().unwrap();
                if let NestedMeta::Lit(Lit::Int(lit_int)) = first {
                    return Some(lit_int.base10_parse::<usize>().unwrap());
                }
            }
        }
        None
    });

    // Parse the input tokens into an enum
    let input = parse_macro_input!(item as ItemEnum);
    let enum_name = &input.ident;
    let variants = &input.variants;

    // Check each variant to ensure it doesn't hold data
    for variant in variants {
        match variant.fields {
            syn::Fields::Unit => {}
            _ => {
                return syn::Error::new_spanned(variant, concat!(env!("CARGO_PKG_NAME"), ": variants with data are not supported"))
                    .to_compile_error()
                    .into();
            }
        }
    }

    // Generate match arms for each enum variant.
    let mut arms_vec = variants
        .iter()
        .map(|variant| {
            let var_ident = &variant.ident;
            let var_name = var_ident.to_string();
            let expected = if has_lowercase { var_name.to_lowercase() } else { var_name };
            quote! {
                #expected => Ok(#enum_name::#var_ident),
            }
        })
        .collect::<Vec<_>>();

    // Add extra match arms for truncated variant names if truncate_value is provided.
    if let Some(trunc) = truncate_value {
        for variant in variants {
            let var_ident = &variant.ident;
            let full_name = var_ident.to_string();
            if full_name.len() > trunc {
                let truncated = &full_name[..trunc];
                let truncated = if has_lowercase {
                    truncated.to_lowercase()
                } else {
                    truncated.to_string()
                };
                let original = if has_lowercase { full_name.to_lowercase() } else { full_name.clone() };
                if truncated != original {
                    arms_vec.push(quote! {
                        #truncated => Ok(#enum_name::#var_ident),
                    });
                }
            }
        }
    }

    // Generate code to transform the input string based on flags.
    let transform = if has_trim && has_lowercase {
        quote! {
            let temp = s.trim().to_lowercase();
            let s = temp.as_str();
        }
    } else if has_trim {
        quote! {
            let s = s.trim();
        }
    } else if has_lowercase {
        quote! {
            let temp = s.to_lowercase();
            let s = temp.as_str();
        }
    } else {
        quote! {}
    };

    // Generate an error enum named Parse{EnumName}Error with required derives.
    let error_enum_ident = syn::Ident::new(&format!("Parse{}Error", enum_name), enum_name.span());
    let error_enum = quote! {
        #[derive(Debug, PartialEq, Eq)]
        pub enum #error_enum_ident {
            UnknownVariant(String),
        }
    };

    // Generate the final tokens including the enum, error enum,
    // and the FromStr implementation using the new error enum.
    let gener = quote! {
        #input
        #error_enum
        impl ::core::str::FromStr for #enum_name {
            type Err = #error_enum_ident;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                #transform
                match s {
                    #( #arms_vec )*
                    _ => Err(#error_enum_ident::UnknownVariant(s.to_string())),
                }
            }
        }

        impl ::core::error::Error for #error_enum_ident {}

        impl ::core::fmt::Display for #error_enum_ident {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    #error_enum_ident::UnknownVariant(s) => write!(f, "Unknown variant: {}", s),
                }
            }
        }
    };

    gener.into()
}
