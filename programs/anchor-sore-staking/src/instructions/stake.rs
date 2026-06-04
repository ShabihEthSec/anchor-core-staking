use anchor_lang::prelude::*;
use mpl_core::{
    accounts::{BaseAssetV1, BaseCollectionV1},
    instructions::{AddPluginV1CpiBuilder, UpdatePluginV1CpiBuilder},
    types::{Attribute, Attributes, FreezeDelegate, Plugin, PluginAuthority, UpdateAuthority},
    ID as MPL_CORE_ID,
};

use super::utils::{
    fetch_asset_attributes, update_collection_staked_count, CLAIMED_AT_ATTRIBUTE, STAKED_ATTRIBUTE,
    STAKED_AT_ATTRIBUTE,
};
use crate::error::ErrorCode;
use crate::state::Config;

#[derive(Accounts)]
pub struct Stake<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        seeds = [b"config", collection.key().as_ref()],
        bump = config.bump,
    )]
    pub config: Account<'info, Config>,

    #[account(
        mut,
        has_one = owner @ ErrorCode::InvalidOwner,
        constraint = asset.update_authority
            == UpdateAuthority::Collection(collection.key())
            @ ErrorCode::InvalidUpdateAuthority,
    )]
    pub asset: Account<'info, BaseAssetV1>,

    #[account(
        mut,
        has_one = update_authority @ ErrorCode::InvalidUpdateAuthority,
    )]
    pub collection: Account<'info, BaseCollectionV1>,

    /// CHECK: This account is not initialized and is being used for signing purposes only,
    /// we verify that derives from the correct seeds
    #[account(
        seeds = [b"update_authority", collection.key().as_ref()],
        bump,
    )]
    pub update_authority: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,

    /// CHECK: This is the ID of the MPL Core Program
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<Stake>) -> Result<()> {
    // We start by fetching the existing attributes (if they exist)
    let attributes_fetched: Option<Attributes> =
        fetch_asset_attributes(&ctx.accounts.asset.to_account_info());

    // Prepare the Attributes list to add or update based on the existing attributes
    let mut attributes_list: Vec<Attribute> = Vec::new();

    // Loop through all attributes and keep only those that are not staking attributes
    if let Some(attributes) = &attributes_fetched {
        for attribute in &attributes.attribute_list {
            if attribute.key == STAKED_ATTRIBUTE {
                require!(attribute.value == "false", ErrorCode::AlreadyStaked);
            } else if attribute.key != STAKED_AT_ATTRIBUTE && attribute.key != CLAIMED_AT_ATTRIBUTE
            {
                attributes_list.push(attribute.clone());
            }
        }
    }

    let current_timestamp = Clock::get()?.unix_timestamp;

    // Add staking attributes
    attributes_list.push(Attribute {
        key: STAKED_ATTRIBUTE.to_string(),
        value: "true".to_string(),
    });

    attributes_list.push(Attribute {
        key: STAKED_AT_ATTRIBUTE.to_string(),
        value: current_timestamp.to_string(),
    });

    attributes_list.push(Attribute {
        key: CLAIMED_AT_ATTRIBUTE.to_string(),
        value: current_timestamp.to_string(),
    });

    // Prepare signing seeds for update authority PDA
    let collection_key: Pubkey = ctx.accounts.collection.key();

    let signer_seeds: &[&[u8]; 3] = &[
        b"update_authority",
        collection_key.as_ref(),
        &[ctx.bumps.update_authority],
    ];

    // If the Attributes Plugin does not exist, add it
    if attributes_fetched.is_none() {
        AddPluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
            .asset(&ctx.accounts.asset.to_account_info())
            .collection(Some(&ctx.accounts.collection.to_account_info()))
            .payer(&ctx.accounts.owner.to_account_info())
            .authority(Some(&ctx.accounts.update_authority.to_account_info()))
            .system_program(&ctx.accounts.system_program.to_account_info())
            .plugin(Plugin::Attributes(Attributes {
                attribute_list: attributes_list,
            }))
            .init_authority(PluginAuthority::UpdateAuthority)
            .invoke_signed(&[signer_seeds])?;
    } else {
        // If the Attributes Plugin exists, update it
        UpdatePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
            .asset(&ctx.accounts.asset.to_account_info())
            .collection(Some(&ctx.accounts.collection.to_account_info()))
            .payer(&ctx.accounts.owner.to_account_info())
            .authority(Some(&ctx.accounts.update_authority.to_account_info()))
            .system_program(&ctx.accounts.system_program.to_account_info())
            .plugin(Plugin::Attributes(Attributes {
                attribute_list: attributes_list,
            }))
            .invoke_signed(&[signer_seeds])?;
    }

    update_collection_staked_count(
        ctx.accounts.collection.to_account_info(),
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.update_authority.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.mpl_core_program.to_account_info(),
        &[signer_seeds],
        1,
    )?;

    // Freeze the asset with the FreezeDelegate Plugin
    // Owner-managed plugin, signed by owner
    AddPluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .payer(&ctx.accounts.owner.to_account_info())
        .authority(Some(&ctx.accounts.owner.to_account_info()))
        .system_program(&ctx.accounts.system_program.to_account_info())
        .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen: true }))
        .init_authority(PluginAuthority::UpdateAuthority)
        .invoke()?;

    Ok(())
}
