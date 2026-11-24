use std::time::Duration;

use crate::prelude::*;

#[get("/")]
async fn wake_interval(config: &Config) -> String {
    tokio::time::sleep(Duration::from_millis(50)).await;
    config.wake_interval.to_string()
}

fn test_wake_interval() -> Result<()> {
    let server = spawn! {
        Rocket::default()
            .reconfigure_with_toml("[default]\nwake_interval = 25")
            .mount("/", routes![wake_interval])
    }?;

    let response = Client::default().get(&server, "/")?.send()?;
    assert_eq!(response.error_for_status()?.text()?, "25");
    Ok(())
}

register!(test_wake_interval);
