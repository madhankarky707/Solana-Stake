use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("3ycLKuh79RpM8WQ4pBJbMdHkME68mtBPtpo74T37QMNT");

#[program]
pub mod stake {
    use super::*;

    pub const DIVISOR_BASE: u64 = 100;
    pub const DAY_EPOCH: i64 = 86400;

    pub fn initialize(
        ctx: Context<Initialize>,
        min_stake: u64,
        stake_period: i64,
        reward_percentage: u64
    ) -> Result<()> {
        let platform_info = &mut ctx.accounts.platform_info;
        platform_info.owner = ctx.accounts.owner.key();
        platform_info.token = ctx.accounts.mint.key();
        platform_info.platform_token_account = ctx.accounts.platform_token_account.key();
        platform_info.min_stake = min_stake;
        platform_info.reward_percentage = reward_percentage;
        platform_info.stake_period = stake_period;
        platform_info.total_staked = 0;
        platform_info.total_withdrawn = 0;
        platform_info.total_reward_claimed = 0;

        Ok(())
    }

    pub fn stake(ctx: Context<Stake>, amount : u64) -> Result<()> {
        let platform_info = &mut ctx.accounts.platform_info;
        require_keys_eq!(ctx.accounts.platform_token_account.key(), platform_info.platform_token_account);
        require!(amount >= platform_info.min_stake, CustomError::InvalidAmount);

        let user_stake_account = &mut ctx.accounts.user_stake_account;
        let user_stake_counter = &mut ctx.accounts.user_stake_counter;
        
        let clock = Clock::get()?; // gets latest time
        user_stake_account.amount = amount;
        user_stake_account.stake_on = clock.unix_timestamp;
        user_stake_account.last_claim = clock.unix_timestamp;
        user_stake_account.reward_claimed = 0;
        platform_info.total_staked += amount;
        let stake_id:u64 = user_stake_counter.current_id;
        user_stake_counter.current_id += 1;

        let cpi_accounts = Transfer {
            from: ctx.accounts.user_token_account.to_account_info(),
            to: ctx.accounts.platform_token_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };

        let cpi_program = ctx.accounts.token_program.to_account_info();

        token::transfer(
            CpiContext::new(cpi_program, cpi_accounts),
            amount,
        )?;

        emit!(Staked {
            user: ctx.accounts.user.key(),
            stake_id: stake_id,
            amount: amount,
        });

        Ok(())
    }

