#[macro_use] extern crate rkt;

#[cfg(test)]
mod tests;
mod task;

use rkt::{Rocket, Build, State};
use rkt::fairing::AdHoc;
use rkt::request::FlashMessage;
use rkt::response::{Flash, Redirect};
use rkt::serde::Serialize;
use rkt::form::Form;
use rkt::fs::{FileServer, relative};

use rkt_dyn_templates::Template;
use sqlx::SqlitePool;

use crate::task::{Task, Todo};

#[derive(Debug, Serialize)]
#[serde(crate = "rkt::serde")]
struct Context {
    flash: Option<(String, String)>,
    tasks: Vec<Task>,
}

impl Context {
    pub async fn err<M: std::fmt::Display>(pool: &SqlitePool, msg: M) -> Context {
        Context {
            flash: Some(("error".into(), msg.to_string())),
            tasks: Task::all(pool).await.unwrap_or_default(),
        }
    }

    pub async fn raw(pool: &SqlitePool, flash: Option<(String, String)>) -> Context {
        match Task::all(pool).await {
            Ok(tasks) => Context { flash, tasks },
            Err(e) => {
                error!("DB Task::all() error: {e}");
                Context {
                    flash: Some(("error".into(), "Fail to access database.".into())),
                    tasks: vec![],
                }
            }
        }
    }
}

#[post("/", data = "<todo_form>")]
async fn new(todo_form: Form<Todo>, pool: &State<SqlitePool>) -> Flash<Redirect> {
    let todo = todo_form.into_inner();
    if todo.description.is_empty() {
        Flash::error(Redirect::to("/"), "Description cannot be empty.")
    } else if let Err(e) = Task::insert(todo, pool).await {
        error!("DB insertion error: {e}");
        Flash::error(Redirect::to("/"), "Todo could not be inserted due an internal error.")
    } else {
        Flash::success(Redirect::to("/"), "Todo successfully added.")
    }
}

#[put("/<id>")]
async fn toggle(id: i64, pool: &State<SqlitePool>) -> Result<Redirect, Template> {
    match Task::toggle_with_id(id, pool).await {
        Ok(_) => Ok(Redirect::to("/")),
        Err(e) => {
            error!("DB toggle({id}) error: {e}");
            Err(Template::render("index", Context::err(pool, "Failed to toggle task.").await))
        }
    }
}

#[delete("/<id>")]
async fn delete(id: i64, pool: &State<SqlitePool>) -> Result<Flash<Redirect>, Template> {
    match Task::delete_with_id(id, pool).await {
        Ok(_) => Ok(Flash::success(Redirect::to("/"), "Todo was deleted.")),
        Err(e) => {
            error!("DB deletion({id}) error: {e}");
            Err(Template::render("index", Context::err(pool, "Failed to delete task.").await))
        }
    }
}

#[get("/")]
async fn index(flash: Option<FlashMessage<'_>>, pool: &State<SqlitePool>) -> Template {
    let flash = flash.map(FlashMessage::into_inner);
    Template::render("index", Context::raw(pool, flash).await)
}

async fn init_db(rocket: Rocket<Build>) -> rkt::fairing::Result {
    use sqlx::sqlite::SqliteConnectOptions;
    use std::str::FromStr;

    let url = match rocket.figment().extract_inner::<String>("databases.sqlite_database.url") {
        Ok(url) => url,
        Err(e) => {
            error!("database URL not configured: {e}");
            return Err(rocket);
        }
    };

    let opts = match SqliteConnectOptions::from_str(&url) {
        Ok(opts) => opts.create_if_missing(true),
        Err(e) => {
            error!("invalid database URL: {e}");
            return Err(rocket);
        }
    };

    match SqlitePool::connect_with(opts).await {
        Ok(pool) => {
            if let Err(e) = sqlx::migrate!().run(&pool).await {
                error!("migrations failed: {e}");
                return Err(rocket);
            }
            Ok(rocket.manage(pool))
        }
        Err(e) => {
            error!("failed to connect to database: {e}");
            Err(rocket)
        }
    }
}

#[launch]
fn rocket() -> _ {
    rkt::build()
        .attach(AdHoc::try_on_ignite("SQLite Database", init_db))
        .attach(Template::fairing())
        .mount("/", FileServer::new(relative!("static")))
        .mount("/", routes![index])
        .mount("/todo", routes![new, toggle, delete])
}
