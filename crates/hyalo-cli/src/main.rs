mod alloc_metrics;

fn main() {
    alloc_metrics::enable_if_requested();
    hyalo_cli::run();
    alloc_metrics::write_report();
}
