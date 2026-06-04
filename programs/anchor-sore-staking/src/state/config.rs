use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub rewards_bps: u16,
    pub freeze_period: u16,
    pub rewards_bump: u8,
    pub bump: u8,
}
