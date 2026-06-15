use std::fs;
use std::path::Path;
use std::time::Duration;

use agent::{AnthropicClient, ReasoningLog, RetryAgent};
use anyhow::{Context, Result};
use bundle::{BundleSubmitter, MIN_TIP_LAMPORTS};
use lifecycle::{LifecycleEntry, LifecycleLogger};
use solana_keypair::Keypair;
use solana_signer::Signer;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::banner::{accent, bad, dim, good};
use crate::config::{Config, load_keypair};
use crate::pipeline::{Attempt, Stack};

const LOG_DIR: &str = "logs";
const REASONING_LOG: &str = "logs/agent-reasoning.jsonl";
const TIP_REFRESH: Duration = Duration::from_secs(10);

pub async fn run(config: Config, count: u32, tip: Option<u64>) -> Result<()> {
    let payer = load_keypair()?;
    println!("{}", dim(&format!("payer: {}", payer.pubkey())));

    let cancel = CancellationToken::new();
    let stack = Stack::launch(config, cancel.clone());
    let logger = LifecycleLogger::new(LOG_DIR);

    let outcome = async {
        for index in 1..=count {
            println!("\n{}", accent(&format!("submission {index}/{count}")));
            stack.log_leader_window().await;

            let suggestion = stack.tip_suggestion().await?;
            let tip_lamports = tip
                .unwrap_or_else(|| suggestion.baseline_tip_lamports())
                .max(MIN_TIP_LAMPORTS);
            let prio_price = suggestion.prio_fees.percentiles.p50;
            let blockhash = stack.await_blockhash().await?;

            let entry = stack
                .submit_attempt(
                    Attempt {
                        payer: &payer,
                        blockhash: blockhash.blockhash,
                        tip_lamports,
                        prio_price,
                        landing_deadline: lifecycle::DEFAULT_LANDING_DEADLINE,
                        label: "run",
                    },
                    &cancel,
                )
                .await?;

            let path = logger.write(&entry).context("writing lifecycle log")?;
            print_entry_summary(&entry);
            println!("{}", dim(&format!("  logged → {}", path.display())));
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;

    stack.shutdown().await;
    outcome
}

pub async fn inject(config: Config) -> Result<()> {
    let payer = load_keypair()?;
    println!("{}", dim(&format!("payer: {}", payer.pubkey())));

    let model = config.model.clone();
    let cancel = CancellationToken::new();
    let stack = Stack::launch(config, cancel.clone());

    let outcome = async {
        let agent = build_agent(model)?;
        let logger = LifecycleLogger::new(LOG_DIR);
        stack.inject_demo(&payer, &agent, &logger, &cancel).await
    }
    .await;

    stack.shutdown().await;
    outcome
}

pub async fn watch(config: Config) -> Result<()> {
    let cancel = CancellationToken::new();
    let stack = Stack::launch(config, cancel.clone());
    println!(
        "{}",
        dim("watching live chain state — press Ctrl-C to stop\n")
    );

    let outcome = watch_loop(&stack).await;
    stack.shutdown().await;
    outcome
}

async fn watch_loop(stack: &Stack) -> Result<()> {
    let mut slot_tip = stack.state().subscribe_slot_tip();
    let mut last_refresh: Option<Instant> = None;
    let mut tip_line = String::from("tips: (loading)");

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\n{}", dim("stopped."));
                return Ok(());
            }
            changed = slot_tip.changed() => {
                if changed.is_err() {
                    return Ok(());
                }
                let due = last_refresh.is_none_or(|t| t.elapsed() >= TIP_REFRESH);
                if due {
                    if let Ok(s) = stack.tip_suggestion().await {
                        tip_line = format!(
                            "tip floor p50/p75/p95 = {}/{}/{} lamports ({:?})",
                            s.jito_tip_floor.p50, s.jito_tip_floor.p75, s.jito_tip_floor.p95, s.congestion.level
                        );
                    }
                    last_refresh = Some(Instant::now());
                }
                let state = stack.state();
                let leader = stack
                    .current_leader()
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "unknown".to_owned());
                println!(
                    "slot {} (confirmed {}, finalized {}) | leader {} | {}",
                    state.processed_slot(),
                    state.confirmed_slot(),
                    state.finalized_slot(),
                    dim(&leader),
                    tip_line
                );
            }
        }
    }
}

pub fn logs(dir: &str) -> Result<()> {
    let path = Path::new(dir);
    if !path.is_dir() {
        println!("no logs directory at {dir}");
        return Ok(());
    }

    let mut files: Vec<_> = fs::read_dir(path)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("lifecycle-run-") && n.ends_with(".json"))
        })
        .collect();
    files.sort();

    if files.is_empty() {
        println!("no lifecycle logs in {dir}");
        return Ok(());
    }

    let mut landed = 0usize;
    for file in &files {
        let body = fs::read_to_string(file)?;
        let entry: LifecycleEntry =
            serde_json::from_str(&body).with_context(|| format!("parsing {}", file.display()))?;
        let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        if entry.landed_slot.is_some() {
            landed += 1;
        }
        print!("{}  ", dim(name));
        print_entry_summary(&entry);
    }
    println!(
        "\n{}",
        accent(&format!(
            "{} runs — {} landed, {} failed",
            files.len(),
            landed,
            files.len() - landed
        ))
    );
    Ok(())
}

pub async fn status(config: Config, bundle_id: &str) -> Result<()> {
    let submitter = BundleSubmitter::new(&config.block_engine);
    let status = submitter
        .status(bundle_id)
        .await
        .context("querying bundle status")?;
    println!("bundle {bundle_id}: {status:?}");
    Ok(())
}

fn build_agent(model: Option<String>) -> Result<RetryAgent> {
    let mut client = AnthropicClient::from_env()
        .context("initialising the Claude client from the inherited session")?;
    if let Some(model) = model {
        client = client.with_model(model);
    }
    info!(model = client.model(), "agent ready");
    Ok(RetryAgent::new(client).with_log(ReasoningLog::new(REASONING_LOG)))
}

fn print_entry_summary(entry: &LifecycleEntry) {
    let stage = format!("{:?}", entry.stage());
    match entry.landed_slot {
        Some(slot) => {
            let mut parts = vec![format!("slot {slot}")];
            if let Some(ms) = entry.submitted_to_processed_ms() {
                parts.push(format!("submit→processed {ms}ms"));
            }
            if let Some(ms) = entry.processed_to_confirmed_ms() {
                parts.push(format!("processed→confirmed {ms}ms"));
            }
            if let Some(ms) = entry.confirmed_to_finalized_ms() {
                parts.push(format!("confirmed→finalized {ms}ms"));
            }
            println!("{} — {}", good(&stage), parts.join(", "));
        }
        None => {
            let reason = entry.failure.as_deref().unwrap_or("never landed");
            println!("{} — {}", bad(&stage), reason);
        }
    }
}

pub fn keygen(outfile: &str, force: bool) -> Result<()> {
    let path = Path::new(outfile);
    if path.exists() && !force {
        anyhow::bail!("{outfile} already exists; pass --force to overwrite");
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).with_context(|| format!("creating directory for {outfile}"))?;
    }

    let keypair = Keypair::new();
    let json = serde_json::to_string(&keypair.to_bytes().to_vec())
        .context("serializing the new keypair")?;
    fs::write(path, json).with_context(|| format!("writing keypair to {outfile}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }

    eprintln!("{}", good(&format!("wrote keypair to {outfile}")));
    eprintln!(
        "{}",
        dim("fund this address with ~0.1 SOL before running `copilot run`")
    );
    println!("{}", keypair.pubkey());
    Ok(())
}
