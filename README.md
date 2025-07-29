# Solana Anchor Staking Program

This is a Solana program written using the [Anchor framework](https://github.com/coral-xyz/anchor) that allows users to stake SPL tokens to earn periodic rewards and withdraw them after a configured staking period.

## Features

- Stake SPL token with a configurable minimum stake amount.
- Earn daily rewards based on a configurable percentage.
- Claim accrued rewards before the staking period ends.
- Withdraw stake plus any unclaimed rewards after the lock-in period.
- Admin can update platform configuration parameters.

## Program Accounts

### `PlatformInfo`
Stores global configuration and stats:
- Owner
- Token and token account
- Minimum stake
- Reward percentage (daily)
- Stake period (in seconds)
- Aggregated stats

### `StakeInfo`
Stores each stake's metadata:
- Amount staked
- Stake timestamp
- Last reward claim timestamp
- Total reward claimed

### `StakeCounter`
Tracks the latest stake ID per user.

## Instructions

### Initialize

```ts
program.methods.initialize(minStake, stakePeriod, rewardPercentage)
```

Initializes platform configuration and creates a PDA account for managing stake info.

### Stake

```ts
program.methods.stake(amount)
```

Stakes a given amount of tokens. A new `StakeInfo` account is created for every stake.

### Claim Reward

```ts
program.methods.claimReward(stakeId)
```

Claim accrued rewards before the end of the staking period.

### Withdraw

```ts
program.methods.withdraw(stakeId)
```

Withdraws the original stake amount and any remaining unclaimed rewards **after** the staking period ends.

### Update Platform Info

```ts
program.methods.updatePlatformInfo(minStake, rewardPercentage, stakePeriod)
```

Admin can update the staking configuration.

## Events

- `Staked`: emitted when a user stakes tokens.
- `RewardClaimed`: emitted when a reward is successfully claimed.
- `Withdraw`: emitted when a user withdraws stake.
- `PlatformUpdated`: emitted when the platform info is updated.

## Error Codes

- `InvalidAmount`
- `NotExpired`
- `AlreadyClaimed`
- `InvalidStakeAccount`
- `NoAvailReward`
- `InsufficientPlatformFunds`
- `Unauthorized`
- `InvalidRewardPercentage`

## Development

### Build

```bash
anchor build
```

### Test

```bash
anchor test
```

## License

MIT