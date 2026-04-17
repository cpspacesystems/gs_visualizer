use crate::{BridgeConfig, BridgeError, BridgeOptions, ChannelConfig, publisher, source_tism::TismSource};
use foxglove::{Context, RawChannel, WebSocketServerHandle};
use std::{
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
            Ok(())
        }
        result = workers.join_next(), if !workers.is_empty() => {
            match result {
                Some(joined) => joined?,
                None => Ok(()),
            }
        }
    };

    shutdown.store(true, Ordering::Relaxed);

    while let Some(joined) = workers.join_next().await {
        match joined {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                if outcome.is_ok() {
                    return stop_server(server).await.and(Err(err));
                }
            }
            Err(err) => {
                if outcome.is_ok() {
                    return stop_server(server).await.and(Err(BridgeError::from(err)));
                }
            }
        }
    }

    stop_server(server).await?;
    outcome
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
    let mut source: Option<TismSource> = None;
    let mut last_payload: Option<Vec<u8>> = None;

    info!(
        topic = prepared.config.topic,
        address = prepared.config.tism_address,
        "starting channel worker"
    );

    while !shutdown.load(Ordering::Relaxed) {
        if source.is_none() {
            match TismSource::open(&prepared.config.tism_address) {
                Ok(opened) => {
                    info!(
                        topic = prepared.config.topic,
                        address = prepared.config.tism_address,
                        "opened TISM source"
                    );
                    source = Some(opened);
                }
                Err(err) => {
                    warn!(
                        topic = prepared.config.topic,
                        address = prepared.config.tism_address,
                        "failed to open TISM source: {err}"
                    );
                    sleep_with_shutdown(open_retry, &shutdown);
                    continue;
                }
            }
        }

        let loop_started = Instant::now();
        let read_result = source
            .as_mut()
            .expect("source must be open before reading")
            .read();

        match read_result {
            Ok(payload) => {
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
                warn!(
                    topic = prepared.config.topic,
                    address = prepared.config.tism_address,
                    "failed reading TISM source: {err}"
                );
                source = None;
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

fn should_skip_publish(channel: &ChannelConfig, payload: &[u8], last_payload: Option<&[u8]>) -> bool {
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
