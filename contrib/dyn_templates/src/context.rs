use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use crate::engine::Engines;
use crate::template::TemplateInfo;

use normpath::PathExt;
use rkt::http::ContentType;

pub(crate) type Callback =
    Box<dyn Fn(&mut Engines) -> Result<(), Box<dyn Error>> + Send + Sync + 'static>;

/// The customization callbacks of the templating fairing.
pub(crate) enum Callbacks {
    /// From `Template::custom()` and `Template::try_custom()`. The callback
    /// runs after Tera loads template files, like it did with Tera 1. Tera 2
    /// rejects files that use filters, tests, or functions that aren't
    /// registered, so if that's why loading fails, the callback runs first.
    Legacy(Callback),
    /// From `Template::try_custom_with_preload()`.
    Staged {
        before_load: Callback,
        after_load: Callback,
    },
}

pub(crate) struct Context {
    /// The root of the template directory.
    pub root: PathBuf,
    /// Mapping from template name to its information.
    pub templates: HashMap<String, TemplateInfo>,
    /// Loaded template engines
    pub engines: Engines,
    /// Tera renderers configured for the content extensions of disk templates.
    #[cfg(feature = "tera")]
    pub tera_disk: crate::engine::DiskRenderers,
}

pub(crate) use self::manager::ContextManager;

impl Context {
    /// Load all of the templates at `root`, initialize them using the relevant
    /// template engine, and store all of the initialized state in a `Context`
    /// structure, which is returned if all goes well.
    pub fn initialize(root: &Path, callbacks: &Callbacks) -> Option<Context> {
        fn is_file_with_ext(entry: &walkdir::DirEntry, ext: &str) -> bool {
            let is_file = entry.file_type().is_file();
            let has_ext = entry.path().extension().is_some_and(|e| e == ext);
            is_file && has_ext
        }

        let root = match root.normalize() {
            Ok(root) => root.into_path_buf(),
            Err(e) => {
                error!("Invalid template directory '{}': {}.", root.display(), e);
                return None;
            }
        };

        let mut templates: HashMap<String, TemplateInfo> = HashMap::new();
        for &ext in Engines::ENABLED_EXTENSIONS {
            for entry in walkdir::WalkDir::new(&root).follow_links(true) {
                let entry = match entry {
                    Ok(entry) if is_file_with_ext(&entry, ext) => entry,
                    Ok(_) | Err(_) => continue,
                };

                let (template, data_type_str) = split_path(&root, entry.path());
                if let Some(info) = templates.get(&*template) {
                    warn!(
                        %template,
                        first_path = %entry.path().display(),
                        second_path = info.path.as_ref().map(|p| display(p.display())),
                        data_type = %info.data_type,
                        "Template name '{template}' can refer to multiple templates.\n\
                         First path will be used. Second path is ignored."
                    );

                    continue;
                }

                let data_type = data_type_str
                    .as_ref()
                    .and_then(|ext| ContentType::from_extension(ext))
                    .unwrap_or(ContentType::Text);

                templates.insert(
                    template,
                    TemplateInfo {
                        path: Some(entry.into_path()),
                        engine_ext: ext,
                        data_type,
                    },
                );
            }
        }

        let mut engines = Engines::init(&templates)?;
        match callbacks {
            Callbacks::Staged { before_load, after_load } => {
                if let Err(reason) = before_load(&mut engines) {
                    error!(%reason, "pre-load template customization callback failed");
                    return None;
                }

                engines.load(&mut templates).map_err(|e| e.log()).ok()?;
                if let Err(reason) = after_load(&mut engines) {
                    error!(%reason, "post-load template customization callback failed");
                    return None;
                }
            }
            Callbacks::Legacy(callback) => match engines.load(&mut templates) {
                Ok(()) => {
                    if let Err(reason) = callback(&mut engines) {
                        error!(%reason, "template customization callback failed");
                        return None;
                    }
                }
                Err(e) if e.is_unregistered_reference() => {
                    // A failed load leaves the engines unchanged, so the
                    // callback still runs only once.
                    debug!("Tera templates use unregistered filters, tests, or functions\n\
                        running the customization callback before loading Tera templates");

                    if let Err(reason) = callback(&mut engines) {
                        error!(%reason, "template customization callback failed");
                        error!("note: the callback ran before Tera loaded template files \
                            because they use filters, tests, or functions that weren't \
                            registered yet.\n\
                            To register those and also add Tera templates that use template \
                            files, use `Template::try_custom_with_preload()`.");
                        return None;
                    }

                    engines.load(&mut templates).map_err(|e| e.log()).ok()?;
                }
                Err(e) => {
                    e.log();
                    return None;
                }
            },
        }

        #[cfg(feature = "tera")]
        let tera_disk = engines.disk_renderers(&templates);

        for (name, engine_ext) in engines.templates() {
            if !templates.contains_key(name) {
                let data_type = Path::new(name)
                    .extension()
                    .and_then(|osstr| osstr.to_str())
                    .and_then(ContentType::from_extension)
                    .unwrap_or(ContentType::Text);

                let info = TemplateInfo {
                    path: None,
                    engine_ext,
                    data_type,
                };
                templates.insert(name.to_string(), info);
            }
        }

        Some(Context {
            root,
            templates,
            engines,
            #[cfg(feature = "tera")]
            tera_disk,
        })
    }
}

#[cfg(not(debug_assertions))]
mod manager {
    use super::Context;
    use std::ops::Deref;

    /// Wraps a Context. With `cfg(debug_assertions)` active, this structure
    /// additionally provides a method to reload the context at runtime.
    pub(crate) struct ContextManager(Context);

