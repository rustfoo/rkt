use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, TokenStreamExt};

#[derive(Debug, Copy, Clone)]
pub struct StaticPath(pub Option<Span>, pub &'static str);

#[derive(Debug, Copy, Clone)]
pub struct StaticTokens(pub fn() -> TokenStream);

macro_rules! quote_static {
    ($($token:tt)*) => {
        $crate::exports::StaticTokens(|| quote!($($token)*))
    }
}

impl ToTokens for StaticTokens {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.append_all((self.0)());
    }
}

impl StaticPath {
    pub fn respanned(mut self, span: Span) -> Self {
        self.0 = Some(span);
        self
    }
}

impl ToTokens for StaticPath {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let path: syn::Path = syn::parse_str(self.1).unwrap();
        if let Some(span) = self.0 {
            let new_tokens = path.into_token_stream().into_iter().map(|mut t| {
                t.set_span(span);
                t
            });

            tokens.append_all(new_tokens);
        } else {
            path.to_tokens(tokens)
        }
    }
}

macro_rules! define_exported_paths {
    ($($name:ident => $path:path),* $(,)?) => {
        $(
            #[allow(dead_code)]
            #[allow(non_upper_case_globals)]
            pub const $name: StaticPath = $crate::exports::StaticPath(None, stringify!($path));
        )*

        macro_rules! define {
            // Note: the `i` is to capture the input's span.
            $(($span:expr => $i:ident $name) => {
                #[allow(non_snake_case)]
                let $i = $crate::exports::StaticPath(Some($span), stringify!($path));
            };)*
        }
    };
}

define_exported_paths! {
    __req => __req,
    __status => __status,
    __catcher => __catcher,
    __data => __data,
    __error => __error,
    __trail => __trail,
    _request => ::rkt::request,
    _response => ::rkt::response,
    _route => ::rkt::route,
    _error => ::rkt::error,
    _catcher => ::rkt::catcher,
    _sentinel => ::rkt::sentinel,
    _form => ::rkt::form::prelude,
    _http => ::rkt::http,
    _uri => ::rkt::http::uri,
    _fmt => ::rkt::http::uri::fmt,
    _Option => ::std::option::Option,
    _Result => ::std::result::Result,
    _Some => ::std::option::Option::Some,
    _None => ::std::option::Option::None,
    _Ok => ::std::result::Result::Ok,
    _Err => ::std::result::Result::Err,
    _Box => ::std::boxed::Box,
    _Vec => ::std::vec::Vec,
    _Cow => ::std::borrow::Cow,
    _ExitCode => ::std::process::ExitCode,
    display_hack => ::rkt::error::display_hack,
    BorrowMut => ::std::borrow::BorrowMut,
    Outcome => ::rkt::outcome::Outcome,
    FromForm => ::rkt::form::FromForm,
    FromRequest => ::rkt::request::FromRequest,
    FromData => ::rkt::data::FromData,
    FromSegments => ::rkt::request::FromSegments,
    FromParam => ::rkt::request::FromParam,
    Request => ::rkt::request::Request,
    Response => ::rkt::response::Response,
    Data => ::rkt::data::Data,
    StaticRouteInfo => ::rkt::StaticRouteInfo,
    StaticCatcherInfo => ::rkt::StaticCatcherInfo,
    Route => ::rkt::Route,
    Catcher => ::rkt::Catcher,
    Status => ::rkt::http::Status,
}

macro_rules! define_spanned_export {
    ($span:expr => $($name:ident),*) => ($(define!($span => $name $name);)*)
}

/// Convenience: returns a "mixed site" span located at `span`.
#[inline(always)]
pub fn mixed(span: Span) -> Span {
    Span::mixed_site().located_at(span)
}
