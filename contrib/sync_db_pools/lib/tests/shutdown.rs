#[cfg(test)]
#[cfg(feature = "diesel_sqlite_pool")]
mod sqlite_shutdown_test {
    use rkt::{async_test, Build, Rocket};
    use rkt_sync_db_pools::database;

    #[database("test")]
    struct Pool(diesel::SqliteConnection);

    async fn rocket() -> Rocket<Build> {
        use rkt::figment::{util::map, Figment};

        let options = map!["url" => ":memory:"];
        let config = Figment::from(rkt::Config::debug_default())
            .merge(("port", 0))
            .merge(("databases", map!["test" => &options]));

        rkt::custom(config).attach(Pool::fairing())
    }

    #[test]
    fn test_shutdown() {
        let _rocket = async_test(async {
            let rocket = rocket().await.ignite().await.expect("unable to ignite");
            // request shutdown
            rocket.shutdown().notify();
            rocket.launch().await.expect("unable to launch")
        });
        // _rocket is dropped here after the runtime is dropped
    }
}
