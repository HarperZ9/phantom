use crate::validator::report;
use crate::validator::sources;

pub fn run_audit() {
    println!("\n  Reading hardware identifiers...\n");
    let results = sources::read_all_sources();
    report::print_audit_report(&results);
}
