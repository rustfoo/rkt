extern crate rkt;

use std::net::{Ipv4Addr, SocketAddr};

use rkt::config::Config;
use rkt::fairing::AdHoc;
use rkt::futures::channel::oneshot;
use rkt::listener::tcp::TcpListener;

#[rkt::async_test]
async fn on_ignite_fairing_can_inspect_port() {
    let (tx, rx) = oneshot::channel();
    let rocket = rkt::custom(Config::debug_default()).attach(AdHoc::on_liftoff(
        "Send Port -> Channel",
        move |rocket| {
            Box::pin(async move {
                let tcp = rocket.endpoints().find_map(|v| v.tcp());
                tx.send(tcp.unwrap().port()).expect("send okay");
            })
        },
    ));

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    rkt::tokio::spawn(rocket.try_launch_on(TcpListener::bind(addr)));
    assert_ne!(rx.await.unwrap(), 0);
}
