// Tauri 에 묶이지 않은 Amber 의 Rust. 지금 앱(src-tauri)과 GPUI 앱이 같은 코드를 쓴다.

pub mod ai;
pub mod auth;
pub mod dbconn;
pub mod detect;
pub mod report;
pub mod shellenv;

#[cfg(feature = "tauri")]
pub use tauri::ipc::Channel;

#[cfg(not(feature = "tauri"))]
pub use callback::Channel;

/// 스트리밍 결과(글 조각, 진행 상황)를 받는 쪽. Tauri 의 Channel 과 같은 모양으로 쓴다
#[cfg(not(feature = "tauri"))]
mod callback {
    use std::sync::Arc;

    pub struct Channel<T>(Arc<dyn Fn(T) + Send + Sync>);

    impl<T> Clone for Channel<T> {
        fn clone(&self) -> Self {
            Channel(self.0.clone())
        }
    }

    impl<T> Channel<T> {
        pub fn new(f: impl Fn(T) + Send + Sync + 'static) -> Self {
            Channel(Arc::new(f))
        }

        pub fn send(&self, value: T) -> Result<(), std::convert::Infallible> {
            (self.0)(value);
            Ok(())
        }
    }
}
