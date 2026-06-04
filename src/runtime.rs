use crate::{BridgeConfig, BridgeError, BridgeOptions, ChannelConfig, publisher};
use foxglove::{Context, RawChannel, WebSocketServerHandle};
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tokio::task::JoinSet;
use tracing::{info, warn};

struct PreparedChannel {
    config: ChannelConfig,
    raw_channel: Arc<RawChannel>,
}

enum BridgeOutcome {
    Shutdown,
    Worker(Result<(), BridgeError>),
}

pub async fn run_bridge(config: BridgeConfig) -> Result<(), BridgeError> {
    init_tracing();

    let context = Context::new();
    let prepared = prepare_channels(&context, &config)?;
    let server = start_server(&context, &config).await?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let mut workers = spawn_workers(prepared, config.bridge.clone(), shutdown.clone());

    info!(
        "Foxglove websocket server listening on {}",
        server.app_url().to_string()
    );

    let outcome = tokio::select! {
        signal = tokio::signal::ctrl_c() => {
            signal?;
            info!("received Ctrl-C, shutting down");
            BridgeOutcome::Shutdown
        }
        result = workers.join_next(), if !workers.is_empty() => {
            match result {
                Some(joined) => BridgeOutcome::Worker(joined?),
                None => BridgeOutcome::Worker(Ok(())),
            }
        }
    };

    shutdown.store(true, Ordering::Relaxed);
    workers.detach_all();
    stop_server(server).await?;

    match outcome {
        BridgeOutcome::Shutdown => Ok(()),
        BridgeOutcome::Worker(result) => result,
    }
}

fn prepare_channels(
    context: &Arc<Context>,
    config: &BridgeConfig,
) -> Result<Vec<PreparedChannel>, BridgeError> {
    config
        .channels
        .iter()
        .cloned()
        .map(|channel| {
            let raw_channel = publisher::build_channel(context, &channel)?;
            Ok(PreparedChannel {
                config: channel,
                raw_channel,
            })
        })
        .collect()
}

async fn start_server(
    context: &Arc<Context>,
    config: &BridgeConfig,
) -> Result<WebSocketServerHandle, BridgeError> {
    context
        .websocket_server()
        .name(config.server.name.clone())
        .bind(config.server.host.clone(), config.server.port)
        .message_backlog_size(config.server.message_backlog_size)
        .start()
        .await
        .map_err(BridgeError::from)
}

fn spawn_workers(
    prepared: Vec<PreparedChannel>,
    options: BridgeOptions,
    shutdown: Arc<AtomicBool>,
) -> JoinSet<Result<(), BridgeError>> {
    let mut workers = JoinSet::new();

    for prepared_channel in prepared {
        let shutdown = shutdown.clone();
        let options = options.clone();
        workers.spawn_blocking(move || run_channel_worker(prepared_channel, options, shutdown));
    }

    workers
}

fn run_channel_worker(
    prepared: PreparedChannel,
    options: BridgeOptions,
    shutdown: Arc<AtomicBool>,
) -> Result<(), BridgeError> {
    let publish_period =
        Duration::from_secs_f64(1.0 / prepared.config.effective_publish_hz(&options));
    let open_retry = Duration::from_millis(options.open_retry_ms);
    let mut last_payload: Option<Vec<u8>> = None;
    let mut source_connected = false;
    let mut waiting_for_publisher = false;

    info!(
        topic = prepared.config.topic,
        address = prepared.config.tism_address,
        "starting channel worker"
    );

    while !shutdown.load(Ordering::Relaxed) {
        let loop_started = Instant::now();
        let read_result = {
            let mut shm = tism::dynamic::open(prepared.config.tism_address.as_str())?;
            shm.read()
        };

        match read_result {
            Ok(payload) => {
                if !source_connected {
                    info!(
                        topic = prepared.config.topic,
                        address = prepared.config.tism_address,
                        "connected to TISM publisher"
                    );
                    source_connected = true;
                }
                waiting_for_publisher = false;

                if should_skip_publish(&prepared.config, &payload, last_payload.as_deref()) {
                } else if exceeds_max_size(&prepared.config, payload.len()) {
                    warn!(
                        topic = prepared.config.topic,
                        limit = prepared.config.max_message_bytes.unwrap_or(0),
                        size = payload.len(),
                        "skipping payload that exceeds configured max_message_bytes"
                    );

                    if prepared.config.on_change_only {
                        last_payload = Some(payload);
                    }
                } else {
                    prepared.raw_channel.log(&payload);

                    if prepared.config.on_change_only {
                        last_payload = Some(payload);
                    }
                }
            }
            Err(err) => {
                last_payload = None;

                if err.kind() == io::ErrorKind::NotFound {
                    if source_connected || !waiting_for_publisher {
                        info!(
                            topic = prepared.config.topic,
                            address = prepared.config.tism_address,
                            "waiting for live TISM publisher"
                        );
                    }
                    source_connected = false;
                    waiting_for_publisher = true;
                    sleep_with_shutdown(open_retry, &shutdown);
                    continue;
                }

                waiting_for_publisher = false;
                source_connected = false;
                warn!(
                    topic = prepared.config.topic,
                    address = prepared.config.tism_address,
                    "failed reading TISM source: {err}"
                );
                sleep_with_shutdown(open_retry, &shutdown);
                continue;
            }
        }

        let elapsed = loop_started.elapsed();
        if publish_period > elapsed {
            sleep_with_shutdown(publish_period - elapsed, &shutdown);
        }
    }

    info!(
        topic = prepared.config.topic,
        address = prepared.config.tism_address,
        "channel worker stopped"
    );
    Ok(())
}

fn should_skip_publish(
    channel: &ChannelConfig,
    payload: &[u8],
    last_payload: Option<&[u8]>,
) -> bool {
    channel.on_change_only && last_payload == Some(payload)
}

fn exceeds_max_size(channel: &ChannelConfig, payload_len: usize) -> bool {
    channel
        .max_message_bytes
        .map(|limit| payload_len > limit)
        .unwrap_or(false)
}

fn sleep_with_shutdown(duration: Duration, shutdown: &AtomicBool) {
    let slice = Duration::from_millis(50);
    let mut remaining = duration;

    while remaining > Duration::ZERO && !shutdown.load(Ordering::Relaxed) {
        let step = remaining.min(slice);
        thread::sleep(step);
        remaining = remaining.saturating_sub(step);
    }
}

async fn stop_server(server: WebSocketServerHandle) -> Result<(), BridgeError> {
    server.stop().wait().await;
    Ok(())
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gs_visualizer=info".into()),
        )
        .with_target(false)
        .try_init();
}
