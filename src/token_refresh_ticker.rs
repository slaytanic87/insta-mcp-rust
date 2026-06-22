use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use envie::Envie;
use periodically::{AsyncTask, IntervalSchedule, Scheduler};
use tracing::{debug, error};

use crate::{instagram_client::InstagramClient};

static INSTAGRAM_CLIENT: OnceLock<Arc<InstagramClient>> = OnceLock::new();

#[derive(Default)]
struct InstagramSchedulerTask;

impl AsyncTask for InstagramSchedulerTask {
    async fn run(&self) {
        let client = INSTAGRAM_CLIENT
            .get()
            .expect("INSTAGRAM_CLIENT client must be initialized before tool handlers run");

        let is_valid: bool = match client.check_token_validy().await {
            Ok(validity) => validity,
            Err(err) => {
                error!(error = %err, "Failed to check Instagram token validity");
                return;
            }
        };

        if is_valid {
            debug!("Long live token still valid");
            return;
        }

        let long_lived_token_rs: Result<String, reqwest::Error> = client.refresh_long_lived_token().await;
        if let Err(err) = long_lived_token_rs {
            error!(error = %err, "Failed to refresh long lived token")
        } else {
            let mut env = Envie::load().expect("Failed to load env. file");
            env.set(
                "INSTAGRAM_ACCESS_TOKEN",
                long_lived_token_rs.unwrap().as_str(),
            )
            .expect("Failed to set INSTAGRAM_ACCESS_TOKEN");
            debug!("Long live token refreshed");
        }
    }
}

pub fn create_and_run_instagram_token_validity_task(client: Arc<InstagramClient>) {
    let _ = INSTAGRAM_CLIENT.set(Arc::clone(&client));
    let mut scheduler = Scheduler::tokio_scheduler_with_current();
    let _ = scheduler.add_async_task(
        InstagramSchedulerTask::default(),
        IntervalSchedule::every(Duration::from_hours(4)),
    );
}
