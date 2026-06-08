---
sidebar_position: 9
sidebar_label: "State"
---

# State

Many web applications have a need to maintain state. This can be as simple as
maintaining a counter for the number of visits or as complex as needing to
access job queues and multiple databases. Rocket provides the tools to enable
these kinds of interactions in a safe and simple manner.

## Managed State

The enabling feature for maintaining state is _managed state_. Managed state, as
the name implies, is state that Rocket manages for your application. The state
is managed on a per-type basis: Rocket will manage at most one value of a given
type.

The process for using managed state is simple:

  1. Call `manage` on the `Rocket` instance corresponding to your application
     with the initial value of the state.
  2. Add a `&State<T>` type to any request handler, where `T` is the type of the
     value passed into `manage`.

:::note[All managed state must be thread-safe.]

Because Rocket automatically parallelizes your application, handlers can
concurrently access managed state. As a result, managed state must be
thread-safe. Thanks to Rust, this condition is checked at compile-time by
ensuring that the type of values you store in managed state implement `Send` +
`Sync`.
:::


### Adding State

To instruct Rocket to manage state for your application, call the
[`manage`](https://docs.rs/rkt/latest/rkt/struct.Rocket.html#method.manage) method
on an instance of `Rocket`. For example, to ask Rocket to manage a `HitCount`
structure with an internal `AtomicUsize` with an initial value of `0`, we can
write the following:

```rust
use std::sync::atomic::AtomicUsize;

struct HitCount {
    count: AtomicUsize
}

rkt::build().manage(HitCount { count: AtomicUsize::new(0) });
```

The `manage` method can be called any number of times as long as each call
refers to a value of a different type. For instance, to have Rocket manage both
a `HitCount` value and a `Config` value, we can write:

```rust
# use std::sync::atomic::AtomicUsize;
# struct HitCount { count: AtomicUsize }
# type Config = &'static str;
# let user_input = "input";

rkt::build()
    .manage(HitCount { count: AtomicUsize::new(0) })
    .manage(Config::from(user_input));
```

### Retrieving State

State that is being managed by Rocket can be retrieved via the
[`&State`](https://docs.rs/rkt/latest/rkt/struct.State.html) type: a [request
guard](./requests/#request-guards) for managed state. To use the request guard,
add a `&State<T>` type to any request handler, where `T` is the type of the
managed state. For example, we can retrieve and respond with the current
`HitCount` in a `count` route as follows:

```rust
# #[macro_use] extern crate rkt;
# fn main() {}

# use std::sync::atomic::{AtomicUsize, Ordering};
# struct HitCount { count: AtomicUsize }

use rkt::State;

#[get("/count")]
fn count(hit_count: &State<HitCount>) -> String {
    let current_count = hit_count.count.load(Ordering::Relaxed);
    format!("Number of visits: {}", current_count)
}
```

You can retrieve more than one `&State` type in a single route as well:

```rust
# #[macro_use] extern crate rkt;
# fn main() {}

# struct HitCount;
# struct Config;
# use rkt::State;

#[get("/state")]
fn state(hit_count: &State<HitCount>, config: &State<Config>) { /* .. */ }
```

:::warning

If you request a `&State<T>` for a `T` that is not `managed`, Rocket will
refuse to start your application. This prevents what would have been an
unmanaged state runtime error. Unmanaged state is detected at runtime through
[_sentinels_](https://docs.rs/rkt/latest/rkt/trait.Sentinel.html), so there are limitations. If a
limitation is hit, Rocket still won't call the offending route. Instead,
Rocket will log an error message and return a **500** error to the client.
:::

You can find a complete example using the `HitCount` structure in the [state
example on GitHub](https://github.com/rustfoo/rkt/tree/main/examples/state) and learn more about the [`manage`
method](https://docs.rs/rkt/latest/rkt/struct.Rocket.html#method.manage) and [`State`
type](https://docs.rs/rkt/latest/rkt/struct.State.html) in the API docs.

### Within Guards

Because `State` is itself a request guard, managed state can be retrieved from
another request guard's implementation using either [`Request::guard()`] or
[`Rocket::state()`]. In the following code example, the `Item` request guard
retrieves `MyConfig` from managed state using both methods:

```rust
use rkt::State;
use rkt::request::{self, Request, FromRequest};
use rkt::outcome::IntoOutcome;
use rkt::http::Status;

# struct MyConfig { user_val: String };
struct Item<'r>(&'r str);

#[rkt::async_trait]
impl<'r> FromRequest<'r> for Item<'r> {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> request::Outcome<Self, ()> {
        // Using `State` as a request guard. Use `inner()` to get an `'r`.
        let outcome = request.guard::<&State<MyConfig>>().await
            .map(|my_config| Item(&my_config.user_val));

        // Or alternatively, using `Rocket::state()`:
        let outcome = request.rocket().state::<MyConfig>()
            .map(|my_config| Item(&my_config.user_val))
            .or_forward(Status::InternalServerError);

        outcome
    }
}
```

[`Request::guard()`]: https://docs.rs/rkt/latest/rkt/struct.Request.html#method.guard
[`Rocket::state()`]: https://docs.rs/rkt/latest/rkt/struct.Rocket.html#method.state

## Request-Local State

While managed state is *global* and available application-wide, request-local
state is *local* to a given request, carried along with the request, and dropped
once the request is completed. Request-local state can be used whenever a
`Request` is available, such as in a fairing, a request guard, or a responder.

Request-local state is *cached*: if data of a given type has already been
stored, it will be reused. This is especially useful for request guards that
might be invoked multiple times during routing and processing of a single
request, such as those that deal with authentication.

As an example, consider the following request guard implementation for
`RequestId` that uses request-local state to generate and expose a unique
integer ID per request:

```rust
# #[macro_use] extern crate rkt;
# fn main() {}
# use std::sync::atomic::{AtomicUsize, Ordering};

use rkt::request::{self, Request, FromRequest};

/// A global atomic counter for generating IDs.
static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A type that represents a request's ID.
struct RequestId(pub usize);

/// Returns the current request's ID, assigning one only as necessary.
#[rkt::async_trait]
impl<'r> FromRequest<'r> for &'r RequestId {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        // The closure passed to `local_cache` will be executed at most once per
        // request: the first time the `RequestId` guard is used. If it is
        // requested again, `local_cache` will return the same value.
        request::Outcome::Success(request.local_cache(|| {
            RequestId(ID_COUNTER.fetch_add(1, Ordering::Relaxed))
        }))
    }
}

