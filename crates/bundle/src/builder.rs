use base64::prelude::{BASE64_STANDARD, Engine as _};
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_hash::Hash;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_message::{VersionedMessage, v0};
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_system_interface::instruction::transfer;
use solana_transaction::versioned::VersionedTransaction;

use crate::{Error, tip_accounts::MIN_TIP_LAMPORTS};

#[derive(Debug, Clone, Copy)]
pub struct TipConfig {
    pub tip_account: Pubkey,
    pub tip_lamports: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct ComputeBudget {
    pub unit_limit: u32,
    pub unit_price_micro_lamports: u64,
}

fn assemble_instructions(
    payer: &Pubkey,
    payload: Vec<Instruction>,
    compute: ComputeBudget,
    tip: &TipConfig,
) -> Vec<Instruction> {
    let mut instructions = Vec::with_capacity(payload.len() + 3);
    instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(
        compute.unit_limit,
    ));
    instructions.push(ComputeBudgetInstruction::set_compute_unit_price(
        compute.unit_price_micro_lamports,
    ));
    instructions.extend(payload);
    instructions.push(transfer(payer, &tip.tip_account, tip.tip_lamports));
    instructions
}

pub fn build_transaction(
    payer: &Keypair,
    payload: Vec<Instruction>,
    compute: ComputeBudget,
    tip: &TipConfig,
    recent_blockhash: Hash,
) -> Result<VersionedTransaction, Error> {
    if tip.tip_lamports < MIN_TIP_LAMPORTS {
        return Err(Error::TipTooLow {
            tip: tip.tip_lamports,
            min: MIN_TIP_LAMPORTS,
        });
    }

    let instructions = assemble_instructions(&payer.pubkey(), payload, compute, tip);
    let message = v0::Message::try_compile(&payer.pubkey(), &instructions, &[], recent_blockhash)?;
    let transaction = VersionedTransaction::try_new(VersionedMessage::V0(message), &[payer])?;
    Ok(transaction)
}

pub fn encode_transaction(transaction: &VersionedTransaction) -> Result<String, Error> {
    let bytes = bincode::serialize(transaction)?;
    Ok(BASE64_STANDARD.encode(bytes))
}
