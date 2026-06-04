use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{mint_to_checked, Mint, MintToChecked, TokenAccount, TokenInterface},
};

use mpl_core::{
    accounts::{BaseAssetV1, BaseCollectionV1},
    instructions::UpdatePluginV1CpiBuilder,
    types::{Attribute, Attributes, FreezeDelegate, Plugin, UpdateAuthority},
    ID as MPL_CORE_ID,
};

use super::utils::{
    fetch_asset_attributes, parse_i64_attribute, reward_amount, reward_days,
    reward_start_timestamp, update_collection_staked_count, CLAIMED_AT_ATTRIBUTE, STAKED_ATTRIBUTE,
    STAKED_AT_ATTRIBUTE,
};
use crate::error::ErrorCode;
use crate::Config;

#[derive(Accounts)]
pub struct Unstake<'info> {
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
        has_one = update_authority @ ErrorCode::InvalidUpdateAuthority
    )]
    pub collection: Account<'info, BaseCollectionV1>,

    /// CHECK: This account data is not used, we only verify the address
    #[account(
        seeds = [b"update_authority", collection.key().as_ref()],
        bump,
    )]
    pub update_authority: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [b"rewards_mint", config.key().as_ref()],
        bump = config.rewards_bump,
    )]
    pub rewards_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init_if_needed,
        payer = owner,
        associated_token::mint = rewards_mint,
        associated_token::authority = owner,
    )]
    pub user_rewards_ata: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,

    /// CHECK: This is the MPL Core program
    #[account(address = Pubkey::from(MPL_CORE_ID.to_bytes()))]
    pub mpl_core_program: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<Unstake>) -> Result<()> {
    // We start by fetching the existing attributes
    let attributes_fetched: Option<Attributes> =
        fetch_asset_attributes(&ctx.accounts.asset.to_account_info());

    // If the attributes don't exist, we return an error
    require!(attributes_fetched.is_some(), ErrorCode::AssetNotStaked);

    let attributes: Attributes = attributes_fetched.unwrap();

    // Prepare the Attributes list to update based on the existing attributes
    let mut attributes_list: Vec<Attribute> = Vec::with_capacity(attributes.attribute_list.len());

    // Additional auxiliary variables
    let current_timestamp: i64 = Clock::get()?.unix_timestamp;
    let mut staked_timestamp: i64 = 0;
    let mut claimed_timestamp: i64 = 0;
    let mut found_staked = false;
    let mut found_staked_at = false;

    for attribute in &attributes.attribute_list {
        if attribute.key == STAKED_ATTRIBUTE {
            found_staked = true;
            require!(attribute.value == "true", ErrorCode::AssetNotStaked);
        } else if attribute.key == STAKED_AT_ATTRIBUTE {
            found_staked_at = true;
            staked_timestamp = parse_i64_attribute(attribute)?;

            // Calculate time since staked
            let mut staked_days = current_timestamp
                .checked_sub(staked_timestamp)
                .ok_or(ErrorCode::InvalidTimestamp)?;

            // Convert to days
            staked_days = staked_days
                .checked_div(super::utils::SECONDS_PER_DAY)
                .ok_or(ErrorCode::InvalidTimestamp)?;

            require!(
                staked_days >= ctx.accounts.config.freeze_period as i64,
                ErrorCode::FreezePeriodNotElapsed
            );
        } else if attribute.key == CLAIMED_AT_ATTRIBUTE {
            claimed_timestamp = parse_i64_attribute(attribute)?;
        } else {
            attributes_list.push(attribute.clone());
        }
    }

    require!(found_staked && found_staked_at, ErrorCode::AssetNotStaked);

    // Reset staking attributes
    attributes_list.push(Attribute {
        key: STAKED_ATTRIBUTE.to_string(),
        value: "false".to_string(),
    });

    attributes_list.push(Attribute {
        key: STAKED_AT_ATTRIBUTE.to_string(),
        value: "0".to_string(),
    });

    attributes_list.push(Attribute {
        key: CLAIMED_AT_ATTRIBUTE.to_string(),
        value: "0".to_string(),
    });

    // Prepare signing seeds for update authority
    let collection_key: Pubkey = ctx.accounts.collection.key();

    let signer_seeds: &[&[u8]; 3] = &[
        b"update_authority",
        collection_key.as_ref(),
        &[ctx.bumps.update_authority],
    ];

    // Update Attributes plugin
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

    update_collection_staked_count(
        ctx.accounts.collection.to_account_info(),
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.update_authority.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.mpl_core_program.to_account_info(),
        &[signer_seeds],
        -1,
    )?;

    // Thaw asset
    UpdatePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .payer(&ctx.accounts.owner.to_account_info())
        .authority(Some(&ctx.accounts.update_authority.to_account_info()))
        .system_program(&ctx.accounts.system_program.to_account_info())
        .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen: false }))
        .invoke_signed(&[signer_seeds])?;

    // Calculate rewards
    let reward_start = reward_start_timestamp(staked_timestamp, claimed_timestamp);
    let unclaimed_days = reward_days(current_timestamp, reward_start)?;
    let amount = reward_amount(
        unclaimed_days,
        ctx.accounts.config.rewards_bps,
        ctx.accounts.rewards_mint.decimals,
    )?;

    // Config PDA signer seeds
    let config_seeds: &[&[u8]; 3] = &[
        b"config",
        collection_key.as_ref(),
        &[ctx.accounts.config.bump],
    ];

    let config_signer_seeds: &[&[&[u8]]] = &[config_seeds];

    if amount > 0 {
        mint_to_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintToChecked {
                    mint: ctx.accounts.rewards_mint.to_account_info(),
                    to: ctx.accounts.user_rewards_ata.to_account_info(),
                    authority: ctx.accounts.config.to_account_info(),
                },
                config_signer_seeds,
            ),
            amount,
            ctx.accounts.rewards_mint.decimals,
        )?;
    }

    Ok(())
}
