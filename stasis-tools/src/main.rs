use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use qapi::{self, Qga, Qmp, Stream};
use std::io::BufReader;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;
use tracing::{debug, error, info, warn};

#[derive(Parser)]
#[command(author, version, about = "VM snapshot and management tool")]
struct Cli {
    /// Path to the QGA socket
    #[arg(long, default_value = "/tmp/qga.sock")]
    qga_socket: String,

    /// Path to the QMP socket
    #[arg(long, default_value = "/tmp/qemu-sock")]
    qmp_socket: String,

    /// Timeout duration in seconds
    #[arg(long, default_value_t = 30)]
    timeout: u64,

    /// The name of the snapshot
    #[arg(long, default_value = "vm_snapshot_latest")]
    snapshot_name: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Take a snapshot of the VM
    Snapshot,

    /// Unfreeze a VM that has been loaded
    Unfreeze {
        /// Wait for guest agent to become available before unfreezing
        #[arg(long)]
        wait: bool,

        /// Maximum time to wait for guest agent in seconds (only used with --wait)
        #[arg(long, default_value_t = 60)]
        max_wait: u64,

        /// Polling interval in seconds when waiting for guest agent
        #[arg(long, default_value_t = 2)]
        poll_interval: u64,
    },
}

fn main() -> Result<()> {
    // Initialize tracing subscriber
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // Configuration from CLI arguments
    let qga_socket_path = cli.qga_socket;
    let qmp_socket_path = cli.qmp_socket;
    let snapshot_name = cli.snapshot_name;
    let timeout_duration = Duration::from_secs(cli.timeout);

    match cli.command {
        Commands::Snapshot => {
            // Verify socket paths exist for snapshot command
            if !Path::new(&qga_socket_path).exists() {
                error!("QGA socket {} does not exist", qga_socket_path);
                anyhow::bail!("QGA socket {} does not exist", qga_socket_path);
            }

            if !Path::new(&qmp_socket_path).exists() {
                error!("QMP socket {} does not exist", qmp_socket_path);
                anyhow::bail!("QMP socket {} does not exist", qmp_socket_path);
            }

            take_snapshot(
                &qga_socket_path,
                &qmp_socket_path,
                &snapshot_name,
                timeout_duration,
            )
        }
        Commands::Unfreeze {
            wait,
            max_wait,
            poll_interval,
        } => {
            if wait {
                wait_and_unfreeze_vm(&qga_socket_path, timeout_duration, max_wait, poll_interval)
            } else {
                // Only verify QGA socket for unfreeze command when not waiting
                if !Path::new(&qga_socket_path).exists() {
                    error!("QGA socket {} does not exist", qga_socket_path);
                    anyhow::bail!("QGA socket {} does not exist", qga_socket_path);
                }

                unfreeze_vm(&qga_socket_path, timeout_duration)
            }
        }
    }
}

fn take_snapshot(
    qga_socket_path: &str,
    qmp_socket_path: &str,
    snapshot_name: &str,
    timeout_duration: Duration,
) -> Result<()> {
    // Connect to QMP socket with standard library's synchronous UnixStream
    info!("Connecting to QMP socket...");
    let qmp_stream =
        UnixStream::connect(qmp_socket_path).context("Failed to connect to QMP socket")?;

    // Set socket timeouts
    qmp_stream
        .set_read_timeout(Some(timeout_duration))
        .context("Failed to set read timeout")?;
    qmp_stream
        .set_write_timeout(Some(timeout_duration))
        .context("Failed to set write timeout")?;

    // Create QMP client
    let mut qmp = Qmp::from_stream(&qmp_stream);

    // Negotiate capabilities
    info!("Negotiating QMP capabilities...");
    qmp.read_capabilities()
        .context("Failed to negotiate QMP capabilities")?;

    // Connect to QGA socket
    info!("Connecting to QGA socket...");
    let qga_stream =
        UnixStream::connect(qga_socket_path).context("Failed to connect to QGA socket")?;

    // Set socket timeouts
    qga_stream
        .set_read_timeout(Some(timeout_duration))
        .context("Failed to set read timeout")?;
    qga_stream
        .set_write_timeout(Some(timeout_duration))
        .context("Failed to set write timeout")?;

    // Create QGA client
    let mut qga = Qga::from_stream(&qga_stream);

    // Freeze the filesystem
    info!("Freezing filesystem...");
    let freeze_result = qga
        .execute(&qapi::qga::guest_fsfreeze_freeze {})
        .context("Failed to freeze filesystem")?;
    info!(
        frozen_count = freeze_result,
        "Filesystem frozen successfully"
    );

    // Take snapshot using human-monitor-command (HMP passthrough)
    info!(snapshot = snapshot_name, "Taking snapshot");
    qmp.execute(&qapi::qmp::qmp_capabilities {
        enable: Some(vec![]),
    })
    .context("Failed to read capabilities")?;
    let snapshot_result = qmp.execute(&qapi::qmp::human_monitor_command {
        cpu_index: None,
        command_line: format!("savevm {}", snapshot_name),
    });

    // Always thaw filesystem, even if snapshot fails
    info!("Thawing filesystem...");
    let thaw_result = qga.execute(&qapi::qga::guest_fsfreeze_thaw {});

    // Check snapshot result
    let snapshot_response = snapshot_result.context("Failed to take snapshot")?;
    // For human-monitor-command, check if there's any error message in the output
    if !snapshot_response.is_empty() {
        // HMP commands that succeed often return empty strings
        // If we got something back, it might be an error message
        error!(response = snapshot_response, "Snapshot might have failed");
        anyhow::bail!("Snapshot might have failed: {}", snapshot_response);
    }
    info!("Snapshot taken successfully");

    // Check thaw result
    let thawed_count = thaw_result.context("Failed to thaw filesystem")?;
    info!(
        thawed_count = thawed_count,
        "Filesystem thawed successfully"
    );

    // Shutdown the VM
    info!("Shutting down VM...");
    qmp.execute(&qapi::qmp::system_powerdown {})
        .context("Failed to shut down VM")?;
    info!("VM shutdown initiated successfully");

    info!("All operations completed successfully");
    Ok(())
}

