use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};

fn run(label: &str, mut cmd: Command) -> bool {
    eprintln!("\n=== {label} ===");
    match cmd.status() {
        Ok(status) if status.success() => {
            eprintln!("✓ {label}");
            true
        }
        Ok(status) => {
            eprintln!("✗ {label}: exited with {status}");
            false
        }
        Err(err) => {
            eprintln!("✗ {label}: {err}");
            false
        }
    }
}

fn cargo_test(args: &[&str]) -> Command {
    let mut c = Command::new("cargo");
    c.arg("test").args(args);
    c
}

fn cargo_check(args: &[&str]) -> Command {
    let mut c = Command::new("cargo");
    c.arg("check").args(args);
    c
}

fn cargo_fmt(args: &[&str]) -> Command {
    let mut c = Command::new("cargo");
    c.arg("fmt").args(args);
    c
}

fn cargo_clippy(args: &[&str]) -> Command {
    let mut c = Command::new("cargo");
    c.arg("clippy").args(args);
    c
}

fn build_canister(package: &str, profile: &str) -> Command {
    let mut c = Command::new("tools/scripts/build-canister");
    c.arg(package).arg(profile);
    c
}

fn icp(args: &[&str]) -> Command {
    let mut c = Command::new("icp");
    c.args(args);
    c
}

fn run_subcommand(sub: &str) -> bool {
    let exe = env::current_exe().expect("current exe");
    let mut c = Command::new(exe);
    c.arg(sub);
    run(sub, c)
}

fn read_file(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|err| format!("{path}: {err}"))
}

fn require_absent(path: &str, text: &str, needles: &[&str]) -> Result<(), String> {
    for needle in needles {
        if text.contains(needle) {
            return Err(format!("{path} must not contain {needle:?}"));
        }
    }
    Ok(())
}

fn require_present(path: &str, text: &str, needles: &[&str]) -> Result<(), String> {
    for needle in needles {
        if !text.contains(needle) {
            return Err(format!("{path} must contain {needle:?}"));
        }
    }
    Ok(())
}

fn check_did_surface() -> Result<(), String> {
    let stream_production_path = "canisters/io_stream_manager/io_stream_manager.did";
    let stream_debug_path = "canisters/io_stream_manager/io_stream_manager_debug.did";
    let nns_production_path = "canisters/io_nns_neuron_manager/io_nns_neuron_manager.did";
    let nns_debug_path = "canisters/io_nns_neuron_manager/io_nns_neuron_manager_debug.did";

    let stream_production = read_file(stream_production_path)?;
    let stream_debug = read_file(stream_debug_path)?;
    let nns_production = read_file(nns_production_path)?;
    let nns_debug = read_file(nns_debug_path)?;

    require_absent(
        stream_production_path,
        &stream_production,
        &[
            " get_state :",
            " get_config :",
            " get_redemption_rate :",
            " process_stream_event :",
            " redeem :",
            " debug_tick :",
            " plan_rebalance :",
            " advance_model_time :",
            "debug_",
            " get_events :",
        ],
    )?;
    require_absent(
        nns_production_path,
        &nns_production,
        &[
            " get_state :",
            " get_config :",
            " plan_rebalance :",
            " advance_model_time :",
            " debug_tick :",
            "debug_",
            " get_events :",
        ],
    )?;

    require_present(
        stream_debug_path,
        &stream_debug,
        &[
            "debug_get_state",
            "debug_get_redemption_rate",
            "debug_process_stream_event",
            "debug_redeem",
            "debug_tick",
        ],
    )?;
    require_present(
        nns_debug_path,
        &nns_debug,
        &[
            "debug_get_config",
            "debug_get_state",
            "debug_plan_rebalance",
            "debug_advance_model_time",
            "debug_tick",
        ],
    )?;

    require_absent(
        stream_debug_path,
        &stream_debug,
        &[
            "  get_state :",
            "  get_redemption_rate :",
            "  process_stream_event :",
            "  redeem :",
        ],
    )?;
    require_absent(
        nns_debug_path,
        &nns_debug,
        &[
            "  get_config :",
            "  get_state :",
            "  plan_rebalance :",
            "  advance_model_time :",
        ],
    )?;

    Ok(())
}