    impl ContextManager {
        pub fn new(ctxt: Context) -> ContextManager {
            ContextManager(ctxt)
        }

        pub fn context<'a>(&'a self) -> impl Deref<Target = Context> + 'a {
            &self.0
        }

        pub fn is_reloading(&self) -> bool {
            false
        }
    }
}

#[cfg(debug_assertions)]
mod manager {
    use std::ops::{Deref, DerefMut};
    use std::sync::mpsc::{channel, Receiver};
    use std::sync::{Mutex, RwLock};

    use notify::{recommended_watcher, Error, Event, RecommendedWatcher, RecursiveMode, Watcher};

    use super::{Callbacks, Context};

    /// Wraps a Context. With `cfg(debug_assertions)` active, this structure
    /// additionally provides a method to reload the context at runtime.
    pub(crate) struct ContextManager {
        /// The current template context, inside an RwLock so it can be updated.
        context: RwLock<Context>,
        /// A filesystem watcher and the receive queue for its events.
        watcher: Option<(RecommendedWatcher, Mutex<Receiver<Result<Event, Error>>>)>,
    }

    impl ContextManager {
        pub fn new(ctxt: Context) -> ContextManager {
            let (tx, rx) = channel();
            let watcher = recommended_watcher(tx).and_then(|mut watcher| {
                watcher.watch(&ctxt.root.canonicalize()?, RecursiveMode::Recursive)?;
                Ok(watcher)
            });

            let watcher = match watcher {
                Ok(watcher) => Some((watcher, Mutex::new(rx))),
                Err(e) => {
                    warn!(
                        "live template reloading initialization failed: {e}\n\
                        live template reloading is unavailable"
                    );
                    None
                }
            };

            ContextManager {
                watcher,
                context: RwLock::new(ctxt),
            }
        }

        pub fn context(&self) -> impl Deref<Target = Context> + '_ {
            self.context.read().unwrap()
        }

        pub fn is_reloading(&self) -> bool {
            self.watcher.is_some()
        }

        fn context_mut(&self) -> impl DerefMut<Target = Context> + '_ {
            self.context.write().unwrap()
        }

        /// Checks whether any template files have changed on disk. If there
        /// have been changes since the last reload, all templates are
        /// reinitialized from disk and the user's customization callbacks are run
        /// again.
        pub fn reload_if_needed(&self, callbacks: &Callbacks) {
            let templates_changes = self
                .watcher
                .as_ref()
                .map(|(_, rx)| rx.lock().expect("fsevents lock").try_iter().count() > 0);

            if let Some(true) = templates_changes {
                debug!("template change detected: reloading templates");
                let root = self.context().root.clone();
                if let Some(new_ctxt) = Context::initialize(&root, callbacks) {
                    *self.context_mut() = new_ctxt;
                } else {
                    warn!(
                        "error while reloading template\n\
                        existing templates will remain active."
                    )
                };
            }
        }
    }
}

/// Removes the file path's extension or does nothing if there is none.
fn remove_extension(path: &Path) -> PathBuf {
    let stem = match path.file_stem() {
        Some(stem) => stem,
        None => return path.to_path_buf(),
    };

    match path.parent() {
        Some(parent) => parent.join(stem),
        None => PathBuf::from(stem),
    }
}

/// Splits a path into a name that may be used to identify the template, and the
/// template's data type, if any.
fn split_path(root: &Path, path: &Path) -> (String, Option<String>) {
    let rel_path = path.strip_prefix(root).unwrap().to_path_buf();
    let path_no_ext = remove_extension(&rel_path);
    let data_type = path_no_ext.extension();
    let mut name = remove_extension(&path_no_ext)
        .to_string_lossy()
        .into_owned();

    // Ensure template name consistency on Windows systems
    if cfg!(windows) {
        name = name.replace('\\', "/");
    }

    (name, data_type.map(|d| d.to_string_lossy().into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_path_index_html() {
        for root in &["/", "/a/b/c/", "/a/b/c/d/", "/a/"] {
            for filename in &["index.html.hbs", "index.html.tera"] {
                let path = Path::new(root).join(filename);
                let (name, data_type) = split_path(Path::new(root), &path);

                assert_eq!(name, "index");
                assert_eq!(data_type, Some("html".into()));
            }
        }
    }

    #[test]
    fn template_path_subdir_index_html() {
        for root in &["/", "/a/b/c/", "/a/b/c/d/", "/a/"] {
            for sub in &["a/", "a/b/", "a/b/c/", "a/b/c/d/"] {
                for filename in &["index.html.hbs", "index.html.tera"] {
                    let path = Path::new(root).join(sub).join(filename);
                    let (name, data_type) = split_path(Path::new(root), &path);

                    let expected_name = format!("{}index", sub);
                    assert_eq!(name, expected_name.as_str());
                    assert_eq!(data_type, Some("html".into()));
                }
            }
        }
    }

    #[test]
    fn template_path_doc_examples() {
        fn name_for(path: &str) -> String {
            split_path(Path::new("templates/"), &Path::new("templates/").join(path)).0
        }

        assert_eq!(name_for("index.html.hbs"), "index");
        assert_eq!(name_for("index.tera"), "index");
        assert_eq!(name_for("index.hbs"), "index");
        assert_eq!(name_for("dir/index.hbs"), "dir/index");
        assert_eq!(name_for("dir/index.html.tera"), "dir/index");
        assert_eq!(name_for("index.template.html.hbs"), "index.template");
        assert_eq!(
            name_for("subdir/index.template.html.hbs"),
            "subdir/index.template"
        );
    }
}
