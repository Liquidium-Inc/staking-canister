# PSBT Unstake Transaction

Transaction to unstake sLIQ runes and receive LIQ runes in return.

## Required Inputs

0. **Canister UTXO(s) containing LIQ runes** (amount to be returned to user)
1. **User UTXO(s) containing sLIQ runes** (amount being unstaked)
2. **User UTXO with BTC for transaction fees** (separate from rune UTXOs)

## Required Outputs

0. **Runestone Output (OP_RETURN):**
   - Value: 0 sats
   - Contains encoded runestone with edicts

1. **To Primary Pool Address (LIQ Change):**
   - Value: 546 sats (dust)
   - Receives: LIQ runes (change amount)
   - Note: Creates first UTXO when splitting sLIQ change to maintain healthy UTXO count

2. **[Optional] To Primary Pool Address (Additional LIQ Change):**
   - Value: 546 sats (dust)
   - Receives: LIQ runes (additional change amount)
   - Note: Creates second UTXO when splitting LIQ change

3. **To Primary Pool Address:**
   - Value: 546 sats (dust)
   - Receives: sLIQ runes (amount being unstaked)

4. **To User Address (sLIQ Change):**
   - Value: 546 sats (dust)
   - Receives: sLIQ runes (change amount)

5. **To Secondary Pool Address (Retention):**
   - Value: 546 sats (dust)
   - Receives: LIQ runes (amount based on exchange rate)

6. **Change Output (BTC):**
   - Value: BTC change after fees and dust outputs
   - To: User address

## Runestone Edicts

- **Edict 0:** Transfer sLIQ (unstaked amount) → Primary Pool. Output index 1.
- **[Optional] Edict 1:** Transfer LIQ (change from canister) → Primary Pool. Output index 2.
- **Edict 2:** Transfer LIQ (staked amount) → Primary Pool. Output index 3.
- **Edict 3:** Transfer sLIQ (change) → User. Output index 4.
- **Edict 4:** Transfer LIQ (unstaked amount) → Secondary Pool (Retention). Output index 5.
- **Pointer:** Output index 1.

## Source Code References

- **Client-Side Construction:** `api-webapp/src/app/api/unstake/route.ts` (Function: `POST`, using `RunePSBT` class)
