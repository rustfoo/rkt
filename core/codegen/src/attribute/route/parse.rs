use devise::ext::{SpanDiagnosticExt, TypeExt};
use devise::{FromMeta, Result, SpanWrapped, Spanned};
use indexmap::{IndexMap, IndexSet};
use proc_macro2::Span;
use ubyte::ByteUnit;

use crate::attribute::param::{Dynamic, Guard, Parameter};
use crate::attribute::suppress::Lint;
use crate::http::ext::IntoOwned;
use crate::http::{parse_duration, Rate};
use crate::http::uri::{fmt, Origin};
use crate::http_codegen::{MediaType, Method};
use crate::name::Name;
use crate::proc_macro_ext::{Diagnostics, StringLit};
use crate::syn_ext::FnArgExt;

/// This structure represents the parsed `route` attribute and associated items.
#[derive(Debug)]
pub struct Route {
    /// The attribute: `#[get(path, ...)]`.
    pub attr: Attribute,
    /// The static and dynamic path parameters.
    pub path_params: Vec<Parameter>,
    /// The static and dynamic query parameters.
    pub query_params: Vec<Parameter>,
    /// The data guard, if any.
    pub data_guard: Option<Guard>,
    /// The request guards.
    pub request_guards: Vec<Guard>,
    /// The decorated function: the handler.
    pub handler: syn::ItemFn,
    /// The parsed arguments to the user's function.
    pub arguments: Arguments,
}

type ArgumentMap = IndexMap<Name, (syn::Ident, syn::Type)>;

#[derive(Debug)]
pub struct Arguments {
    pub span: Span,
    pub map: ArgumentMap,
}

/// The parsed `#[route(..)]` attribute.
#[derive(Debug, FromMeta)]
pub struct Attribute {
    #[meta(naked)]
    pub uri: RouteUri,
    pub method: Option<SpanWrapped<Method>>,
    pub data: Option<SpanWrapped<Dynamic>>,
    pub format: Option<MediaType>,
    pub rank: Option<isize>,
    pub body_timeout: Option<SpanWrapped<BodyTimeout>>,
    pub body_min_rate: Option<SpanWrapped<BodyMinRate>>,
    pub body_grace: Option<SpanWrapped<BodyGrace>>,
    pub body_policy: Option<SpanWrapped<BodyPolicy>>,
    #[allow(dead_code)] // Emitted as route metadata in a later step.
    pub body_limit: Option<SpanWrapped<BodyLimit>>,
}

/// The parsed `#[method(..)]` (e.g, `get`, `put`, etc.) attribute.
#[derive(Debug, FromMeta)]
pub struct MethodAttribute {
    #[meta(naked)]
    pub uri: RouteUri,
    pub data: Option<SpanWrapped<Dynamic>>,
    pub format: Option<MediaType>,
    pub rank: Option<isize>,
    pub body_timeout: Option<SpanWrapped<BodyTimeout>>,
    pub body_min_rate: Option<SpanWrapped<BodyMinRate>>,
    pub body_grace: Option<SpanWrapped<BodyGrace>>,
    pub body_policy: Option<SpanWrapped<BodyPolicy>>,
    pub body_limit: Option<SpanWrapped<BodyLimit>>,
}

/// The parsed value of `body_timeout`: a non-zero duration.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // Emitted as route metadata in a later step.
pub struct BodyTimeout(pub std::time::Duration);

/// The parsed value of `body_min_rate`: a non-zero rate.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // Emitted as route metadata in a later step.
pub struct BodyMinRate(pub Rate);

/// The parsed value of `body_grace`: a non-zero duration.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // Emitted as route metadata in a later step.
pub struct BodyGrace(pub std::time::Duration);

/// The parsed value of `body_policy`: only `"unlimited"` is accepted.
#[derive(Debug, Clone, Copy)]
pub struct BodyPolicy;

/// The parsed value of `body_limit`: a non-zero number of bytes.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // Emitted as route metadata in a later step.
pub struct BodyLimit(pub ByteUnit);

fn parse_body_duration(meta: &devise::MetaItem) -> Result<std::time::Duration> {
    let string = StringLit::from_meta(meta)?;
    parse_duration(&string).map_err(|e| {
        string.span()
            .error(format!("invalid duration: {}", e))
            .help("expected a duration such as \"30s\", \"250ms\", or \"1m30s\"")
    })
}

impl FromMeta for BodyTimeout {
    fn from_meta(meta: &devise::MetaItem) -> Result<Self> {
        parse_body_duration(meta).map(BodyTimeout)
    }
}

impl FromMeta for BodyGrace {
    fn from_meta(meta: &devise::MetaItem) -> Result<Self> {
        parse_body_duration(meta).map(BodyGrace)
    }
}