#[get("/")]
fn id(id: &RequestId) -> String {
    format!("This is request #{}.", id.0)
}
```

Note that, without request-local state, it would not be possible to:

  1. Associate a piece of data, here an ID, directly with a request.
  2. Ensure that a value is generated at most once per request.

For more examples, see the [`FromRequest` request-local state] documentation,
which uses request-local state to cache expensive authentication and
authorization computations, and the [`Fairing`] documentation, which uses
request-local state to implement request timing.

[`FromRequest` request-local state]: https://docs.rs/rkt/latest/rkt/request/trait.FromRequest.html#request-local-state
[`Fairing`]: https://docs.rs/rkt/latest/rkt/fairing/trait.Fairing.html#request-local-state

## Databases

Rocket does not include a built-in database pooling abstraction. Instead, use
your preferred database crate's own connection pool directly, manage it via
[`manage()`], and retrieve it in handlers with [`&State<Pool>`].

The example below uses [`sqlx`] with a SQLite database, but the same pattern
applies to any pool — `deadpool-postgres`, `bb8`, or `r2d2` for synchronous
ORMs via [`spawn_blocking`].

### Async databases (sqlx)

Add `sqlx` to `Cargo.toml`:

```toml
[dependencies.sqlx]
version = "0.8"
default-features = false
features = ["sqlite", "macros", "migrate", "runtime-tokio"]
```

Connect the pool via an [`AdHoc`] fairing during launch, run any pending
migrations, then manage the pool:

```rust
#[macro_use] extern crate rkt;

