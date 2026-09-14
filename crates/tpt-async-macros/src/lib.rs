// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Proc-macro crate for `tpt-async`.
//!
//! Provides the [`macro@main`] attribute, which drives an `async fn main` on
//! the facade's executor via `tpt_async::__private::LocalExecutor` — so
//! depending on `tpt-async` (with the default features) is all that is
//! required; no direct `tpt-async-executor` dependency needed.

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::Parser as _;
use syn::{parse_macro_input, ItemFn, LitStr, Path, ReturnType};

/// Marks an `async fn main()` as the entry point, driving it with the
/// `tpt-async` executor.
///
/// The async function's return value is the process exit value: returning
/// `()` exits successfully, and any type implementing
/// `Termination` (e.g. `Result<(), E: Debug>`)
/// is forwarded to the runtime, which reports `Err` values and exits with a
/// failure code.
///
/// # Example
///
/// ```rust,ignore
/// use tpt_async::prelude::*;
///
/// #[tpt_async::main]
/// async fn main() -> Result<(), std::io::Error> {
///     println!("hello from tpt-async");
///     Ok(())
/// }
/// ```
///
/// # Requirements
///
/// The invoking crate must depend on the `tpt-async` facade (the macro
/// expands to `tpt_async::__private::…`, which the facade always provides).
///
/// # Panics (compile-time)
///
/// - If applied to a function that is not named `main`.
/// - If applied to a non-`async` function.
/// - If unknown arguments are passed, or `executor = "…"` is not a string
///   literal containing a valid path (or `"default"`).
#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse `#[tpt_async::main(executor = "…")]` (the only supported arg).
    let mut executor: Option<Path> = None;
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("executor") {
            let value = meta.value()?;
            let lit: LitStr = value.parse()?;
            if lit.value() == "default" {
                executor = None;
            } else {
                match lit.parse::<Path>() {
                    Ok(path) => executor = Some(path),
                    Err(e) => {
                        return Err(syn::Error::new(
                            e.span(),
                            "executor must be \"default\" or a valid path string",
                        ));
                    }
                }
            }
            Ok(())
        } else {
            Err(meta.error("unsupported argument; expected `executor = \"…\"`"))
        }
    });
    let args = proc_macro2::TokenStream::from(attr);
    if let Err(e) = parser.parse2(args) {
        return e.to_compile_error().into();
    }

    let input = parse_macro_input!(item as ItemFn);

    if input.sig.asyncness.is_none() {
        return syn::Error::new_spanned(
            input.sig.fn_token,
            "#[tpt_async::main] can only be applied to an `async fn`",
        )
        .to_compile_error()
        .into();
    }

    if input.sig.ident != "main" {
        return syn::Error::new_spanned(
            &input.sig.ident,
            "#[tpt_async::main] can only be applied to `fn main`",
        )
        .to_compile_error()
        .into();
    }

    let body = &input.block;
    let attrs = &input.attrs;
    let vis = &input.vis;

    // Preserve a non-unit return type so `Result` exits report failures via
    // `Termination`; a unit return is discarded as before.
    let returns_value = !matches!(input.sig.output, ReturnType::Default);
    let output = if let Some(runner) = executor {
        // Third-party runtime: call the given `fn(Future) -> Output`.
        let ret = &input.sig.output;
        if returns_value {
            quote! {
                #(#attrs)*
                #vis fn main() #ret {
                    #runner(async #body)
                }
            }
        } else {
            quote! {
                #(#attrs)*
                #vis fn main() {
                    #runner(async #body);
                }
            }
        }
    } else if returns_value {
        let ret = &input.sig.output;
        quote! {
            #(#attrs)*
            #vis fn main() #ret {
                ::tpt_async::__private::LocalExecutor::new().block_on(async #body)
            }
        }
    } else {
        quote! {
            #(#attrs)*
            #vis fn main() {
                ::tpt_async::__private::LocalExecutor::new().block_on(async #body);
            }
        }
    };

    output.into()
}