impl FromMeta for BodyMinRate {
    fn from_meta(meta: &devise::MetaItem) -> Result<Self> {
        let string = StringLit::from_meta(meta)?;
        string.parse::<Rate>().map(BodyMinRate).map_err(|e| {
            string.span()
                .error(format!("invalid rate: {}", e))
                .help("expected a rate such as \"8KiB/s\" or \"1MiB/10s\"")
        })
    }
}

impl FromMeta for BodyPolicy {
    fn from_meta(meta: &devise::MetaItem) -> Result<Self> {
        let string = StringLit::from_meta(meta)?;
        match &*string {
            "unlimited" => Ok(BodyPolicy),
            _ => Err(string.span()
                .error("invalid body policy")
                .help("the only supported body policy is \"unlimited\"")),
        }
    }
}

impl FromMeta for BodyLimit {
    fn from_meta(meta: &devise::MetaItem) -> Result<Self> {
        let string = StringLit::from_meta(meta)?;
        let limit = string.parse::<ByteUnit>().map_err(|e| {
            string.span()
                .error(format!("invalid body limit: {}", e))
                .help("expected a byte size such as \"2MiB\" or \"64KiB\"")
        })?;

        if limit.as_u64() == 0 {
            return Err(string.span()
                .error("body limit must be non-zero")
                .help("the default body limit is finite and nonzero; use a non-zero size"));
        }

        Ok(BodyLimit(limit))
    }
}

#[derive(Debug)]
pub struct RouteUri {
    origin: Origin<'static>,
    path_span: Span,
    query_span: Option<Span>,
}

impl FromMeta for RouteUri {
    fn from_meta(meta: &devise::MetaItem) -> Result<Self> {
        let string = crate::proc_macro_ext::StringLit::from_meta(meta)?;

        let origin = Origin::parse_route(&string).map_err(|e| {
            let span = string.subspan(e.index() + 1..(e.index() + 2));
            span.error(format!("invalid route URI: {}", e))
                .help("expected URI in origin form: \"/path/<param>\"")
        })?;

        if !origin.is_normalized() {
            let normalized = origin.clone().into_normalized();
            let span = origin
                .path()
                .find("//")
                .or_else(|| {
                    origin
                        .query()
                        .and_then(|q| q.find("&&"))
                        .map(|i| origin.path().len() + 1 + i)
                })
                .map(|i| string.subspan((1 + i)..(1 + i + 2)))
                .unwrap_or_else(|| string.span());

            return Err(span
                .error("route URIs cannot contain empty segments")
                .note(format!("expected \"{}\", found \"{}\"", normalized, origin)));
        }

        let path_span = string.subspan(1..origin.path().len() + 1);
        let query_span = origin.query().map(|q| {
            let len_to_q = 1 + origin.path().len() + 1;
            let end_of_q = len_to_q + q.len();
            string.subspan(len_to_q..end_of_q)
        });

        Ok(RouteUri {
            origin: origin.into_owned(),
            path_span,
            query_span,
        })
    }
}

impl Route {
    pub fn upgrade_param(param: Parameter, args: &Arguments) -> Result<Parameter> {
        if param.dynamic().is_none() {
            return Ok(param);
        }

        let param = param.take_dynamic().expect("dynamic() => take_dynamic()");
        Route::upgrade_dynamic(param, args).map(Parameter::Guard)
    }

    pub fn upgrade_dynamic(param: Dynamic, args: &Arguments) -> Result<Guard> {
        if let Some((ident, ty)) = args.map.get(&param.name) {
            Ok(Guard::from(param, ident.clone(), ty.clone()))
        } else {
            let msg = format!("expected argument named `{}` here", param.name);
            let diag = param
                .span()
                .error("unused parameter")
                .span_note(args.span, msg);
            Err(diag)
        }
    }