use rkt::{Rocket, Build, State};
use rkt::fairing::AdHoc;
use rkt::trace::error;

use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use std::str::FromStr;

async fn init_db(rocket: Rocket<Build>) -> rkt::fairing::Result {
    let url = match rocket.figment().extract_inner::<String>("databases.sqlite.url") {
        Ok(url) => url,
        Err(e) => { error!("database URL not configured: {e}"); return Err(rocket); }
    };

    let opts = match SqliteConnectOptions::from_str(&url) {
        Ok(opts) => opts.create_if_missing(true),
        Err(e) => { error!("invalid database URL: {e}"); return Err(rocket); }
    };

    match SqlitePool::connect_with(opts).await {
        Ok(pool) => {
            if let Err(e) = sqlx::migrate!().run(&pool).await {
                error!("migrations failed: {e}");
                return Err(rocket);
            }
            Ok(rocket.manage(pool))
        }
        Err(e) => { error!("failed to connect to database: {e}"); Err(rocket) }
    }
}

#[launch]
fn rocket() -> _ {
    rkt::build()
        .attach(AdHoc::try_on_ignite("Database", init_db))
        .mount("/", routes![/* ... */])
}
```

Handlers receive the pool as `&State<SqlitePool>`:

```rust
use rkt::get;
use rkt::State;
use rkt::serde::{Serialize, json::Json};
use sqlx::SqlitePool;

#[derive(Serialize, sqlx::FromRow)]
#[serde(crate = "rkt::serde")]
struct Post { id: i64, title: String, body: String }

#[get("/<id>")]
async fn get_post(pool: &State<SqlitePool>, id: i64) -> Option<Json<Post>> {
    sqlx::query_as::<_, Post>("SELECT id, title, body FROM posts WHERE id = ?")
        .bind(id)
        .fetch_optional(pool.inner())
        .await
        .ok()
        .flatten()
        .map(Json)
}
```

### Synchronous ORMs (Diesel, rusqlite)

For blocking ORMs, create an `r2d2::Pool` and wrap calls in [`spawn_blocking`]:

```rust
use rkt::{get, Rocket, Build, State};
use rkt::fairing::AdHoc;
use rkt::tokio::task::spawn_blocking;
use rkt::trace::error;

use diesel::r2d2::{ConnectionManager, Pool};
use diesel::SqliteConnection;

type DbPool = Pool<ConnectionManager<SqliteConnection>>;

async fn init_db(rocket: Rocket<Build>) -> rkt::fairing::Result {
    let url = match rocket.figment().extract_inner::<String>("databases.sqlite.url") {
        Ok(url) => url,
        Err(e) => { error!("database URL not configured: {e}"); return Err(rocket); }
    };

    let manager = ConnectionManager::<SqliteConnection>::new(url);
    match Pool::builder().build(manager) {
        Ok(pool) => Ok(rocket.manage(pool)),
        Err(e) => { error!("failed to build pool: {e}"); Err(rocket) }
    }
}

#[get("/<id>")]
async fn get_item(pool: &State<DbPool>, id: i32) -> Option<String> {
    let pool = pool.inner().clone();
    spawn_blocking(move || {
        use diesel::prelude::*;
        let mut conn = pool.get().ok()?;
        // diesel query here...
        Some("result".into())
    }).await.ok().flatten()
}
```

### Examples

For complete working examples — including a todo app with a web UI, templates,
and automatic migrations — see the [`todo` example](https://github.com/rustfoo/rkt/tree/main/examples/todo)
(sqlx + SQLite) and the [`databases` example](https://github.com/rustfoo/rkt/tree/main/examples/databases)
(sqlx + Diesel).

[`manage()`]: https://docs.rs/rkt/latest/rkt/struct.Rocket.html#method.manage
[`&State<Pool>`]: https://docs.rs/rkt/latest/rkt/struct.State.html
[`sqlx`]: https://docs.rs/sqlx/
[`AdHoc`]: https://docs.rs/rkt/latest/rkt/fairing/struct.AdHoc.html
[`spawn_blocking`]: https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html
