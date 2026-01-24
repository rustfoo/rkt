#![allow(dead_code, unused_variables)]

mod a {
    // async launch that is async.
    #[rkt::launch]
    async fn rocket() -> rkt::Rocket<rkt::Build> {
        let _ = rkt::build().launch().await;
        rkt::build()
    }

    async fn use_it() {
        let rocket: rkt::Rocket<rkt::Build> = rocket().await;
    }
}

mod b {
    // async launch that isn't async.
    #[rkt::launch]
    async fn main2() -> _ {
        rkt::build()
    }

    async fn use_it() {
        let rocket: rkt::Rocket<_> = main2().await;
    }
}

mod b_inferred {
    #[rkt::launch]
    async fn main2() -> _ {
        rkt::build()
    }

    async fn use_it() {
        let rocket: rkt::Rocket<_> = main2().await;
    }
}

mod c {
    // non-async launch.
    #[rkt::launch]
    fn rocket() -> _ {
        rkt::build()
    }

    fn use_it() {
        let rocket: rkt::Rocket<_> = rocket();
    }
}

mod c_inferred {
    #[rkt::launch]
    fn rocket() -> _ {
        rkt::build()
    }

    fn use_it() {
        let rocket: rkt::Rocket<_> = rocket();
    }
}

mod d {
    // main with async, is async.
    #[rkt::main]
    async fn main() {
        let _ = rkt::build().launch().await;
    }
}

mod e {
    // main with async, isn't async.
    #[rkt::main]
    async fn main() {}
}

mod f {
    // main with async, is async, with termination return.
    #[rkt::main]
    async fn main() -> Result<(), rkt::Error> {
        let _: rkt::Rocket<rkt::Ignite> = rkt::build().launch().await?;
        Ok(())
    }
}

mod g {
    // main with async, isn't async, with termination return.
    #[rkt::main]
    async fn main() -> Result<(), String> {
        Ok(())
    }
}

// main with async, is async, with termination return.
#[rkt::main]
async fn main() -> Result<(), String> {
    let result = rkt::build().launch().await;
    let _: rkt::Rocket<rkt::Ignite> = result.map_err(|e| e.to_string())?;
    Ok(())
}
