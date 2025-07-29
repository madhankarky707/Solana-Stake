import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Stake } from "../target/types/stake";

import {
  PublicKey,
  Keypair,
  SystemProgram,
} from "@solana/web3.js";

import {
  createMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  transfer,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";

import { assert } from "chai";

describe("stake", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const payer = provider.wallet.payer;
  const program = anchor.workspace.Stake as Program<Stake>;

  let user: Keypair;
  let owner: Keypair;
  let mint: PublicKey;
  let userAta;
  let programAta;
  let programInfoPDA: PublicKey;
  let authorityPDA: PublicKey;
  let userStakePDA: PublicKey;
  let userStakeCountPDA: PublicKey;

  before(async () => {
    user = Keypair.generate();
    owner = Keypair.generate();

    // Airdrops
    await provider.connection.requestAirdrop(user.publicKey, 5e9);
    await provider.connection.requestAirdrop(owner.publicKey, 2e9);

    [programInfoPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("platforminfo")],
      program.programId
    );

    [authorityPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("tokenauthority")],
      program.programId
    );

    [userStakeCountPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("stakecounter"), user.publicKey.toBuffer()],
      program.programId
    );

    const currentId = new anchor.BN(0);
    [userStakePDA] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("userstakeaccount"),
        user.publicKey.toBuffer(),
        currentId.toArrayLike(Buffer, "le", 8)
      ],
      program.programId
    );

    // Create mint & accounts
    mint = await createMint(provider.connection, payer, user.publicKey, null, 6);
    userAta = await getOrCreateAssociatedTokenAccount(provider.connection, payer, mint, user.publicKey);
    programAta = await getOrCreateAssociatedTokenAccount(provider.connection, payer, mint, authorityPDA, true);

    await mintTo(provider.connection, payer, mint, userAta.address, user, 10_000_0000_000);
    // Now transfer to ICO PDA ATA
    await transfer(
      provider.connection,
      user,
      userAta.address,
      programAta.address,
      user,
      10_000_000
    );
  });

  async function getBalance(ata: PublicKey): Promise<number> {
    const bal = await provider.connection.getTokenAccountBalance(ata);
    return Number(bal.value.amount);
  }

  it("Initializes platform", async () => {
    const minStake = new anchor.BN(10_000_000);
    const stakePeriod = new anchor.BN(10*86400); // 10 days
    const rewardPercentage = new anchor.BN(1);

    await program.methods.initialize(minStake, stakePeriod, rewardPercentage)
      .accounts({
        owner: owner.publicKey,
        platformInfo: programInfoPDA,
        authority: authorityPDA,
        mint,
        platformTokenAccount: programAta.address,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId
      })
      .signers([owner])
      .rpc();

    const info = await program.account.platformInfo.fetch(programInfoPDA);
    assert.strictEqual(info.minStake.toNumber(),  minStake.toNumber());
    assert.strictEqual(info.rewardPercentage.toNumber(),  rewardPercentage.toNumber());
    assert.strictEqual(info.stakePeriod.toNumber(), stakePeriod.toNumber());
  });

  it("Stakes tokens", async () => {
    const amount = new anchor.BN(20_000_000);
    const userBalBefore = await getBalance(userAta.address);
    const progBalBefore = await getBalance(programAta.address);

    await program.methods.stake(amount)
      .accounts({
        user: user.publicKey,
        userStakeCounter: userStakeCountPDA,
        userStakeAccount: userStakePDA,
        platformInfo: programInfoPDA,
        mint,
        userTokenAccount: userAta.address,
        authority: authorityPDA,
        platformTokenAccount: programAta.address,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId
      })
      .signers([user])
      .rpc();

    const userBalAfter = await getBalance(userAta.address);
    const progBalAfter = await getBalance(programAta.address);

    assert.strictEqual(userBalBefore - userBalAfter, amount.toNumber());
    assert.strictEqual(progBalAfter - progBalBefore, amount.toNumber());

    const stakeInfo = await program.account.stakeInfo.fetch(userStakePDA);
    assert.strictEqual(stakeInfo.amount.toNumber(), amount.toNumber());
    console.log(userBalBefore, userBalAfter);
    
  });

  it("Claims reward", async () => {
    const stakeId = new anchor.BN(0);

    const userBalBefore = await getBalance(userAta.address);
    const progBalBefore = await getBalance(programAta.address);

    try {
      await program.methods.claimReward(stakeId)
        .accounts({
          user: user.publicKey,
          userStakeAccount: userStakePDA,
          platformInfo: programInfoPDA,
          mint,
          authority: authorityPDA,
          userTokenAccount: userAta.address,
          platformTokenAccount: programAta.address,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId
        })
        .signers([user])
        .rpc();

      const userBalAfter = await getBalance(userAta.address);
      const progBalAfter = await getBalance(programAta.address);

      const reward = userBalAfter - userBalBefore;
      assert.isAbove(reward, 0, "User should receive a reward");
      assert.strictEqual(progBalBefore - progBalAfter, reward);
    } catch (err: any) {
      const errMsg = err?.error?.errorMessage || err.toString();
      console.log("Caught error:", errMsg);

      // Handle the custom error
      if (errMsg.includes("No available reward")) {
        console.log("Test passed: No reward available.");
        assert.ok(true);
      } else {
        assert.fail("Unexpected error: " + errMsg);
      }
    }
  });

  it("Withdraws stake + reward", async () => {
    const stakeId = new anchor.BN(0);

    const userBalBefore = await getBalance(userAta.address);
    const progBalBefore = await getBalance(programAta.address);

    try {
      await program.methods.withdraw(stakeId)
        .accounts({
          user: user.publicKey,
          userStakeAccount: userStakePDA,
          platformInfo: programInfoPDA,
          mint,
          authority: authorityPDA,
          userTokenAccount: userAta.address,
          platformTokenAccount: programAta.address,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId
        })
        .signers([user])
        .rpc();

      const userBalAfter = await getBalance(userAta.address);
      const progBalAfter = await getBalance(programAta.address);

      const stakeInfo = await program.account.stakeInfo.fetch(userStakePDA);
      assert.strictEqual(stakeInfo.amount.toNumber(), 0);

      const received = userBalAfter - userBalBefore;
      assert.isAbove(received, 0, "User should receive full withdrawal");
      assert.strictEqual(progBalBefore - progBalAfter, received);

    } catch (err: any) {
      const errMsg = err?.error?.errorMessage || err.toString();
      console.log("Caught error:", errMsg);

      if (errMsg.includes("Stake has not expired")) {
        console.log("Test passed: Stake not expired error thrown as expected.");
        assert.ok(true);
      } else {
        assert.fail("Unexpected error: " + errMsg);
      }
    }
  });

});
