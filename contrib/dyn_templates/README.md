# `dyn_templates` [![ci.svg]][ci] [![crates.io]][crate] [![docs.svg]][crate docs]

[crates.io]: https://img.shields.io/crates/v/rkt_dyn_templates.svg
[crate]: https://crates.io/crates/rkt_dyn_templates
[docs.svg]: https://img.shields.io/badge/web-master-red.svg?style=flat&label=docs&colorB=d33847
[crate docs]: https://docs.rs/rkt_dyn_templates/latest/rkt_dyn_templates/
[ci.svg]: https://github.com/rustfoo/rkt/workflows/CI/badge.svg
[ci]: https://github.com/rustfoo/rkt/actions

This crate adds support for dynamic template rendering to rkt. It
automatically discovers templates, provides a `Responder` to render templates,
and automatically reloads templates when compiled in debug mode. It supports [Handlebars], [Tera] and [MiniJinja].

[Tera]: https://docs.rs/crate/tera/1
[Handlebars]: https://docs.rs/crate/handlebars/6
[MiniJinja]: https://docs.rs/minijinja/2

# Usage

  1. Enable the `rkt_dyn_templates` feature corresponding to your templating
     engine(s) of choice:

     ```toml
     [dependencies.rkt_dyn_templates]
     version = "1.0.1"
     features = ["handlebars", "tera", "minijinja"]
     ```

  1. Write your template files in Handlebars (`.hbs`), Tera (`.tera`) and/or
     MiniJinja (`.j2`) in the configurable `template_dir` directory (default:
     `{rkt_root}/templates`).

  2. Attach `Template::fairing()` and return a `Template` using
     `Template::render()`, supplying the name of the template file **minus the
     last two extensions**:

     ```rust
     use rkt_dyn_templates::{Template, context};

     #[get("/")]
     fn index() -> Template {
         Template::render("template-name", context! { field: "value" })
     }

     #[launch]
     fn rocket() -> _ {
         rkt::build().attach(Template::fairing())
     }
     ```

See the [crate docs] for full details.
