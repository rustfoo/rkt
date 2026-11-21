use std::collections::HashMap;
use std::path::Path;

use rkt::serde::Serialize;

use crate::template::TemplateInfo;

#[cfg(feature = "tera")]
mod tera;
#[cfg(feature = "tera")]
use ::tera::Tera;
#[cfg(feature = "tera")]
pub(crate) use self::tera::DiskRenderers;

#[cfg(feature = "handlebars")]
mod handlebars;
#[cfg(feature = "handlebars")]
use ::handlebars::Handlebars;

#[cfg(feature = "minijinja")]
mod minijinja;
#[cfg(feature = "minijinja")]
use ::minijinja::Environment;

/// An error from [`Engines::load()`] that hasn't been logged yet.
#[cfg_attr(not(feature = "tera"), allow(dead_code))]
pub(crate) struct LoadError {
    #[cfg(feature = "tera")]
    tera: ::tera::Error,
}

impl LoadError {
    /// Whether templates use a filter, test, or function that isn't
    /// registered.
    pub(crate) fn is_unregistered_reference(&self) -> bool {
        #[cfg(feature = "tera")]
        return tera::is_unregistered_reference(&self.tera);

        #[cfg(not(feature = "tera"))]
        false
    }

    pub(crate) fn log(&self) {
        #[cfg(feature = "tera")]
        tera::log_load_error(&self.tera);
    }
}

#[cfg_attr(
    not(any(feature = "tera", feature = "handlebars", feature = "minijinja")),
    allow(dead_code)
)]
pub(crate) trait Engine: Send + Sync + Sized + 'static {
    const EXT: &'static str;

    fn init<'a>(templates: impl Iterator<Item = (&'a str, &'a Path)>) -> Option<Self>;
    fn render<C: Serialize>(&self, name: &str, context: C) -> Option<String>;
}

/// A structure exposing access to templating engines.
///
/// Calling methods on the exposed template engine types may require importing
/// types from the respective templating engine library. These types should be
/// imported from the reexported crate at the root of `rkt_dyn_templates` to
/// avoid version mismatches. For instance, when registering a Tera filter, the
/// [`tera::Value`] and [`tera::TeraResult`] types are required. Import them from
/// `rkt_dyn_templates::tera`. The example below illustrates this:
///
/// ```rust
/// # #[cfg(feature = "tera")] {
/// use rkt_dyn_templates::{Template, Engines};
/// use rkt_dyn_templates::tera::{self, Value};
///
/// fn my_filter(value: &Value, _: tera::Kwargs, _: &tera::State) -> tera::TeraResult<Value> {
///     # /*
///     ...
///     # */ unimplemented!();
/// }
///
/// fn main() {
///     rkt::build()
///         // ...
///         .attach(Template::custom(|engines: &mut Engines| {
///             engines.tera.register_filter("my_filter", my_filter);
///         }))
///         // ...
///         # ;
/// }
/// # }
/// ```
///
/// [`tera::Value`]: crate::tera::Value
/// [`tera::TeraResult`]: crate::tera::TeraResult
///
pub struct Engines {
    /// A `Tera` templating engine.
    ///
    /// This field is only available when the `tera` feature is enabled. When
    /// calling methods on the `Tera` instance, ensure you use types imported
    /// from `rkt_dyn_templates::tera` to avoid version mismatches. When
    /// rendering through Rocket, file-backed Tera templates use the file's
    /// extension to select HTML escaping. Templates registered directly with
    /// Tera use the suffixes configured by `Tera::autoescape_on()`. A template
    /// registered before Tera loads template files replaces a file with the
    /// same name, and a warning is logged.
    #[cfg(feature = "tera")]
    pub tera: Tera,

    /// The Handlebars templating engine.
    ///
    /// This field is only available when the `handlebars` feature is enabled.
    /// When calling methods on the `Handlebars` instance, ensure you use types
    /// imported from `rkt_dyn_templates::handlebars` to avoid version
    /// mismatches.
    #[cfg(feature = "handlebars")]
    pub handlebars: Handlebars<'static>,

    /// The minijinja templating engine.
    ///
    /// This field is only available when the `minijinja` feature is enabled.
    /// When calling methods on the [`Environment`] instance, ensure you use
    /// types imported from `rkt_dyn_templates::minijinja` to avoid version
    /// mismatches.
    #[cfg(feature = "minijinja")]
    pub minijinja: Environment<'static>,
}

