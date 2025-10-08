# PSBT Withdraw Transaction

Transaction to withdraw LIQ runes from the secondary pool (retention) after the unstaking period has elapsed.

## Required Inputs

0. **Secondary Pool UTXO(s)** (from previous unstake transaction)
1. **User UTXO with BTC for transaction fees**

## Required Outputs

0. **Runestone Output (OP_RETURN):**
   - Value: 0 sats
   - Contains encoded runestone

1. **To User Address:**
   - Value: BTC value from secondary pool UTXOs
   - Receives: All LIQ runes from secondary pool (retention)

2. **Change Output (BTC):**
   - Value: BTC change after fees
   - To: User address

## Runestone Edicts

- **Pointer mechanism**: Transfers all LIQ runes to user address

## Source Code References

- **Client-Side Construction:** `api-webapp/src/app/api/withdraw/route.ts` (Function: `POST`, using `RunePSBT` class)
