use futures::{
	channel::mpsc::{UnboundedReceiver, UnboundedSender},
	Future,
};
use once_cell::sync::{Lazy, OnceCell};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::{
	sync::{Semaphore, SemaphorePermit},
	task::{self, JoinHandle},
	time,
};

/// Global job queue
static TASKS: OnceCell<UnboundedSender<JoinHandle<()>>> = OnceCell::new();
static TASKS_RUNNING: Lazy<Semaphore> = Lazy::new(|| Semaphore::new(0));
static REQUEST_TICKETS: Lazy<Semaphore> = Lazy::new(|| Semaphore::new(0));

/// Every HTTP request in the program passes through here, so this counts all of them.
static REQUESTS_MADE: AtomicUsize = AtomicUsize::new(0);

pub async fn get_request_ticket() {
	REQUEST_TICKETS.acquire().await.unwrap().forget();
	REQUESTS_MADE.fetch_add(1, Ordering::Relaxed);
}

pub fn requests_made() -> usize {
	REQUESTS_MADE.load(Ordering::Relaxed)
}

pub async fn get_ticket() -> SemaphorePermit<'static> {
	TASKS_RUNNING.acquire().await.unwrap()
}

pub fn spawn(e: impl Future<Output = ()> + Send + 'static) {
	TASKS.get().unwrap().unbounded_send(task::spawn(e)).unwrap();
}

pub fn set_download_rate(rate: usize) {
	task::spawn(async move {
		let mut interval = time::interval(time::Duration::from_secs_f64(60.0 / rate as f64));
		loop {
			interval.tick().await;
			REQUEST_TICKETS.add_permits(1);
		}
	});
}

pub fn set_parallel_jobs(jobs: usize) -> UnboundedReceiver<JoinHandle<()>> {
	let (tx, rx) = futures::channel::mpsc::unbounded::<JoinHandle<()>>();
	TASKS.get_or_init(|| tx.clone());
	TASKS_RUNNING.add_permits(jobs);
	rx
}
