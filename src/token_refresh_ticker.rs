use std::{
    ops::Deref,
    sync::{Arc, OnceLock},
    time::Duration,
};

use periodically::{AsyncTask, IntervalSchedule, Scheduler};
use tracing::{debug, error};

use crate::instagram_client::InstagramClient;

static INSTAGRAM_CLIENT: OnceLock<Arc<InstagramClient>> = OnceLock::new();

struct InstagramSchedulerTask;

impl AsyncTask for InstagramSchedulerTask {
    async fn run(&self) {
        let client = INSTAGRAM_CLIENT
            .get()
            .expect("INSTAGRAM_CLIENT client must be initialized before tool handlers run");
        let is_valid = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(client.check_token_validy())
            .unwrap();
        if !is_valid {
            let long_lived_token_rs = client.refresh_long_lived_token().await;
            if let Err(err) = long_lived_token_rs {
                error!(error = %err, "Failed to refresh long lived token")
            } else {
                debug!("Long live token refreshed");
                client.set_access_token(long_lived_token_rs.unwrap());
            }
        }
    }
}

pub fn create_and_run_instagram_token_validity_task(client: Arc<InstagramClient>) {
    let _ = INSTAGRAM_CLIENT.set(Arc::clone(&client));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut scheduler = Scheduler::tokio_scheduler(runtime);
    let _ = scheduler.add_async_task(
        InstagramSchedulerTask,
        IntervalSchedule::every(Duration::from_hours(60)),
    );
}