fn check_artifacts(paths: &[&str]) -> Result<(), String> {
    for path in paths {
        if !Path::new(path).is_file() {
            return Err(format!("missing artifact {path}"));
        }
    }
    Ok(())
}

fn expected_release_artifacts() -> Vec<String> {
    [
        "io_stream_manager",
        "io_nns_neuron_manager",
        "io_historian",
        "io_frontend",
    ]
    .into_iter()
    .flat_map(|name| {
        [
            format!("release-artifacts/{name}.wasm"),
            format!("release-artifacts/{name}.wasm.gz"),
            format!("release-artifacts/{name}.wasm.sha256"),
            format!("release-artifacts/{name}.wasm.gz.sha256"),
        ]
    })
    .collect()
}

fn verify_artifact_hash(path: &str) -> Result<(), String> {
    let output = Command::new("sha256sum")
        .arg("-c")
        .arg(path)
        .output()
        .map_err(|err| format!("{path}: failed to run sha256sum -c: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{path}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn verify_artifacts() -> Result<(), String> {
    let artifacts = expected_release_artifacts();
    let refs = artifacts.iter().map(String::as_str).collect::<Vec<_>>();
    check_artifacts(&refs)?;
    for sha in artifacts
        .iter()
        .filter(|path| path.ends_with(".sha256"))
        .map(String::as_str)
    {
        verify_artifact_hash(sha)?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let cmd = env::args().nth(1).unwrap_or_else(|| "test_all".to_string());
    let mut ok = true;
    match cmd.as_str() {
        "check" => {
            ok &= run(
                "check: workspace all targets",
                cargo_check(&["--workspace", "--all-targets"]),
            );
        }
        "fmt_check" => {
            ok &= run("fmt: workspace", cargo_fmt(&["--all", "--", "--check"]));
        }
        "did_surface" => match check_did_surface() {
            Ok(()) => eprintln!("✓ did_surface"),
            Err(err) => {
                eprintln!("✗ did_surface: {err}");
                ok = false;
            }
        },
        "build_canisters" => {
            for package in [
                "io-stream-manager",
                "io-nns-neuron-manager",
                "io-historian",
                "io-frontend",
            ] {
                ok &= run(
                    &format!("build canister: {package}"),
                    build_canister(package, "release"),
                );
            }
            match verify_artifacts() {
                Ok(()) => eprintln!("✓ build_canisters artifacts"),
                Err(err) => {
                    eprintln!("✗ build_canisters artifacts: {err}");
                    ok = false;
                }
            }
        }
        "verify_artifacts" => match verify_artifacts() {
            Ok(()) => eprintln!("✓ verify_artifacts"),
            Err(err) => {
                eprintln!("✗ verify_artifacts: {err}");
                ok = false;
            }
        },
        "build_debug_canisters" => {
            for package in [
                "io-stream-manager",
                "io-nns-neuron-manager",
                "io-historian",
                "io-frontend",
                "mock-icp-ledger",
                "mock-io-ledger",
                "mock-icp-index",
                "mock-io-index",
                "mock-nns-governance",
                "mock-sns-governance",
                "mock-jupiter-faucet",
            ] {
                ok &= run(
                    &format!("build debug canister: {package}"),
                    build_canister(package, "debug"),
                );
            }
        }
        "preflight" => {
            ok &= run_subcommand("check");
            ok &= run_subcommand("did_surface");
        }
        "test_unit" => {
            ok &= run("unit: io-core-model", cargo_test(&["-p", "io-core-model"]));
            ok &= run(
                "unit: io-reward-policy",
                cargo_test(&["-p", "io-reward-policy"]),
            );
            ok &= run(
                "unit: io-stream-manager",
                cargo_test(&["-p", "io-stream-manager", "--lib"]),
            );
            ok &= run(
                "unit: io-nns-neuron-manager",
                cargo_test(&["-p", "io-nns-neuron-manager", "--lib"]),
            );
            ok &= run(
                "unit: placeholders",
                cargo_test(&["-p", "io-historian", "-p", "io-frontend"]),
            );
        }
        "test_pocketic_integration" => {
            ok &= run_subcommand("build_debug_canisters");
            ok &= run(
                "pocketic: io-stream-manager",
                cargo_test(&[
                    "-p",
                    "io-stream-manager",
                    "--test",
                    "io_stream_manager_pocketic",
                ]),
            );
            ok &= run(
                "pocketic: io-nns-neuron-manager",
                cargo_test(&[
                    "-p",
                    "io-nns-neuron-manager",
                    "--test",
                    "io_nns_neuron_manager_pocketic",
                ]),
            );
        }
        "test_pocketic_required" => {
            if env::var_os("POCKET_IC_BIN").is_none() {
                eprintln!("✗ test_pocketic_required: POCKET_IC_BIN is not set");
                ok = false;
            } else {
                ok &= run_subcommand("test_pocketic_integration");
            }
        }
        "test_local_integration" => {
            ok &= run_subcommand("build_canisters");
            ok &= run_subcommand("did_surface");
            ok &= run("local-cli: icp project show", icp(&["project", "show"]));
            ok &= run("local-cli: icp build", icp(&["build"]));
            ok &= run(
                "local-cli: io-stream-manager",
                cargo_test(&["-p", "io-stream-manager", "--test", "io_stream_manager_cli"]),
            );
            ok &= run(
                "local-cli: io-nns-neuron-manager",
                cargo_test(&[
                    "-p",
                    "io-nns-neuron-manager",
                    "--test",
                    "io_nns_neuron_manager_cli",
                ]),
            );
        }
        "test_e2e" => {
            ok &= run(
                "e2e: io suite model",
                cargo_test(&["-p", "io-stream-manager", "--test", "io_e2e"]),
            );
        }
        "stream_manager_unit" => {
            ok &= run(
                "unit: io-stream-manager",
                cargo_test(&["-p", "io-stream-manager", "--lib"]),
            )
        }
        "nns_neuron_manager_unit" => {
            ok &= run(
                "unit: io-nns-neuron-manager",
                cargo_test(&["-p", "io-nns-neuron-manager", "--lib"]),
            )
        }
        "stream_manager_pocketic_integration" => {
            ok &= run(
                "pocketic: io-stream-manager",
                cargo_test(&[
                    "-p",
                    "io-stream-manager",
                    "--test",
                    "io_stream_manager_pocketic",
                ]),
            )
        }
        "nns_neuron_manager_pocketic_integration" => {
            ok &= run(
                "pocketic: io-nns-neuron-manager",
                cargo_test(&[
                    "-p",
                    "io-nns-neuron-manager",
                    "--test",
                    "io_nns_neuron_manager_pocketic",
                ]),
            )
        }
        "test_all" => {
            for sub in [
                "preflight",
                "test_unit",
                "test_pocketic_integration",
                "test_local_integration",
                "test_e2e",
            ] {
                ok &= run_subcommand(sub);
            }
        }
        "test_ci" => {
            for sub in [
                "fmt_check",
                "check",
                "did_surface",
                "build_canisters",
                "verify_artifacts",
                "test_unit",
                "test_pocketic_required",
                "test_local_integration",
                "test_e2e",
            ] {
                ok &= run_subcommand(sub);
            }
            ok &= run(
                "clippy: workspace all targets",
                cargo_clippy(&["--workspace", "--all-targets", "--", "-D", "warnings"]),
            );
        }
        other => {
            eprintln!("unknown xtask command: {other}");
            eprintln!("known: test_all, test_ci, test_pocketic_required, preflight, check, fmt_check, did_surface, build_canisters, verify_artifacts, build_debug_canisters, test_unit, test_pocketic_integration, test_local_integration, test_e2e, stream_manager_unit, nns_neuron_manager_unit, stream_manager_pocketic_integration, nns_neuron_manager_pocketic_integration");
            return ExitCode::from(2);
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