#[cfg_attr(
    not(any(feature = "tera", feature = "handlebars", feature = "minijinja")),
    allow(dead_code)
)]
impl Engines {
    pub(crate) const ENABLED_EXTENSIONS: &'static [&'static str] = &[
        #[cfg(feature = "tera")]
        Tera::EXT,
        #[cfg(feature = "handlebars")]
        Handlebars::EXT,
        #[cfg(feature = "minijinja")]
        Environment::EXT,
    ];

    fn files<E: Engine>(templates: &HashMap<String, TemplateInfo>)
        -> impl Iterator<Item = (&str, &Path)>
    {
        templates.iter()
            .filter(|(_, info)| info.engine_ext == E::EXT)
            .filter_map(|(name, info)| Some((name.as_str(), info.path.as_deref()?)))
    }

    pub(crate) fn init(_templates: &HashMap<String, TemplateInfo>) -> Option<Engines> {
        Some(Engines {
            #[cfg(feature = "tera")]
            tera: Tera::init(Self::files::<Tera>(_templates))?,
            #[cfg(feature = "handlebars")]
            handlebars: Handlebars::init(Self::files::<Handlebars<'static>>(_templates))?,
            #[cfg(feature = "minijinja")]
            minijinja: Environment::init(Self::files::<Environment<'static>>(_templates))?,
        })
    }

    /// Loads the template files that engines don't load in `init()`, which
    /// are Tera's. Templates already registered with Tera shadow files with
    /// the same name; those files' entries are removed from `templates`.
    pub(crate) fn load(&mut self, _templates: &mut HashMap<String, TemplateInfo>)
        -> Result<(), LoadError>
    {
        #[cfg(feature = "tera")]
        {
            tera::remove_shadowed(&self.tera, _templates);
            tera::load(&mut self.tera, Self::files::<Tera>(_templates))
                .map_err(|tera| LoadError { tera })?;
        }

        Ok(())
    }

    #[cfg(feature = "tera")]
    pub(crate) fn disk_renderers(&self, templates: &HashMap<String, TemplateInfo>)
        -> DiskRenderers
    {
        DiskRenderers::new(&self.tera, Self::files::<Tera>(templates).map(|(_, path)| path))
    }

    #[cfg_attr(
        not(any(feature = "tera", feature = "handlebars", feature = "minijinja")),
        allow(unused_variables)
    )]
    pub(crate) fn render<C: Serialize>(
        &self,
        name: &str,
        info: &TemplateInfo,
        context: C,
        #[cfg(feature = "tera")] tera_disk: &DiskRenderers,
    ) -> Option<String> {
        #[cfg(feature = "tera")]
        {
            if info.engine_ext == Tera::EXT {
                let tera = info.path.as_deref()
                    .and_then(|path| tera_disk.for_path(path))
                    .unwrap_or(&self.tera);

                return Engine::render(tera, name, context);
            }
        }

        #[cfg(feature = "handlebars")]
        {
            if info.engine_ext == Handlebars::EXT {
                return Engine::render(&self.handlebars, name, context);
            }
        }

        #[cfg(feature = "minijinja")]
        {
            if info.engine_ext == Environment::EXT {
                return Engine::render(&self.minijinja, name, context);
            }
        }

        None
    }

    /// Returns iterator over template (name, engine_extension).
    pub(crate) fn templates(&self) -> impl Iterator<Item = (&str, &'static str)> {
        #[cfg(feature = "tera")]
        let tera = self.tera.get_template_names().map(|name| (name, Tera::EXT));

        #[cfg(feature = "handlebars")]
        let handlebars = self
            .handlebars
            .get_templates()
            .keys()
            .map(|name| (name.as_str(), Handlebars::EXT));

        #[cfg(feature = "minijinja")]
        let minijinja = self
            .minijinja
            .templates()
            .map(|(name, _)| (name, Environment::EXT));

        #[cfg(not(feature = "tera"))]
        let tera = std::iter::empty();
        #[cfg(not(feature = "handlebars"))]
        let handlebars = std::iter::empty();
        #[cfg(not(feature = "minijinja"))]
        let minijinja = std::iter::empty();

        tera.chain(handlebars).chain(minijinja)
    }
}
