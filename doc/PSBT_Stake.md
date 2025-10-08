# PSBT Stake Transaction

Transaction to stake LIQ runes and receive sLIQ runes in return.

## Required Inputs

0. **Canister UTXO(s) containing sLIQ runes** (amount to be transferred to user)
1. **User UTXO(s) containing LIQ runes** (amount being staked)
2. **User UTXO with BTC for transaction fees** (separate from rune UTXOs)

## Required Outputs

0. **Runestone Output (OP_RETURN):**
   - Value: 0 sats
   - Contains encoded runestone with edicts

1. **To Primary Pool Address (sLIQ Change):**
   - Value: 546 sats (dust)
   - Receives: sLIQ runes (change amount)
   - Note: Creates first UTXO when splitting sLIQ change to maintain healthy UTXO count

2. **[Optional] To Primary Pool Address (Additional sLIQ Change):**
   - Value: 546 sats (dust)
   - Receives: sLIQ runes (additional change amount)
   - Note: Creates second UTXO when splitting sLIQ change

3. **To Primary Pool Address:**
   - Value: 546 sats (dust)
   - Receives: LIQ runes (amount being staked)

4. **To User Address (LIQ Change):**
   - Value: 546 sats (dust)
   - Receives: LIQ runes (change amount)

5. **To User Address (sLIQ):**
   - Value: 546 sats (dust)
   - Receives: sLIQ runes (amount based on exchange rate)

6. **Change Output (BTC):**
   - Value: BTC change after fees and dust outputs
   - To: User address

## Runestone Edicts

- **Edict 0:** Transfer sLIQ (change) → Primary Pool. Output index 1.
- **[Optional] Edict 1:** Transfer sLIQ (additional change) → Primary Pool (if splitting). Output index 2.
- **Edict 2:** Transfer LIQ (staked amount) → Primary Pool. Output index 3.
- **Edict 3:** Transfer LIQ (change) → User. Output index 4.
- **Edict 4:** Transfer sLIQ → User. Output index 5.
- **Pointer:** Output index 1.

## Source Code References

- **Client-Side Construction:** `api-webapp/src/lib/psbt.ts` (Class: `RunePSBT`, Method: `build()`)
