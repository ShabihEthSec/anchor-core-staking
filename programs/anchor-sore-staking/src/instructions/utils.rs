use anchor_lang::prelude::*;
use mpl_core::{
    accounts::{BaseAssetV1, BaseCollectionV1},
    fetch_plugin,
    instructions::{AddCollectionPluginV1CpiBuilder, UpdateCollectionPluginV1CpiBuilder},
    types::{Attribute, Attributes, Plugin, PluginAuthority, PluginType},
};

use crate::error::ErrorCode;

pub const SECONDS_PER_DAY: i64 = 86400;
pub const STAKED_ATTRIBUTE: &str = "staked";
pub const STAKED_AT_ATTRIBUTE: &str = "staked_at";
pub const CLAIMED_AT_ATTRIBUTE: &str = "claimed_at";
pub const STAKED_NFTS_ATTRIBUTE: &str = "staked_nfts";

pub fn fetch_asset_attributes(asset: &AccountInfo) -> Option<Attributes> {
    fetch_plugin::<BaseAssetV1, Attributes>(asset, PluginType::Attributes)
        .ok()
        .map(|(_, attrs, _)| attrs)
}

pub fn fetch_collection_attributes(collection: &AccountInfo) -> Option<Attributes> {
    fetch_plugin::<BaseCollectionV1, Attributes>(collection, PluginType::Attributes)
        .ok()
        .map(|(_, attrs, _)| attrs)
}

pub fn parse_i64_attribute(attribute: &Attribute) -> Result<i64> {
    attribute
        .value
        .parse::<i64>()
        .map_err(|_| ErrorCode::InvalidTimestamp.into())
}

pub fn reward_start_timestamp(staked_timestamp: i64, claimed_timestamp: i64) -> i64 {
    if claimed_timestamp > staked_timestamp {
        claimed_timestamp
    } else {
        staked_timestamp
    }
}

pub fn reward_days(current_timestamp: i64, start_timestamp: i64) -> Result<i64> {
    current_timestamp
        .checked_sub(start_timestamp)
        .ok_or(ErrorCode::InvalidTimestamp)?
        .checked_div(SECONDS_PER_DAY)
        .ok_or(ErrorCode::InvalidTimestamp.into())
}

pub fn reward_amount(days: i64, rewards_bps: u16, decimals: u8) -> Result<u64> {
    (days as u64)
        .checked_mul(rewards_bps as u64)
        .ok_or(ErrorCode::InvalidRewardsBps)?
        .checked_mul(10u64.pow(decimals as u32))
        .ok_or(ErrorCode::InvalidRewardsBps)?
        .checked_div(10000u64)
        .ok_or(ErrorCode::InvalidRewardsBps.into())
}

pub fn update_collection_staked_count<'info>(
    collection: AccountInfo<'info>,
    payer: AccountInfo<'info>,
    authority: AccountInfo<'info>,
    system_program: AccountInfo<'info>,
    mpl_core_program: AccountInfo<'info>,
    signer_seeds: &[&[&[u8]]],
    delta: i64,
) -> Result<()> {
    let attributes_fetched = fetch_collection_attributes(&collection);
    let mut attributes_list: Vec<Attribute> = Vec::new();
    let mut current_count: u64 = 0;
    let mut found_count = false;

    if let Some(attributes) = &attributes_fetched {
        attributes_list.reserve(attributes.attribute_list.len());

        for attribute in &attributes.attribute_list {
            if attribute.key == STAKED_NFTS_ATTRIBUTE {
                current_count = attribute
                    .value
                    .parse::<u64>()
                    .map_err(|_| ErrorCode::InvalidStakedCount)?;
                found_count = true;
            } else {
                attributes_list.push(attribute.clone());
            }
        }
    }

    let next_count = if delta.is_negative() {
        current_count
            .checked_sub(delta.unsigned_abs())
            .ok_or(ErrorCode::InvalidStakedCount)?
    } else {
        current_count
            .checked_add(delta as u64)
            .ok_or(ErrorCode::InvalidStakedCount)?
    };

    attributes_list.push(Attribute {
        key: STAKED_NFTS_ATTRIBUTE.to_string(),
        value: next_count.to_string(),
    });

    if attributes_fetched.is_none() || !found_count {
        AddCollectionPluginV1CpiBuilder::new(&mpl_core_program)
            .collection(&collection)
            .payer(&payer)
            .authority(Some(&authority))
            .system_program(&system_program)
            .plugin(Plugin::Attributes(Attributes {
                attribute_list: attributes_list,
            }))
            .init_authority(PluginAuthority::UpdateAuthority)
            .invoke_signed(signer_seeds)?;
    } else {
        UpdateCollectionPluginV1CpiBuilder::new(&mpl_core_program)
            .collection(&collection)
            .payer(&payer)
            .authority(Some(&authority))
            .system_program(&system_program)
            .plugin(Plugin::Attributes(Attributes {
                attribute_list: attributes_list,
            }))
            .invoke_signed(signer_seeds)?;
    }

    Ok(())
}
