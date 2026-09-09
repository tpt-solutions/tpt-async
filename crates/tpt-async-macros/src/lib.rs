//! Proc-macro crate for `tpt-async`.
//!
//! Provides the [`macro@main`] attribute.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

/// Marks an `async fn main()` as the entry point, driving it with
/// `tpt_async_executor::LocalExecutor`.
///
/// # Example
///
/// ```rust,ignore
/// #[tpt_async::main]
/// async fn main() {
///     println!("hello from tpt-async");
/// }
/// ```
///
/// Expands to:
///
/// ```rust,ignore
/// fn main() {
///     tpt_async_executor::LocalExecutor::new().block_on(async {
///         // original body
///     });
/// }
/// ```
///
/// # Panics (compile-time)
///
/// - If applied to a function that is not named `main`.
/// - If applied to a non-`async` function.
#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = proc_macro2::TokenStream::from(attr);
    if !args.is_empty() {
        return syn::Error::new_spanned(args, "#[tpt_async::main] takes no arguments")
            .to_compile_error()
            .into();
    }

    let input = parse_macro_input!(item as ItemFn);

    if input.sig.asyncness.is_none() {
        return syn::Error::new_spanned(
            &input.sig.fn_token,
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

    let output = quote! {
        #(#attrs)*
        #vis fn main() {
            ::tpt_async_executor::LocalExecutor::new().block_on(async #body);
        }
    };

    output.into()
}
