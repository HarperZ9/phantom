use phantom_cli::json_out::{Envelope, LayerStatus, StatusPayload};
use phantom_cli::{apply, config};

pub fn run(json: bool) {
    let statuses = apply::status();
    let cfg = config::resolved();

    if json {
        let layers = statuses
            .iter()
            .map(|(layer, status)| LayerStatus {
                layer: match layer {
                    apply::Layer::Firmware => 0,
                    apply::Layer::Kernel => 1,
                    apply::Layer::Userland => 2,
                },
                name: layer.name(),
                status: status.to_string(),
            })
            .collect();
        Envelope::ok(
            "status",
            StatusPayload {
                layers,
                data_dir: cfg.data_dir.display().to_string(),
                pipe_name: cfg.pipe_name.clone(),
            },
        )
        .print();
    } else {
        println!("\n  Phantom Status\n  {}\n", "=".repeat(50));
        for (layer, status) in &statuses {
            println!("  {} : {}", layer.name(), status);
        }
        println!();
    }
}