fn unfreeze_vm(qga_socket_path: &str, timeout_duration: Duration) -> Result<()> {
    // Connect to QGA socket
    info!("Connecting to QGA socket...");
    let qga_stream =
        UnixStream::connect(qga_socket_path).context("Failed to connect to QGA socket")?;

    // Set socket timeouts
    qga_stream
        .set_read_timeout(Some(timeout_duration))
        .context("Failed to set read timeout")?;
    qga_stream
        .set_write_timeout(Some(timeout_duration))
        .context("Failed to set write timeout")?;

    // Create QGA client
    let qga = Qga::from_stream(&qga_stream);

    unfreeze_with_qga(qga)
}

fn wait_and_unfreeze_vm(
    qga_socket_path: &str,
    timeout_duration: Duration,
    max_wait: u64,
    poll_interval: u64,
) -> Result<()> {
    use std::thread;
    use std::time::Instant;

    info!(
        max_wait_seconds = max_wait,
        "Waiting for guest agent to become available"
    );

    let start_time = Instant::now();
    let max_wait_duration = Duration::from_secs(max_wait);
    let poll_interval_duration = Duration::from_secs(poll_interval);

    while start_time.elapsed() < max_wait_duration {
        // Check if the socket exists
        if Path::new(qga_socket_path).exists() {
            // Try to connect to the socket
            match UnixStream::connect(qga_socket_path) {
                Ok(stream) => {
                    info!(
                        elapsed_seconds = start_time.elapsed().as_secs(),
                        "Guest agent is available"
                    );

                    // Set socket timeouts
                    if let Err(e) = stream.set_read_timeout(Some(timeout_duration)) {
                        warn!(error = %e, "Failed to set read timeout");
                    }
                    if let Err(e) = stream.set_write_timeout(Some(timeout_duration)) {
                        warn!(error = %e, "Failed to set write timeout");
                    }

                    // Create QGA client and try to ping
                    let mut qga = Qga::from_stream(&stream);

                    // Try to ping the agent to make sure it's fully operational
                    match qga.execute(&qapi::qga::guest_ping {}) {
                        Ok(_) => {
                            info!("Guest agent responded to ping, proceeding with unfreeze");
                            // Thaw the filesystem
                            return unfreeze_with_qga(qga);
                        }
                        Err(e) => {
                            debug!(
                                error = %e,
                                "Guest agent socket exists but ping failed, will retry"
                            );
                        }
                    }
                }
                Err(e) => {
                    debug!(
                        error = %e,
                        "Socket exists but connection failed, will retry"
                    );
                }
            }
        } else {
            debug!(
                socket_path = qga_socket_path,
                elapsed_seconds = start_time.elapsed().as_secs(),
                max_wait_seconds = max_wait,
                "Waiting for QGA socket to appear"
            );
        }

        // Sleep before retrying
        thread::sleep(poll_interval_duration);
    }

    error!(
        max_wait_seconds = max_wait,
        "Timed out waiting for guest agent"
    );
    anyhow::bail!(
        "Timed out after {} seconds waiting for guest agent",
        max_wait
    )
}

fn unfreeze_with_qga(mut qga: Qga<Stream<BufReader<&UnixStream>, &UnixStream>>) -> Result<()> {
    // Thaw the filesystem
    info!("Thawing filesystem...");
    let thawed_count = qga
        .execute(&qapi::qga::guest_fsfreeze_thaw {})
        .context("Failed to thaw filesystem")?;

    info!(
        thawed_count = thawed_count,
        "Filesystem thawed successfully"
    );

    Ok(())
}
