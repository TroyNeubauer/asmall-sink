mod runtime;
mod types;
mod validator;

use types::*;
use validator::*;

use crossbeam_channel::{bounded as bounded_sync, Receiver as ReceiverSync, Sender as SenderSync};
use rand::Rng;
use std::{
    thread,
    time::{Duration, Instant},
};

fn main() {
    let (terrain_request_tx, terrain_request_rx) = bounded_sync(256);
    let (terrain_response_tx, terrain_response_rx) = bounded_sync(256);
    let mut validator = GrpcValidator::new(terrain_request_tx);

    let mut last_request = Instant::now() - Duration::from_secs(20);
    let mut rng = rand::thread_rng();
    let _terrain_task = terrain_driver(terrain_request_rx, terrain_response_tx);

    // let terrain task start...
    thread::sleep(Duration::from_millis(50));

    let mut last_stats_log = Instant::now();
    let mut num_requests_sent = 0;
    let mut num_requests_received = 0;
    loop {
        // create random request
        if true || last_request.elapsed().as_millis() > 15 {
            last_request = Instant::now();

            let req = match rng.gen_range(0..3) {
                0 => RawGrpcRequest::Basic,
                1 => RawGrpcRequest::FlyTo(LLA::new_rand(&mut rng)),
                2 => {
                    let mut v = vec![];
                    for _ in 0..rng.gen_range(2..5) {
                        v.push(LLA::new_rand(&mut rng));
                    }
                    RawGrpcRequest::Waypoint(v)
                }
                _ => unreachable!(),
            };
            num_requests_sent += 1;
            validator.insert(req);
        }

        // update validator

        validator.tick();

        while let Ok(r) = terrain_response_rx.try_recv() {
            validator.update_terrain(&r);
        }

        if let Some(_v) = validator.take_validated() {
            num_requests_received += 1;
            //println!();
            //println!("VALIDATED REQUEST: {v:#?}");
            //println!();
        }

        // log stats

        let now = Instant::now();
        if now.duration_since(last_stats_log).as_secs_f64() >= 1. {
            last_stats_log = now;
            println!();
            println!("Requests sent: {num_requests_sent}");
            println!("Requests received: {num_requests_received}");
            println!(
                "Throughpet: {} million requests / second",
                num_requests_received as f64 / 1_000_000.
            );

            let num_dropped = num_requests_sent - num_requests_received;
            println!("Requests dropped: {num_dropped}");

            println!(
                "Percent dropped: {}",
                num_dropped as f64 / num_requests_sent as f64 * 100.
            );

            num_requests_sent = 0;
            num_requests_received = 0;
        }
    }
}

pub fn terrain_driver(
    terrain_request_rx: ReceiverSync<TerrainQuery>,
    terrain_response_tx: SenderSync<TerrainResponse>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut rng = rand::thread_rng();
        loop {
            //thread::sleep(Duration::from_millis(5));
            if let Ok(r) = terrain_request_rx.try_recv() {
                // add latency...
                //thread::sleep(Duration::from_millis(5));

                if terrain_response_tx
                    .send(TerrainResponse {
                        terrain: LLA {
                            lat: r.lat,
                            lon: r.lon,
                            alt: rng.gen_range(0.0..50.0),
                        },
                        seq: r.seq,
                    })
                    .is_err()
                {
                    println!("Failed to send terrain request");
                }
            }
        }
    })
}
