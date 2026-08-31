use std::error::Error;
use std::path::Path;

use rkt::serde::Serialize;

// Enabling both `tera1` and `tera` is not an error, because Cargo features
// have to stay additive. If both are enabled `tera`` has priority.
#[cfg(feature = "tera")]
pub(crate) use ::tera::{Context, Tera};

#[cfg(all(feature = "tera1", not(feature = "tera")))]
pub(crate) use ::tera1::{Context, Tera};

use crate::engine::Engine;

/// Builds the suffixes Tera uses to decide whether to escape a template.
///
/// Each file type has two forms:
///
///   * `.html.tera` for templates loaded from files.
///   * `.html` for templates added directly in code.
///
/// Generating both from one list keeps their escaping rules in sync.
macro_rules! autoescape_suffixes {
    ($engine_ext:literal; $($data_type:literal),+ $(,)?) => {
        [$(concat!(".", $data_type, ".", $engine_ext),)+ $(concat!(".", $data_type),)+]
    };
}

/// File extensions that Tera HTML-escapes.
///
/// Rocket removes file extensions from registered template names. Tera 1 can
/// also check the original file path, while Tera 2 needs the matching template
/// names added separately.
const AUTOESCAPE_SUFFIXES: &[&str] = &autoescape_suffixes!("tera"; "html", "htm", "xml");

const _: () = {
    // The engine extension above is a literal because `concat!` needs one.
    assert!(matches!(<Tera as Engine>::EXT.as_bytes(), b"tera"));
};

/// Tera 1.x: the static list is enough. A discovered template is matched by
/// its source path, a raw template by its registered name.
#[cfg(all(feature = "tera1", not(feature = "tera")))]
fn autoescape_suffixes(_files: &[(&Path, Option<&str>)]) -> Vec<&'static str> {
    AUTOESCAPE_SUFFIXES.to_vec()
}

/// Adds the names of file templates that Tera 2 should escape.
///
/// Tera 2 checks template names, but Rocket's registered names have no file
/// extension. Use each template's file path to decide whether to add its name.
#[cfg(feature = "tera")]
fn autoescape_suffixes(files: &[(&Path, Option<&str>)]) -> Vec<std::borrow::Cow<'static, str>> {
    use std::borrow::Cow;

    let mut suffixes: Vec<Cow<'static, str>> = AUTOESCAPE_SUFFIXES
        .iter()
        .copied()
        .map(Cow::Borrowed)
        .collect();

    suffixes.extend(files.iter().filter_map(|(path, name)| {
        let path = path.to_str()?;
        let name = (*name)?;
        AUTOESCAPE_SUFFIXES
            .iter()
            .any(|s| path.ends_with(s))
            .then(|| Cow::Owned(name.to_owned()))
    }));

    suffixes
}

impl Engine for Tera {
    const EXT: &'static str = "tera";

    fn init<'a>(templates: impl Iterator<Item = (&'a str, &'a Path)>) -> Option<Self> {
        // Collect into a tuple of (path, name) for Tera. If we register one at
        // a time, it will complain about unregistered base templates.
        let files = templates
            .map(|(name, path)| (path, Some(name)))
            .collect::<Vec<_>>();

        // Create the Tera instance.
        let mut tera = Tera::default();
        tera.autoescape_on(autoescape_suffixes(&files));

        // Finally try to tell Tera about all of the templates.
        if let Err(e) = tera.add_template_files(files) {
            span_error!("templating", "Tera templating initialization failed" => {
                let mut error = Some(&e as &dyn Error);
                while let Some(err) = error {
                    error!("{err}");
                    error = err.source();
                }
            });

            None
        } else {
            Some(tera)
        }
    }

    fn render<C: Serialize>(&self, template: &str, context: C) -> Option<String> {
        #[cfg(all(feature = "tera1", not(feature = "tera")))]
        let exists = self.get_template(template).is_ok();
        #[cfg(feature = "tera")]
        let exists = self.contains_template(template);

        if !exists {
            error!(template, "requested template does not exist");
            return None;
        };

        let tera_ctx = Context::from_serialize(&context)
            .map_err(|e| error!("Tera context error: {}.", e))
            .ok()?;

        match Tera::render(self, template, &tera_ctx) {
            Ok(string) => Some(string),
            Err(e) => {
                span_error!("templating", template, "failed to render Tera template" => {
                    let mut error = Some(&e as &dyn Error);
                    while let Some(err) = error {
                        error!("{err}");
                        error = err.source();
                    }
                });

                None
            }
        }
    }
}