    pub fn from(attr: Attribute, handler: syn::ItemFn) -> Result<Route> {
        // Collect diagnostics as we proceed.
        let mut diags = Diagnostics::new();

        // Emit a warning if a `data` param was supplied for non-payload methods.
        if let Some(ref data) = attr.data {
            let lint = Lint::DubiousPayload;
            match attr.method.as_ref() {
                Some(m) if m.0.allows_request_body() == Some(false) => diags.push(
                    data.full_span
                        .error("`data` cannot be used on this route")
                        .span_note(m.span, "method does not support request payloads"),
                ),
                Some(m) if m.0.allows_request_body().is_none() && lint.enabled(handler.span()) => {
                    data.full_span
                        .warning("`data` used with non-payload-supporting method")
                        .span_note(
                            m.span,
                            format!("'{}' does not typically support payloads", m.0),
                        )
                        .note(lint.how_to_suppress())
                        .emit_as_item_tokens();
                }
                None if lint.enabled(handler.span()) => {
                    data.full_span
                        .warning("`data` used on route with wildcard method")
                        .note("some methods may not support request payloads")
                        .note(lint.how_to_suppress())
                        .emit_as_item_tokens();
                }
                _ => { /* okay */ }
            }
        }

        // Validate the body policy options: at most one progress policy, and
        // a grace period only alongside a minimum rate.
        let progress = [
            attr.body_timeout.as_ref().map(|o| ("body_timeout", o.full_span)),
            attr.body_min_rate.as_ref().map(|o| ("body_min_rate", o.full_span)),
            attr.body_policy.as_ref().map(|o| ("body_policy", o.full_span)),
        ];

        let mut progress = progress.into_iter().flatten();
        if let Some((first, first_span)) = progress.next() {
            for (name, span) in progress {
                diags.push(
                    span.error(format!("`{}` cannot be used with `{}`", name, first))
                        .span_note(first_span, "the progress policy was first set here")
                        .help("a route can have only one of `body_timeout`, `body_min_rate`, \
                               and `body_policy`"),
                );
            }
        }

        if let (Some(grace), None) = (&attr.body_grace, &attr.body_min_rate) {
            diags.push(
                grace.full_span
                    .error("`body_grace` requires `body_min_rate`")
                    .help("the grace period applies only to rate-based body policies"),
            );
        }

        // Check the validity of function arguments.
        let span = handler.sig.paren_token.span.join();
        let mut arguments = Arguments {
            map: ArgumentMap::new(),
            span,
        };
        for arg in &handler.sig.inputs {
            if let Some((ident, ty)) = arg.typed() {
                let value = (ident.clone(), ty.with_stripped_lifetimes());
                arguments.map.insert(Name::from(ident), value);
            } else {
                let span = arg.span();
                let diag = if arg.wild().is_some() {
                    span.error("handler arguments must be named")
                        .help("to name an ignored handler argument, use `_name`")
                } else {
                    span.error("handler arguments must be of the form `ident: Type`")
                };

                diags.push(diag);
            }
        }

        // Parse and collect the path parameters.
        let (source, span) = (attr.uri.path(), attr.uri.path_span);
        let path_params = Parameter::parse_many::<fmt::Path>(source.as_str(), span)
            .map(|p| Route::upgrade_param(p?, &arguments))
            .filter_map(|p| p.map_err(|e| diags.push(e)).ok())
            .collect::<Vec<_>>();

        // Parse and collect the query parameters.
        let query_params = match (attr.uri.query(), attr.uri.query_span) {
            (Some(q), Some(span)) => Parameter::parse_many::<fmt::Query>(q.as_str(), span)
                .map(|p| Route::upgrade_param(p?, &arguments))
                .filter_map(|p| p.map_err(|e| diags.push(e)).ok())
                .collect::<Vec<_>>(),
            _ => vec![],
        };

        // Remove the `SpanWrapped` layer and upgrade to a guard.
        let data_guard = attr
            .data
            .clone()
            .map(|p| Route::upgrade_dynamic(p.value, &arguments))
            .and_then(|p| p.map_err(|e| diags.push(e)).ok());

        // Collect all of the declared dynamic route parameters.
        let all_dyn_params = path_params
            .iter()
            .filter_map(|p| p.dynamic())
            .chain(query_params.iter().filter_map(|p| p.dynamic()))
            .chain(data_guard.as_ref().map(|g| &g.source));

        // Check for any duplicates in the dynamic route parameters.
        let mut dyn_params: IndexSet<&Dynamic> = IndexSet::new();
        for p in all_dyn_params {
            if let Some(prev) = dyn_params.replace(p) {
                diags.push(
                    p.span()
                        .error(format!("duplicate parameter: `{}`", p.name))
                        .span_note(prev.span(), "previous parameter with the same name here"),
                )
            }
        }

        // Collect the request guards: all the arguments not already a guard.
        let request_guards = arguments
            .map
            .iter()
            .filter(|(name, _)| {
                let mut all_other_guards = path_params
                    .iter()
                    .filter_map(|p| p.guard())
                    .chain(query_params.iter().filter_map(|p| p.guard()))
                    .chain(data_guard.as_ref());

                all_other_guards.all(|g| &g.name != *name)
            })
            .enumerate()
            .map(|(index, (name, (ident, ty)))| Guard {
                source: Dynamic {
                    index,
                    name: name.clone(),
                    trailing: false,
                },
                fn_ident: ident.clone(),
                ty: ty.clone(),
            })
            .collect();

        diags.head_err_or(Route {
            attr,
            path_params,
            query_params,
            data_guard,
            request_guards,
            handler,
            arguments,
        })
    }
}

impl std::ops::Deref for RouteUri {
    type Target = Origin<'static>;

    fn deref(&self) -> &Self::Target {
        &self.origin
    }
}
