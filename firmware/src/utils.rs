use embassy_time::{Duration, Timer};
use futures::{
    future::{select, Either},
    pin_mut, Future,
};

#[macro_export]
macro_rules! sleep {
    ($ms:expr) => {{
        use embassy_time::{Duration, Timer};
        Timer::after(Duration::from_millis($ms)).await;
    }};
}

#[macro_export]
macro_rules! checkBit {
    ($byte:expr, $bit:expr) => {
        $byte & $bit == $bit
    };
}

pub async fn run_with_timeout<F: Future>(ms: u64, future: F) -> Result<F::Output, u64> {
    let timeout_fut = Timer::after(Duration::from_millis(ms));
    pin_mut!(future);
    match select(timeout_fut, future).await {
        Either::Left(_) => Err(ms),
        Either::Right((result, _)) => Ok(result),
    }
}
