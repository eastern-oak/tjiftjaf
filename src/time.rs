#[cfg(not(target_arch = "wasm32"))]
pub use std::time::{Instant, SystemTime};

#[cfg(target_arch = "wasm32")]
pub use web_time::{Instant, SystemTime};

#[cfg(target_arch = "wasm32")]
mod wasm {
    use js_sys::Promise;
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use std::time::Duration;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;
    use web_time::Instant;

    pub struct Timeout {
        id: JsValue,
        inner: JsFuture,
    }

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_name = setTimeout)]
        fn set_timeout(closure: JsValue, millis: f64) -> JsValue;

        #[wasm_bindgen(js_name = clearTimeout)]
        fn clear_timeout(id: &JsValue);
    }

    impl Timeout {
        pub fn new(dur: Duration) -> Timeout {
            let millis = dur
                .as_secs()
                .checked_mul(1000)
                .unwrap()
                .checked_add(dur.subsec_millis() as u64)
                .unwrap() as f64; // TODO: checked cast

            let mut id = None;
            let promise = Promise::new(&mut |resolve, _reject| {
                id = Some(set_timeout(resolve.into(), millis));
            });

            Timeout {
                id: id.unwrap(),
                inner: JsFuture::from(promise),
            }
        }
    }

    impl Future for Timeout {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<()> {
            Pin::new(&mut self.inner).poll(cx).map(|_| ())
        }
    }

    impl Drop for Timeout {
        fn drop(&mut self) {
            clear_timeout(&self.id);
        }
    }

    /// Creates a timer that emits an event once at the given time instant.
    /// This recreates `async_io::Timer::at` in wasm using browser APIs.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    ///
    /// # futures_lite::future::block_on(async {
    /// let now = Instant::now();
    /// let when = now + Duration::from_secs(1);
    /// timer_at(when).await;
    /// # });
    /// ```
    pub async fn timer_at(timeout: Instant) {
        let duration = timeout.duration_since(Instant::now());
        Timeout::new(duration).await;
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn timer_at(timeout: Instant) {
    async_io::Timer::at(timeout).await;
}

#[cfg(target_arch = "wasm32")]
pub use wasm::timer_at;
