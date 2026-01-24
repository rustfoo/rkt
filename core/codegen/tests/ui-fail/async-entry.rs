#![allow(dead_code)]

mod main_a {
    #[rkt::main]
    fn foo() { }

}

mod main_b {
    #[rkt::main]
    async fn foo() { }

}

mod main_d {
    #[rkt::main]
    fn main() {
        let _ = rkt::build().launch().await;
    }
}

mod main_f {
    #[rkt::main]
    async fn main() {
        rkt::build()
    }
}

// launch

mod launch_a {
    #[rkt::launch]
    async fn rocket() -> String {
        let _ = rkt::build().launch().await;
        rkt::build()

    }
}

mod launch_b {
    #[rkt::launch]
    async fn rocket() -> _ {
        let _ = rkt::build().launch().await;
        "hi".to_string()
    }
}

mod launch_c {
    #[rkt::launch]
    fn main() -> rkt::Rocket<rkt::Build> {
        rkt::build()
    }
}

mod launch_d {
    #[rkt::launch]
    async fn rocket() {
        let _ = rkt::build().launch().await;
        rkt::build()
    }
}

mod launch_e {
    #[rkt::launch]
    fn rocket() {
        rkt::build()
    }
}

mod launch_f {
    #[rkt::launch]
    fn rocket() -> _ {
        let _ = rkt::build().launch().await;
        rkt::build()
    }
}

mod launch_g {
    #[rkt::launch]
    fn main() -> &'static str {
        let _ = rkt::build().launch().await;
        "hi"
    }
}

mod launch_h {
    #[rkt::launch]
    async fn main() -> _ {
        rkt::build()
    }
}

#[rkt::main]
async fn main() -> rkt::Rocket<rkt::Build> {
    rkt::build()
}
