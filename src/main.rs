use std::{env, str::FromStr};

use color_eyre::Result;
use corgi::run;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // set up logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(
            env::var("CORGI_LOG_LEVEL")
                .ok()
                .and_then(|s| Level::from_str(&s).ok())
                .unwrap_or(Level::WARN),
        )
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;
    color_eyre::install()?;

    // start app
    run().await
}