    pub fn claim_reward(ctx: Context<Claim>, stake_id: u64) -> Result<()> {
        let (expected_pda, _bump) = Pubkey::find_program_address(
            &[b"userstakeaccount", ctx.accounts.user.key().as_ref(), &stake_id.to_le_bytes()],
            ctx.program_id,
        );

        require_keys_eq!(
            expected_pda,
            ctx.accounts.user_stake_account.key(),
            CustomError::InvalidStakeAccount
        );

        let platform_info = &mut ctx.accounts.platform_info;
        let user_stake_account = &mut ctx.accounts.user_stake_account;

        require!(
            user_stake_account.amount > 0,
            CustomError::InvalidAmount
        );

        let last_claim = user_stake_account.last_claim;
        let stake_end_time = user_stake_account.stake_on + platform_info.stake_period;
        require!(
            last_claim < stake_end_time,
            CustomError::AlreadyClaimed
        );

        let total_reward: u64 = calculate_reward(user_stake_account, platform_info)?;

        require!(
            total_reward != 0,
            CustomError::NoAvailReward
        );

        require!(
            ctx.accounts.platform_token_account.amount >= total_reward,
            CustomError::InsufficientPlatformFunds
        );

        // Transfer SPL tokens using PDA as signer
        let seeds = &[b"tokenauthority".as_ref(), &[ctx.bumps.authority]];
        let signer = &[&seeds[..]];

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.platform_token_account.to_account_info(),
                    to: ctx.accounts.user_token_account.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
                signer,
            ),
            total_reward,
        )?;

        emit!(RewardClaimed {
            user: ctx.accounts.user.key(),
            stake_id: stake_id,
            amount: total_reward,
        });

        Ok(())
    }

    pub fn withdraw(ctx: Context<Withdrawal>, stake_id: u64) -> Result<()> {
        let (expected_pda, _bump) = Pubkey::find_program_address(
            &[b"userstakeaccount", ctx.accounts.user.key().as_ref(), &stake_id.to_le_bytes()],
            ctx.program_id,
        );

        require_keys_eq!(
            expected_pda,
            ctx.accounts.user_stake_account.key(),
            CustomError::InvalidStakeAccount
        );

        let platform_info = &mut ctx.accounts.platform_info;
        let user_stake_account = &mut ctx.accounts.user_stake_account;     
        
        require!(
            user_stake_account.amount > 0,
            CustomError::InvalidAmount
        );

        let clock = Clock::get()?;
        let current_time = clock.unix_timestamp;
        let stake_end_time = user_stake_account.stake_on + platform_info.stake_period;

        require!(
            stake_end_time <= current_time,
            CustomError::NotExpired
        );

        let total_reward: u64 = calculate_reward(user_stake_account, platform_info)?;

        // Transfer SPL tokens using PDA as signer
        let seeds = &[b"tokenauthority".as_ref(), &[ctx.bumps.authority]];
        let signer = &[&seeds[..]];

        let mut amount = user_stake_account.amount;
        platform_info.total_withdrawn += amount;
        amount = amount + total_reward;
        user_stake_account.amount = 0;


        require!(
            ctx.accounts.platform_token_account.amount >= amount,
            CustomError::InsufficientPlatformFunds
        );

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.platform_token_account.to_account_info(),
                    to: ctx.accounts.user_token_account.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
                signer,
            ),
            amount,
        )?;

        emit!(Withdraw {
            user: ctx.accounts.user.key(),
            stake_id: stake_id,
            amount: amount,
        });

        Ok(())
    }

    pub fn update_platform_info(
        ctx: Context<UpdatePlatformInfo>,
        min_stake: u64,
        reward_percentage: u64,
        stake_period: i64,
    ) -> Result<()> {
        let platform_info = &mut ctx.accounts.platform_info;

        // Only the owner should be able to update
        require_keys_eq!(
            ctx.accounts.owner.key(),
            platform_info.owner,
            CustomError::Unauthorized
        );

        require!(reward_percentage <= DIVISOR_BASE, CustomError::InvalidRewardPercentage); // Max 1000% (10x)

        platform_info.min_stake = min_stake;
        platform_info.reward_percentage = reward_percentage;
        platform_info.stake_period = stake_period;

        emit!(PlatformUpdated {
            min_stake,
            reward_percentage,
            stake_period
        });

        Ok(())
    }
}

fn calculate_reward(user_stake_account : &mut StakeInfo, platform_info : &mut PlatformInfo) -> Result<u64> {
    let clock = Clock::get()?;
    let mut current_time = clock.unix_timestamp;

    let last_claim = user_stake_account.last_claim;
    let stake_end_time = user_stake_account.stake_on + platform_info.stake_period;

    if current_time > stake_end_time {
        current_time = stake_end_time;
    }

    let total_days: i64 = (current_time - last_claim) / DAY_EPOCH;
    let mut total_reward:u64 = 0;

    if total_days != 0 {
        let reward = user_stake_account.amount * platform_info.reward_percentage / DIVISOR_BASE;
        total_reward = reward * (total_days as u64);
        user_stake_account.last_claim += DAY_EPOCH * total_days;
        user_stake_account.reward_claimed += total_reward;
        platform_info.total_reward_claimed += total_reward;
    }
    
    Ok(total_reward)
}



#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        init,
        payer = owner,
        space = 152,
        seeds = [b"platforminfo"],
        bump
    )]
    pub platform_info: Account<'info, PlatformInfo>,
    /// CHECK: PDA authority to sign for token transfer
    #[account(
        mut,
        seeds = [b"tokenauthority"],
        bump
    )]
    pub authority: AccountInfo<'info>,
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    #[account(
        mut,
        token::mint = mint,
        token::authority = authority
    )]
    pub platform_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
