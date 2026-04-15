mod data;

use std::alloc::Layout;
use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::ptr::NonNull;
use std::time::Instant;

use data::OrderUpdate;
use plexus::{RingError, SharedArena, SharedChannel};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Tcp,
    Plexus,
}

#[derive(Clone, Copy, Debug)]
struct Config {
    mode: Mode,
    iterations: u64,
}

#[derive(Clone, Copy, Debug)]
struct BenchReport {
    total_secs: f64,
    avg_latency_ns: u128,
    throughput_msg_s: f64,
}

#[derive(Debug)]
struct CliError(String);

impl Display for CliError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for CliError {}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = parse_args()?;
    let report = match config.mode {
        Mode::Tcp => run_tcp_json_loopback(config.iterations).await?,
        Mode::Plexus => run_plexus_zero_copy(config.iterations)?,
    };

    print_report(config.mode, config.iterations, report);
    Ok(())
}

fn parse_args() -> Result<Config, Box<dyn Error>> {
    let mut mode = None;
    let mut iterations = None;
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mode" => {
                let raw = args
                    .next()
                    .ok_or_else(|| CliError("missing value for --mode".to_string()))?;
                mode = Some(match raw.as_str() {
                    "tcp" => Mode::Tcp,
                    "plexus" => Mode::Plexus,
                    _ => return Err(CliError(format!("unsupported mode: {raw}")).into()),
                });
            }
            "--iterations" => {
                let raw = args
                    .next()
                    .ok_or_else(|| CliError("missing value for --iterations".to_string()))?;
                iterations = Some(
                    raw.parse::<u64>()
                        .map_err(|_| CliError(format!("invalid --iterations value: {raw}")))?,
                );
            }
            "--help" | "-h" => {
                println!("Usage: cargo run --release -- --mode <tcp|plexus> --iterations <N>");
                std::process::exit(0);
            }
            other => {
                return Err(CliError(format!("unknown argument: {other}")).into());
            }
        }
    }

    let mode = mode.ok_or_else(|| CliError("missing required --mode".to_string()))?;
    let iterations =
        iterations.ok_or_else(|| CliError("missing required --iterations".to_string()))?;
    Ok(Config { mode, iterations })
}

async fn run_tcp_json_loopback(iterations: u64) -> Result<BenchReport, Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        for _ in 0..iterations {
            // Length-prefixed framing keeps message boundaries explicit over a byte stream.
            let mut len_buf = [0_u8; 4];
            stream.read_exact(&mut len_buf).await?;
            let msg_len = u32::from_le_bytes(len_buf) as usize;
            let mut payload = vec![0_u8; msg_len];
            stream.read_exact(&mut payload).await?;
            let update: OrderUpdate = serde_json::from_slice(&payload)?;
            let response = serde_json::to_vec(&update)?;
            stream
                .write_all(&(response.len() as u32).to_le_bytes())
                .await?;
            stream.write_all(&response).await?;
        }
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    });

    let mut client = TcpStream::connect(addr).await?;
    // Disable Nagle to avoid coalescing delays in loopback microbench mode.
    client.set_nodelay(true)?;
    let started = Instant::now();

    for seq in 0..iterations {
        let update = OrderUpdate::synthetic(seq);
        let encoded = serde_json::to_vec(&update)?;
        client
            .write_all(&(encoded.len() as u32).to_le_bytes())
            .await?;
        client.write_all(&encoded).await?;

        let mut len_buf = [0_u8; 4];
        client.read_exact(&mut len_buf).await?;
        let msg_len = u32::from_le_bytes(len_buf) as usize;
        let mut payload = vec![0_u8; msg_len];
        client.read_exact(&mut payload).await?;
        let echoed: OrderUpdate = serde_json::from_slice(&payload)?;
        if echoed.sequence != update.sequence {
            return Err(CliError(format!(
                "sequence mismatch in tcp mode: expected {}, got {}",
                update.sequence, echoed.sequence
            ))
            .into());
        }
    }

    let elapsed = started.elapsed();
    let server_result = server
        .await
        .map_err(|err| CliError(format!("tcp server task join failed: {err}")))?;
    server_result.map_err(|err| CliError(format!("tcp server failed: {err}")))?;

    Ok(build_report(elapsed, iterations))
}

fn run_plexus_zero_copy(iterations: u64) -> Result<BenchReport, Box<dyn Error>> {
    let arena_size = 64 * 1024 * 1024;
    let mut backing = vec![0_u8; arena_size];
    let base = NonNull::new(backing.as_mut_ptr()).ok_or_else(|| {
        CliError("failed to obtain non-null pointer for shared arena backing".to_string())
    })?;
    // SAFETY: benchmark owns `backing` for full arena lifetime.
    let arena = unsafe { SharedArena::from_raw_parts(base, backing.len(), true)? };

    let channel = SharedChannel::<OrderUpdate>::new(4096)?;
    let (producer, consumer) = channel.split();
    // Reuse the same layout for deallocation to keep allocator pressure stable across iterations.
    let layout = Layout::new::<OrderUpdate>();

    let started = Instant::now();
    for seq in 0..iterations {
        let update = OrderUpdate::synthetic(seq);
        let handle = arena.allocate_value(update)?;

        loop {
            match producer.send(handle) {
                Ok(()) => break,
                // Busy-spin on bounded queue backpressure to avoid scheduler jitter in benchmark path.
                Err(RingError::Full) => std::hint::spin_loop(),
                Err(error) => return Err(CliError(format!("ring send failed: {error}")).into()),
            }
        }

        let received = loop {
            match consumer.try_recv() {
                Ok(handle) => break handle,
                Err(RingError::Empty) => std::hint::spin_loop(),
                Err(error) => {
                    return Err(CliError(format!("ring receive failed: {error}")).into());
                }
            }
        };

        let read = arena.read_copy(received)?;
        if read.sequence != seq {
            return Err(CliError(format!(
                "sequence mismatch in plexus mode: expected {seq}, got {}",
                read.sequence
            ))
            .into());
        }

        arena.deallocate_layout(received, layout)?;
    }

    Ok(build_report(started.elapsed(), iterations))
}

fn build_report(elapsed: std::time::Duration, iterations: u64) -> BenchReport {
    let total_secs = elapsed.as_secs_f64();
    let avg_latency_ns = elapsed.as_nanos() / u128::from(iterations);
    let throughput_msg_s = if total_secs > 0.0 {
        iterations as f64 / total_secs
    } else {
        0.0
    };
    BenchReport {
        total_secs,
        avg_latency_ns,
        throughput_msg_s,
    }
}

fn print_report(mode: Mode, iterations: u64, report: BenchReport) {
    let mode_name = match mode {
        Mode::Tcp => "TCP + JSON Baseline",
        Mode::Plexus => "Plexus Zero-Copy",
    };
    println!("Mode: {mode_name}");
    println!("Iterations: {iterations}");
    println!("Total Time: {:.6} s", report.total_secs);
    println!("Avg Latency: {} ns/msg", report.avg_latency_ns);
    println!("Throughput: {:.2} msg/s", report.throughput_msg_s);
}
