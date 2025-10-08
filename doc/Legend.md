# PSBT Documentation Legend

**Terminology:**
- **User Wallet:** Address controlled by the end-user
- **Primary Pool:** Canister-controlled address for staking LIQ and holding sLIQ (Key Index 0)
- **Secondary Pool:** Canister-controlled address for holding LIQ pending withdrawal (Key Index 1)
- **LIQ:** Liquidium Rune
- **sLIQ:** Staked Liquidium Rune
- **BTC:** Bitcoin
- **(D):** Dust amount of BTC, typically 546 sats, accompanying rune transfers

**Common Elements:**
- **Runestone Output:** All PSBTs involving rune transfers include an OP_RETURN output carrying the runestone data (0 sats value)
- **Dust Value:** Outputs carrying runes typically have 546 sats BTC value
- **Fee Range:** Transaction fees expected between 500-100,000 sats
- **Client Construction:** PSBTs are built using `RunePSBT` class in `api-webapp/src/lib/psbt.ts`
