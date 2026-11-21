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

[Tera]: https://docs.rs/crate/tera/2
[Handlebars]: https://docs.rs/crate/handlebars/6
[MiniJinja]: https://docs.rs/minijinja/2

# Usage

  1. Enable the `rkt_dyn_templates` feature corresponding to your templating
     engine(s) of choice:

     ```toml
     [dependencies.rkt_dyn_templates]
     version = "1.3.0"
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

## Upgrading to Tera 2

The `tera` feature uses Tera 2. Replace template macros with components and
update custom filters, tests, and functions to the new API. The
[Tera migration guide](https://github.com/Keats/tera/blob/master/MIGRATION.md)
describes the syntax and API changes.

Tera 2 checks that the filters, tests, and functions a template uses are
registered when the template loads, not when it renders. Ones registered in
`Template::custom()` or `Template::try_custom()` keep working in template
files. A callback that both registers them and adds Tera templates that extend
or include template files must be split with
`Template::try_custom_with_preload()`: register filters, tests, and functions
in `before_load`, and add the templates in `after_load`.
