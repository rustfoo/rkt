extern crate rkt;

use rkt::fairing::AdHoc;

#[rkt::async_test]
async fn test_inspectable_launch_state() -> Result<(), rkt::Error> {
    let rocket = rkt::custom(rkt::Config::debug_default())
        .attach(AdHoc::on_ignite("Add State", |rocket| async {
            rocket.manage("Hi!")
        }))
        .ignite()
        .await?;

    let state = rocket.state::<&'static str>();
    assert_eq!(state, Some(&"Hi!"));
    Ok(())
}

#[rkt::async_test]
async fn test_inspectable_launch_state_in_liftoff() -> Result<(), rkt::Error> {
    let rocket = rkt::custom(rkt::Config::debug_default())
        .attach(AdHoc::on_ignite("Add State", |rocket| async {
            rocket.manage("Hi!")
        }))
        .attach(AdHoc::on_ignite("Inspect State", |rocket| async {
            let state = rocket.state::<&'static str>();
            assert_eq!(state, Some(&"Hi!"));
            rocket
        }))
        .attach(AdHoc::on_liftoff("Inspect State", |rocket| {
            Box::pin(async move {
                let state = rocket.state::<&'static str>();
                assert_eq!(state, Some(&"Hi!"));
            })
        }))
        .ignite()
        .await?;

    let state = rocket.state::<&'static str>();
    assert_eq!(state, Some(&"Hi!"));
    Ok(())
}

#[rkt::async_test]
async fn test_launch_state_is_well_ordered() -> Result<(), rkt::Error> {
    let rocket = rkt::custom(rkt::Config::debug_default())
        .attach(AdHoc::on_ignite("Inspect State Pre", |rocket| async {
            let state = rocket.state::<&'static str>();
            assert_eq!(state, None);
            rocket
        }))
        .attach(AdHoc::on_ignite("Add State", |rocket| async {
            rocket.manage("Hi!")
        }))
        .attach(AdHoc::on_ignite("Inspect State", |rocket| async {
            let state = rocket.state::<&'static str>();
            assert_eq!(state, Some(&"Hi!"));
            rocket
        }))
        .ignite()
        .await?;

    let state = rocket.state::<&'static str>();
    assert_eq!(state, Some(&"Hi!"));
    Ok(())
}

#[should_panic]
#[rkt::async_test]
async fn negative_test_launch_state() {
    let _ = rkt::custom(rkt::Config::debug_default())
        .attach(AdHoc::on_ignite("Add State", |rocket| async {
            rocket.manage("Hi!")
        }))
        .attach(AdHoc::on_ignite("Inspect State", |rocket| async {
            let state = rocket.state::<&'static str>();
            assert_ne!(state, Some(&"Hi!"));
            rocket
        }))
        .ignite()
        .await;
}
