use flatbuffers::FlatBufferBuilder;
use std::{
    env,
    f64::consts::PI,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[allow(non_snake_case, unused_imports, dead_code)]
mod Time_generated {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/schemas/generated/rust/Time_generated.rs"
    ));
    pub use self::foxglove::*;
}

#[allow(non_snake_case, unused_imports, dead_code)]
mod Vector3_generated {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/schemas/generated/rust/Vector3_generated.rs"
    ));
    pub use self::foxglove::*;
}

#[allow(non_snake_case, unused_imports, dead_code)]
mod Quaternion_generated {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/schemas/generated/rust/Quaternion_generated.rs"
    ));
    pub use self::foxglove::*;
}

#[allow(non_snake_case, unused_imports, dead_code)]
mod FrameTransform_generated {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/schemas/generated/rust/FrameTransform_generated.rs"
    ));
    pub use self::foxglove::*;
}

#[allow(non_snake_case, unused_imports, dead_code)]
mod FrameTransforms_generated {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/schemas/generated/rust/FrameTransforms_generated.rs"
    ));
    pub use self::foxglove::*;
}

use FrameTransform_generated::foxglove::{FrameTransform, FrameTransformArgs};
use FrameTransforms_generated::foxglove::{
    FrameTransforms, FrameTransformsArgs, finish_frame_transforms_buffer,
};
use Quaternion_generated::foxglove::{Quaternion, QuaternionArgs};
use Time_generated::foxglove::Time;
use Vector3_generated::foxglove::{Vector3, Vector3Args};

const SHM_ADDRESS: &str = "helix_publisher";
const HELIX_MESSAGE_LEN: usize = 184;
const DEFAULT_HZ: f64 = 30.0;
const RADIUS_METERS: f64 = 2.0;
const ANGULAR_SPEED_RAD_PER_SEC: f64 = 1.0;
const CLIMB_RATE_M_PER_SEC: f64 = 0.5;
const EPSILON: f64 = 1.0e-9;

#[repr(C)]
#[derive(Debug, PartialEq)]
struct HelixFrameTransforms {
    bytes: [u8; HELIX_MESSAGE_LEN],
}

#[tokio::main]
async fn main() {
    let run_once = env::args().any(|arg| arg == "--once");
    let init_data = HelixFrameTransforms {
        bytes: build_frame_transforms_bytes(0.0),
    };
    let mut shm = tism::create(SHM_ADDRESS, init_data).expect("failed to create TISM allocation");

    println!(
        "helix publisher writing {} bytes to TISM address `{}`",
        HELIX_MESSAGE_LEN, SHM_ADDRESS
    );

    if run_once {
        publish_once(&mut shm, 0.0);
        return;
    }

    let start = Instant::now();
    let sleep_duration = Duration::from_secs_f64(1.0 / DEFAULT_HZ);

    loop {
        let elapsed_secs = start.elapsed().as_secs_f64();
        publish_once(&mut shm, elapsed_secs);

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("helix publisher shutting down cleanly");
                break;
            }
            _ = tokio::time::sleep(sleep_duration) => {}
        }
    }
}

fn publish_once(shm: &mut tism::OwnedSharedMemory<HelixFrameTransforms>, elapsed_secs: f64) {
    let next_payload = build_frame_transforms_bytes(elapsed_secs);

    if let Ok(mut lock) = shm.write_lock() {
        lock.as_mut().bytes.copy_from_slice(&next_payload);
    }
}

fn build_frame_transforms_bytes(elapsed_secs: f64) -> [u8; HELIX_MESSAGE_LEN] {
    let phase = elapsed_secs * ANGULAR_SPEED_RAD_PER_SEC;
    let x = (RADIUS_METERS * phase.cos()) + EPSILON;
    let y = (RADIUS_METERS * phase.sin()) + EPSILON;
    let z = (CLIMB_RATE_M_PER_SEC * elapsed_secs) + EPSILON;
    let yaw = phase + (PI / 2.0);
    let yaw_half = yaw / 2.0;
    let (qx, qy, qz, qw) = normalized_quaternion(
        EPSILON,
        EPSILON,
        yaw_half.sin() + EPSILON,
        yaw_half.cos() + EPSILON,
    );

    let timestamp = system_time_to_foxglove_time(SystemTime::now());
    let mut builder = FlatBufferBuilder::with_capacity(256);

    let parent_frame_id = builder.create_string("base");
    let child_frame_id = builder.create_string("rocket");
    let translation = Vector3::create(&mut builder, &Vector3Args { x, y, z });
    let rotation = Quaternion::create(
        &mut builder,
        &QuaternionArgs {
            x: qx,
            y: qy,
            z: qz,
            w: qw,
        },
    );
    let transform = FrameTransform::create(
        &mut builder,
        &FrameTransformArgs {
            timestamp: Some(&timestamp),
            parent_frame_id: Some(parent_frame_id),
            child_frame_id: Some(child_frame_id),
            translation: Some(translation),
            rotation: Some(rotation),
        },
    );
    let transforms = builder.create_vector(&[transform]);
    let root = FrameTransforms::create(
        &mut builder,
        &FrameTransformsArgs {
            transforms: Some(transforms),
        },
    );
    finish_frame_transforms_buffer(&mut builder, root);

    let payload = builder.finished_data();
    assert_eq!(
        payload.len(),
        HELIX_MESSAGE_LEN,
        "helix publisher payload width changed; update HELIX_MESSAGE_LEN"
    );

    let mut bytes = [0_u8; HELIX_MESSAGE_LEN];
    bytes.copy_from_slice(payload);
    bytes
}

fn system_time_to_foxglove_time(time: SystemTime) -> Time {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch");

    Time::new(
        duration
            .as_secs()
            .try_into()
            .expect("timestamp seconds overflow"),
        duration.subsec_nanos(),
    )
}

fn normalized_quaternion(x: f64, y: f64, z: f64, w: f64) -> (f64, f64, f64, f64) {
    let norm = (x * x + y * y + z * z + w * w).sqrt();
    (x / norm, y / norm, z / norm, w / norm)
}
