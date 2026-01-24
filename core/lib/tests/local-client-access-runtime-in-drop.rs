extern crate rkt;

use rkt::local::blocking::Client;

struct SpawnBlockingOnDrop;

impl Drop for SpawnBlockingOnDrop {
    fn drop(&mut self) {
        rkt::tokio::task::spawn_blocking(|| ());
    }
}

#[test]
fn test_access_runtime_in_state_drop() {
    Client::debug(rkt::build().manage(SpawnBlockingOnDrop)).unwrap();
}
