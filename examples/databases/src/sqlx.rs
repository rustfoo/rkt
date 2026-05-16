use rkt::{Rocket, Build, State, futures};
use rkt::fairing::AdHoc;
use rkt::response::status::Created;
use rkt::serde::{Serialize, Deserialize, json::Json};

use futures::{stream::TryStreamExt, future::TryFutureExt};
use sqlx::SqlitePool;

type Result<T, E = rkt::response::Debug<sqlx::Error>> = std::result::Result<T, E>;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(crate = "rkt::serde")]
struct Post {
    #[serde(skip_deserializing, skip_serializing_if = "Option::is_none")]
    id: Option<i64>,
    title: String,
    text: String,
}

#[post("/", data = "<post>")]
async fn create(pool: &State<SqlitePool>, mut post: Json<Post>) -> Result<Created<Json<Post>>> {
    // NOTE: sqlx#2543, sqlx#1648 mean we can't use the pithier `fetch_one()`.
    let results = sqlx::query!(
            "INSERT INTO posts (title, text) VALUES (?, ?) RETURNING id",
            post.title, post.text
        )
        .fetch(pool.inner())
        .try_collect::<Vec<_>>()
        .await?;

    post.id = Some(results.first().expect("returning results").id);
    Ok(Created::new("/").body(post))
}

#[get("/")]
async fn list(pool: &State<SqlitePool>) -> Result<Json<Vec<i64>>> {
    let ids = sqlx::query!("SELECT id FROM posts")
        .fetch(pool.inner())
        .map_ok(|record| record.id)
        .try_collect::<Vec<_>>()
        .await?;

    Ok(Json(ids))
}

#[get("/<id>")]
async fn read(pool: &State<SqlitePool>, id: i64) -> Option<Json<Post>> {
    sqlx::query!("SELECT id, title, text FROM posts WHERE id = ?", id)
        .fetch_one(pool.inner())
        .map_ok(|r| Json(Post { id: Some(r.id), title: r.title, text: r.text }))
        .await
        .ok()
}

#[delete("/<id>")]
async fn delete(pool: &State<SqlitePool>, id: i64) -> Result<Option<()>> {
    let result = sqlx::query!("DELETE FROM posts WHERE id = ?", id)
        .execute(pool.inner())
        .await?;

    Ok((result.rows_affected() == 1).then_some(()))
}

#[delete("/")]
async fn destroy(pool: &State<SqlitePool>) -> Result<()> {
    sqlx::query!("DELETE FROM posts").execute(pool.inner()).await?;
    Ok(())
}

async fn init_db(rocket: Rocket<Build>) -> rkt::fairing::Result {
    use sqlx::sqlite::SqliteConnectOptions;
    use std::str::FromStr;

    let url = match rocket.figment().extract_inner::<String>("databases.sqlx.url") {
        Ok(url) => url,
        Err(e) => {
            error!("sqlx database URL not configured: {e}");
            return Err(rocket);
        }
    };

    let opts = match SqliteConnectOptions::from_str(&url) {
        Ok(opts) => opts.create_if_missing(true),
        Err(e) => {
            error!("invalid sqlx database URL: {e}");
            return Err(rocket);
        }
    };

    match SqlitePool::connect_with(opts).await {
        Ok(pool) => {
            if let Err(e) = sqlx::migrate!("db/sqlx/migrations").run(&pool).await {
                error!("sqlx migrations failed: {e}");
                return Err(rocket);
            }
            Ok(rocket.manage(pool))
        }
        Err(e) => {
            error!("failed to connect to sqlx database: {e}");
            Err(rocket)
        }
    }
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("SQLx Stage", |rocket| async {
        rocket.attach(AdHoc::try_on_ignite("SQLx Database", init_db))
            .mount("/sqlx", routes![list, create, read, delete, destroy])
    })
}
