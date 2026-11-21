use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::error::Error;

use tera::{Context, ErrorKind, Tera};
use rkt::serde::Serialize;

use crate::engine::Engine;
use crate::template::TemplateInfo;

/// Removes template files whose names Tera already has: a template registered
/// before files load takes precedence. Removed names get the same template
/// info as other registered templates, so their escaping and `Content-Type`
/// follow the registered template's name instead of the file's.
pub(super) fn remove_shadowed(tera: &Tera, templates: &mut HashMap<String, TemplateInfo>) {
    let registered: HashSet<_> = tera.get_template_names().collect();
    templates.retain(|name, info| {
        let shadowed = info.engine_ext == Tera::EXT && registered.contains(name.as_str());
        if shadowed {
            warn!(
                template = %name,
                path = info.path.as_ref().map(|p| display(p.display())),
                "registered Tera template '{name}' shadows a template file\n\
                 the file is ignored; escaping and Content-Type follow the registered template"
            );
        }

        !shadowed
    });
}

pub(super) fn load<'a>(tera: &mut Tera, templates: impl Iterator<Item = (&'a str, &'a Path)>)
    -> tera::TeraResult<()>
{
    // Load the files as a batch so Tera can resolve inheritance between them.
    let files: Vec<_> = templates.collect();
    if files.iter().all(|(_, path)| path.to_str().is_some()) {
        return tera.add_template_files(files.into_iter().map(|(name, path)| (path, Some(name))));
    }

    // Tera's file loader requires UTF-8 paths even when given a name.
    // Read the files ourselves and still register them in one batch.
    let mut sources = Vec::with_capacity(files.len());
    for (name, path) in files {
        let source = std::fs::read_to_string(path).map_err(|e| {
            let msg = format!("failed to read Tera template '{}'", path.display());
            tera::Error::chain(msg, e)
        })?;

        sources.push((name, source));
    }

    tera.add_raw_templates(sources)
}

/// Whether Tera rejected templates because they use a filter, test, or
/// function that isn't registered. Tera 2.4 reports these only as messages.
pub(super) fn is_unregistered_reference(error: &tera::Error) -> bool {
    const PREFIXES: [&str; 3] = [
        "error: Unknown filter `",
        "error: Unknown test `",
        "error: Unknown function `",
    ];

    matches!(error.kind(), ErrorKind::Msg(msg) if PREFIXES.iter().any(|p| msg.contains(p)))
}

pub(super) fn log_load_error(e: &tera::Error) {
    span_error!("templating", "Tera templating initialization failed" => {
        let mut error = Some(e as &dyn Error);
        while let Some(err) = error {
            error!("{err}");
            error = err.source();
        }

        if is_unregistered_reference(e) {
            error!(
                "help: Tera checks filters, tests, and functions when templates load.\n\
                 Register them in `Template::custom()`, `Template::try_custom()`, or the \
                 `before_load` callback of `Template::try_custom_with_preload()`."
            );
        }
    });
}

const ESCAPED_FILE_SUFFIXES: [&str; 3] = [".html.tera", ".htm.tera", ".xml.tera"];

/// Tera 2.4 only exposes suffix-based escaping. Keep renderers for disk
/// templates with a uniform policy so their original names (including names
/// used in inheritance and includes) can stay intact.
#[derive(Default)]
pub(crate) struct DiskRenderers {
    escaped: Option<Tera>,
    unescaped: Option<Tera>,
}

impl DiskRenderers {
    fn escape(path: &Path) -> bool {
        path.file_name().is_some_and(|name| {
            let bytes = name.as_encoded_bytes();
            ESCAPED_FILE_SUFFIXES.iter().any(|ext| bytes.ends_with(ext.as_bytes()))
        })
    }

    pub(super) fn new<'a>(tera: &Tera, templates: impl Iterator<Item = &'a Path>) -> Self {
        let mut renderers = Self::default();
        for path in templates {
            let escape = Self::escape(path);
            let renderer = if escape {
                &mut renderers.escaped
            } else {
                &mut renderers.unescaped
            };
            renderer.get_or_insert_with(|| {
                let mut tera = tera.clone();
                // The empty string matches every template name.
                tera.autoescape_on(escape.then_some(""));
                tera
            });

            if renderers.escaped.is_some() && renderers.unescaped.is_some() {
                break;
            }
        }

        renderers
    }

    pub(super) fn for_path(&self, path: &Path) -> Option<&Tera> {
        if Self::escape(path) {
            self.escaped.as_ref()
        } else {
            self.unescaped.as_ref()
        }
    }
}

impl Engine for Tera {
    const EXT: &'static str = "tera";

    fn init<'a>(_: impl Iterator<Item = (&'a str, &'a Path)>) -> Option<Self> {
        let mut tera = Tera::default();
        tera.autoescape_on(ESCAPED_FILE_SUFFIXES.into_iter().chain([".html", ".htm", ".xml"]));
        Some(tera)
    }

    fn render<C: Serialize>(&self, template: &str, context: C) -> Option<String> {
        if !self.contains_template(template) {
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