pub struct Stake<'info> {
    #[account(mut)]
    pub user : Signer <'info>,
     #[account(
        init_if_needed,
        payer = user,
        space = 8 + 8, // StakeCounter size
        seeds = [b"stakecounter", user.key().as_ref()],
        bump
    )]
    pub user_stake_counter: Account<'info, StakeCounter>,
    #[account(
        init_if_needed,
        payer = user,
        space = 8 + 8 + 8 + 8 + 8 + 8,
        seeds = [b"userstakeaccount", user.key().as_ref(), user_stake_counter.current_id.to_le_bytes().as_ref()],
        bump
    )]
    pub user_stake_account : Account<'info, StakeInfo>,
     #[account(
        mut,
        seeds = [b"platforminfo"],
        bump
    )]
    pub platform_info: Account<'info, PlatformInfo>,
    #[account(mut)]
    pub mint : Account<'info, Mint>,
    #[account(
        mut,
        token::mint = mint,
        token::authority = user
    )] 
    pub user_token_account : Account<'info, TokenAccount>,
    /// CHECK: PDA authority to sign for token transfer
    #[account(
        mut,
        seeds = [b"tokenauthority"],
        bump
    )]
    pub authority: AccountInfo<'info>,
    #[account(
        mut,
        token::mint = mint,
        token::authority = authority
    )]
    pub platform_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Claim<'info> {
    #[account(mut)]
    pub user : Signer <'info>,
    #[account(
        mut,
    )]
    pub user_stake_account : Account<'info, StakeInfo>,
    #[account(
        mut,
        seeds = [b"platforminfo"],
        bump
    )]
    pub platform_info: Account<'info, PlatformInfo>,
    #[account(mut)]
    pub mint : Account<'info, Mint>,
    /// CHECK: PDA authority to sign for token transfer
    #[account(
        mut,
        seeds = [b"tokenauthority"],
        bump
    )]
    pub authority: AccountInfo<'info>,
    #[account(
        mut,
        token::mint = mint,
        token::authority = user
    )] 
    pub user_token_account : Account<'info, TokenAccount>,
    #[account(
        mut,
        token::mint = mint,
        token::authority = authority
    )]
    pub platform_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Withdrawal<'info> {
     #[account(mut)]
    pub user : Signer <'info>,
    #[account(
        mut
    )]
    pub user_stake_account : Account<'info, StakeInfo>,
    #[account(
        mut,
        seeds = [b"platforminfo"],
        bump
    )]
    pub platform_info: Account<'info, PlatformInfo>,
    #[account(mut)]
    pub mint : Account<'info, Mint>,
    /// CHECK: PDA authority to sign for token transfer
    #[account(
        mut,
        seeds = [b"tokenauthority"],
        bump
    )]
    pub authority: AccountInfo<'info>,
    #[account(
        mut,
        token::mint = mint,
        token::authority = user
    )] 
    pub user_token_account : Account<'info, TokenAccount>,
    #[account(
        mut,
        token::mint = mint,
        token::authority = authority
    )]
    pub platform_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdatePlatformInfo<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds = [b"platforminfo"],
        bump
    )]
    pub platform_info: Account<'info, PlatformInfo>,
}

#[account]
pub struct StakeInfo {
    pub amount : u64,
    pub stake_on : i64,
    pub last_claim : i64,
    pub reward_claimed : u64,
}

#[account]
pub struct StakeCounter {
    pub current_id: u64,
}

#[account]
pub struct PlatformInfo {
    pub owner: Pubkey,
    pub token: Pubkey,
    pub platform_token_account: Pubkey,
    pub min_stake: u64,
    pub reward_percentage : u64,
    pub stake_period: i64,
    pub total_staked: u64,
    pub total_withdrawn: u64,
    pub total_reward_claimed: u64,
}

#[event]
pub struct Staked {
    pub user: Pubkey,
    pub stake_id: u64,
    pub amount: u64
}

#[event]
pub struct RewardClaimed {
    pub user: Pubkey,
    pub stake_id: u64,
    pub amount: u64,
}

#[event]
pub struct Withdraw {
    pub user: Pubkey,
    pub stake_id: u64,
    pub amount: u64
}

#[event]
pub struct PlatformUpdated {
    pub min_stake: u64,
    pub reward_percentage: u64,
    pub stake_period: i64,
}

#[error_code]
pub enum CustomError {
    #[msg("Amount must be greater than zero.")]
    InvalidAmount,
    #[msg("Stake has not expired")]
    NotExpired,
    #[msg("Reward already claimed")]
    AlreadyClaimed,
    #[msg("Invalid stake account address derived")]
    InvalidStakeAccount,
    #[msg("No available reward")]
    NoAvailReward,
    #[msg("Platform does not have enough tokens")]
    InsufficientPlatformFunds,
    #[msg("Unauthorized action")]
    Unauthorized,
    #[msg("Reward percentage too high")]
    InvalidRewardPercentage,
}