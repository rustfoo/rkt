use proc_macro::TokenStream;

use devise::proc_macro2_diagnostics::SpanDiagnosticExt;
use devise::syn::{self, spanned::Spanned};
use devise::{DeriveGenerator, FromMeta, MapperBuild, Support, ValidatorBuild};

const ONE_DATABASE_ATTR: &str = "missing `#[database(\"name\")]` attribute";
const ONE_UNNAMED_FIELD: &str = "struct must have exactly one unnamed field";

#[derive(Debug, FromMeta)]
struct DatabaseAttribute {
    #[meta(naked)]
    name: String,
}

pub fn derive_database(input: TokenStream) -> TokenStream {
    DeriveGenerator::build_for(input, quote!(impl rkt_db_pools::Database))
        .support(Support::TupleStruct)
        .validator(ValidatorBuild::new().struct_validate(|_, s| {
            if s.fields.len() == 1 {
                Ok(())
            } else {
                Err(s.span().error(ONE_UNNAMED_FIELD))
            }
        }))
        .outer_mapper(MapperBuild::new().struct_map(|_, s| {
            let pool_type = match &s.fields {
                syn::Fields::Unnamed(f) => &f.unnamed[0].ty,
                _ => unreachable!("Support::TupleStruct"),
            };

            let decorated_type = &s.ident;
            let db_ty = quote_spanned!(decorated_type.span() =>
                <#decorated_type as rkt_db_pools::Database>
            );

            quote_spanned! { decorated_type.span() =>
                impl From<#pool_type> for #decorated_type {
                    fn from(pool: #pool_type) -> Self {
                        Self(pool)
                    }
                }

                impl std::ops::Deref for #decorated_type {
                    type Target = #pool_type;

                    fn deref(&self) -> &Self::Target {
                        &self.0
                    }
                }

                impl std::ops::DerefMut for #decorated_type {
                    fn deref_mut(&mut self) -> &mut Self::Target {
                        &mut self.0
                    }
                }

                #[rkt::async_trait]
                impl<'r> rkt::request::FromRequest<'r> for &'r #decorated_type {
                    type Error = ();

                    async fn from_request(
                        req: &'r rkt::request::Request<'_>
                    ) -> rkt::request::Outcome<Self, Self::Error> {
                        match #db_ty::fetch(req.rocket()) {
                            Some(db) => rkt::outcome::Outcome::Success(db),
                            None => rkt::outcome::Outcome::Error((
                                rkt::http::Status::InternalServerError, ()))
                        }
                    }
                }

                impl rkt::Sentinel for &#decorated_type {
                    fn abort(rocket: &rkt::Rocket<rkt::Ignite>) -> bool {
                        #db_ty::fetch(rocket).is_none()
                    }
                }
            }
        }))
        .outer_mapper(quote!(#[rkt::async_trait]))
        .inner_mapper(MapperBuild::new().try_struct_map(|_, s| {
            let db_name = DatabaseAttribute::one_from_attrs("database", &s.attrs)?
                .map(|attr| attr.name)
                .ok_or_else(|| s.span().error(ONE_DATABASE_ATTR))?;

            let fairing_name = format!("'{}' Database Pool", db_name);

            let pool_type = match &s.fields {
                syn::Fields::Unnamed(f) => &f.unnamed[0].ty,
                _ => unreachable!("Support::TupleStruct"),
            };

            Ok(quote_spanned! { pool_type.span() =>
                type Pool = #pool_type;

                const NAME: &'static str = #db_name;

                fn init() -> rkt_db_pools::Initializer<Self> {
                    rkt_db_pools::Initializer::with_name(#fairing_name)
                }
            })
        }))
        .to_tokens()
}
