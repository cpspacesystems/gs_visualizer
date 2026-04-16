use foxglove::{
    WebSocketServer, log,
    messages::{Log, Timestamp, log::Level},
};
use std::{thread, time::Duration};

fn main() {
    WebSocketServer::new()
        .start_blocking()
        .expect("Server failed to start");

    loop {
        log!(
            "/hello",
            Log {
                level: Level::Info.into(),
                timestamp: Some(Timestamp::now()),
                message: "Hello, Foxglove!".to_string(),
                ..Default::default()
            }
        );
        thread::sleep(Duration::from_millis(100));
    }
}

