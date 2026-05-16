use rkt::{Rocket, Build, State};
use rkt::fairing::AdHoc;
use rkt::response::{Debug, status::Created};
use rkt::serde::{Serialize, Deserialize, json::Json};
use rkt::tokio::task::spawn_blocking;

use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};

type DbPool = Pool<ConnectionManager<SqliteConnection>>;
type Result<T, E = Debug<diesel::result::Error>> = std::result::Result<T, E>;

#[derive(Debug, Clone, Deserialize, Serialize, Queryable, Insertable)]
#[serde(crate = "rkt::serde")]
#[diesel(table_name = posts)]
struct Post {
    #[serde(skip_deserializing)]
    id: Option<i32>,
    title: String,
    text: String,
}

table! {
    posts (id) {
        id -> Nullable<Integer>,
        title -> Text,
        text -> Text,
    }
}

#[post("/", data = "<post>")]
async fn create(pool: &State<DbPool>, mut post: Json<Post>) -> Result<Created<Json<Post>>> {
    let pool = pool.inner().clone();
    let post_value = post.clone();
    let id: Option<i32> = spawn_blocking(move || {
        let mut conn = pool.get().expect("pool available");
        diesel::insert_into(posts::table)
            .values(&*post_value)
            .returning(posts::id)
            .get_result(&mut conn)
    }).await.unwrap()?;

    post.id = Some(id.expect("returning guarantees id present"));
    Ok(Created::new("/").body(post))
}

#[get("/")]
async fn list(pool: &State<DbPool>) -> Result<Json<Vec<i64>>> {
    let pool = pool.inner().clone();
    let ids: Vec<Option<i32>> = spawn_blocking(move || {
        let mut conn = pool.get().expect("pool available");
        posts::table.select(posts::id).load(&mut conn)
    }).await.unwrap()?;

    Ok(Json(ids.into_iter().flatten().map(i64::from).collect()))
}

#[get("/<id>")]
async fn read(pool: &State<DbPool>, id: i32) -> Option<Json<Post>> {
    let pool = pool.inner().clone();
    spawn_blocking(move || {
        let mut conn = pool.get().expect("pool available");
        posts::table.filter(posts::id.eq(id)).first(&mut conn)
    }).await.unwrap().map(Json).ok()
}

#[delete("/<id>")]
async fn delete(pool: &State<DbPool>, id: i32) -> Result<Option<()>> {
    let pool = pool.inner().clone();
    let affected = spawn_blocking(move || {
        let mut conn = pool.get().expect("pool available");
        diesel::delete(posts::table).filter(posts::id.eq(id)).execute(&mut conn)
    }).await.unwrap()?;

    Ok((affected == 1).then_some(()))
}

#[delete("/")]
async fn destroy(pool: &State<DbPool>) -> Result<()> {
    let pool = pool.inner().clone();
    spawn_blocking(move || {
        let mut conn = pool.get().expect("pool available");
        diesel::delete(posts::table).execute(&mut conn)
    }).await.unwrap()?;

    Ok(())
}

async fn init_db(rocket: Rocket<Build>) -> rkt::fairing::Result {
    use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
    const MIGRATIONS: EmbeddedMigrations = embed_migrations!("db/diesel/migrations");

    let url = match rocket.figment().extract_inner::<String>("databases.diesel.url") {
        Ok(url) => url,
        Err(e) => {
            error!("diesel database URL not configured: {e}");
            return Err(rocket);
        }
    };

    let manager = ConnectionManager::<SqliteConnection>::new(url);
    let pool = match Pool::builder().build(manager) {
        Ok(pool) => pool,
        Err(e) => {
            error!("failed to build diesel pool: {e}");
            return Err(rocket);
        }
    };

    let pool_clone = pool.clone();
    if let Err(e) = spawn_blocking(move || {
        let mut conn = pool_clone.get().expect("pool available");
        conn.run_pending_migrations(MIGRATIONS).map(|_| ())
    }).await.unwrap() {
        error!("diesel migrations failed: {e}");
        return Err(rocket);
    }

    Ok(rocket.manage(pool))
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("Diesel SQLite Stage", |rocket| async {
        rocket.attach(AdHoc::try_on_ignite("Diesel SQLite Database", init_db))
            .mount("/diesel", routes![list, read, create, delete, destroy])
    })
}
