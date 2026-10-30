//! Print the WASP conformance status for every manifest case.
//!
//! Usage:
//!   cargo run --example wasp_report
//!   cargo run --example wasp_report -- --markdown
//!   cargo run --example wasp_report -- --fail-on-divergence

#[path = "../tests/wasp/support/mod.rs"]
mod support;

use std::collections::BTreeMap;
use support::Outcome;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let markdown = args.iter().any(|arg| arg == "--markdown");
    let tsv = args.iter().any(|arg| arg == "--tsv");
    let fail_on_divergence = args.iter().any(|arg| arg == "--fail-on-divergence");

    let reports = support::report();
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for report in &reports {
        *counts.entry(report.outcome.label()).or_default() += 1;
    }

    if tsv {
        for report in &reports {
            println!(
                "{}\t{}\t{}\t{}",
                report.id,
                report.status,
                report.outcome.label(),
                report.detail.as_deref().unwrap_or("")
            );
        }
        return;
    }

    if markdown {
        println!("| Case | Category | Phase | Status | Result |");
        println!("|---|---|---|---|---|");
        for report in &reports {
            println!(
                "| `{}` | {} | {} | {} | {} |",
                report.id,
                report.category,
                report.phase,
                report.status,
                report.outcome.label()
            );
        }
        println!();
        println!("| Result | Count |");
        println!("|---|---:|");
        for (label, count) in &counts {
            println!("| {label} | {count} |");
        }
    } else {
        for report in &reports {
            println!(
                "{:<12} {:<45} {:<18} {}",
                report.outcome.label(),
                report.id,
                report.category,
                report.status
            );
            if let Some(detail) = &report.detail {
                if report.outcome != Outcome::KnownDivergence {
                    println!("             {detail}");
                }
            }
        }
        println!();
        for (label, count) in &counts {
            println!("{label}: {count}");
        }
    }

    let mismatches = reports
        .iter()
        .filter(|report| report.outcome == Outcome::Mismatch)
        .count();
    if mismatches > 0 {
        eprintln!("{mismatches} case(s) marked MATCH diverged from the oracle");
        std::process::exit(2);
    }
    if fail_on_divergence {
        let divergences = reports
            .iter()
            .filter(|report| {
                matches!(
                    report.outcome,
                    Outcome::KnownDivergence | Outcome::Unsupported
                )
            })
            .count();
        if divergences > 0 {
            eprintln!("{divergences} case(s) are not yet conformant");
            std::process::exit(1);
        }
    }
}
