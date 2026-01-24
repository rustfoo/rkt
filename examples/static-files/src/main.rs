#[cfg(test)] mod tests;

use rkt::fs::{FileServer, relative};

// If we wanted or needed to serve files manually, we'd use `NamedFile`. Always
// prefer to use `FileServer`!
mod manual {
    use std::path::{PathBuf, Path};
    use rkt::fs::NamedFile;

    #[rkt::get("/second/<path..>")]
    pub async fn second(path: PathBuf) -> Option<NamedFile> {
        let mut path = Path::new(super::relative!("static")).join(path);
        if path.is_dir() {
            path.push("index.html");
        }

        NamedFile::open(path).await.ok()
    }
}

#[rkt::launch]
fn rocket() -> _ {
    rkt::build()
        .mount("/", rkt::routes![manual::second])
        .mount("/", FileServer::new(relative!("static")))
}
